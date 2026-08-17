use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use agent::types::AssistantMessagePart;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use futures::StreamExt;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::*;

#[test]
fn flag_is_on_accepts_only_documented_truthy_values() {
    for value in ["1", "true", "TRUE", "On", " on "] {
        assert!(flag_is_on(value), "{value} should enable the cutover");
    }
    for value in ["", "0", "false", "off", "yes", "enabled", "t"] {
        assert!(!flag_is_on(value), "{value} must not enable the cutover");
    }
}

#[test]
fn unset_flag_falls_back_to_agent_loop_even_when_base_url_is_set() {
    assert!(resolve_flue_chat_target(None, Some("http://127.0.0.1:5173")).is_none());
    assert!(resolve_flue_chat_target(Some("0"), Some("http://127.0.0.1:5173")).is_none());
    assert!(resolve_flue_chat_target(Some("false"), Some("http://127.0.0.1:5173")).is_none());
}

#[test]
fn flag_on_without_base_url_falls_back_to_agent_loop() {
    assert!(resolve_flue_chat_target(Some("1"), None).is_none());
    assert!(resolve_flue_chat_target(Some("true"), Some("")).is_none());
    assert!(resolve_flue_chat_target(Some("on"), Some("   ")).is_none());
}

#[test]
fn flag_on_with_invalid_base_url_falls_back_to_agent_loop() {
    assert!(resolve_flue_chat_target(Some("1"), Some("not a url")).is_none());
}

#[test]
fn flag_on_with_base_url_enables_flue() {
    let target = resolve_flue_chat_target(Some("1"), Some("http://127.0.0.1:5173/"))
        .expect("cutover should be on");
    assert_eq!(target.base_url.as_str(), "http://127.0.0.1:5173/");
}

#[test]
fn map_text_and_reasoning_deltas() {
    let mut names = HashMap::new();
    let text = serde_json::from_value::<ConversationStreamChunk>(json!({
        "type": "message-delta",
        "conversationId": "c",
        "messageId": "m",
        "kind": "text",
        "delta": "Hi",
        "position": { "batch": 1, "index": 0 }
    }))
    .unwrap();
    assert_eq!(
        map_update_chunk(&text, &mut names),
        Some(AssistantMessagePart::Text { text: "Hi".into() })
    );

    let thinking = serde_json::from_value::<ConversationStreamChunk>(json!({
        "type": "message-delta",
        "kind": "reasoning",
        "delta": "hmm",
        "position": { "batch": 1, "index": 1 }
    }))
    .unwrap();
    assert_eq!(
        map_update_chunk(&thinking, &mut names),
        Some(AssistantMessagePart::Thinking {
            thinking: "hmm".into()
        })
    );

    let empty = serde_json::from_value::<ConversationStreamChunk>(json!({
        "type": "message-delta",
        "kind": "text",
        "delta": "",
        "position": { "batch": 1, "index": 2 }
    }))
    .unwrap();
    assert_eq!(map_update_chunk(&empty, &mut names), None);
}

#[test]
fn map_tool_lifecycle_and_ignore_other_chunks() {
    let mut names = HashMap::new();
    let input = serde_json::from_value::<ConversationStreamChunk>(json!({
        "type": "tool-input",
        "conversationId": "c",
        "messageId": "m",
        "toolCallId": "call-1",
        "toolName": "search_documents",
        "input": { "q": "onboarding" },
        "position": { "batch": 2, "index": 0 }
    }))
    .unwrap();
    assert_eq!(
        map_update_chunk(&input, &mut names),
        Some(AssistantMessagePart::ToolCall {
            name: "search_documents".into(),
            json: json!({ "q": "onboarding" }),
            id: "call-1".into(),
        })
    );

    let output = serde_json::from_value::<ConversationStreamChunk>(json!({
        "type": "tool-output",
        "toolCallId": "call-1",
        "output": { "hits": 1 },
        "position": { "batch": 2, "index": 1 }
    }))
    .unwrap();
    assert_eq!(
        map_update_chunk(&output, &mut names),
        Some(AssistantMessagePart::ToolCallResponseJson {
            name: "search_documents".into(),
            json: json!({ "hits": 1 }),
            id: "call-1".into(),
        })
    );

    let err = serde_json::from_value::<ConversationStreamChunk>(json!({
        "type": "tool-output-error",
        "toolCallId": "call-1",
        "errorText": "boom",
        "position": { "batch": 2, "index": 2 }
    }))
    .unwrap();
    assert_eq!(
        map_update_chunk(&err, &mut names),
        Some(AssistantMessagePart::ToolCallErr {
            name: "search_documents".into(),
            description: "boom".into(),
            id: "call-1".into(),
        })
    );

    let ignored = serde_json::from_value::<ConversationStreamChunk>(json!({
        "type": "message-started",
        "conversationId": "c",
        "messageId": "m",
        "position": { "batch": 2, "index": 3 }
    }))
    .unwrap();
    assert_eq!(map_update_chunk(&ignored, &mut names), None);
}

struct MockFlue {
    posts: AtomicUsize,
    gets: AtomicUsize,
    aborts: AtomicUsize,
    last_body: Mutex<Option<Value>>,
    abort_requested: AtomicBool,
    hang_until_abort: AtomicBool,
    aborted: tokio::sync::Notify,
}

impl Default for MockFlue {
    fn default() -> Self {
        Self {
            posts: AtomicUsize::new(0),
            gets: AtomicUsize::new(0),
            aborts: AtomicUsize::new(0),
            last_body: Mutex::new(None),
            abort_requested: AtomicBool::new(false),
            hang_until_abort: AtomicBool::new(false),
            aborted: tokio::sync::Notify::new(),
        }
    }
}

fn offset_headers(offset: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(STREAM_NEXT_OFFSET, HeaderValue::from_static(offset));
    headers.insert("stream-up-to-date", HeaderValue::from_static("true"));
    headers
}

async fn admit(
    State(state): State<Arc<MockFlue>>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    state.posts.fetch_add(1, Ordering::SeqCst);
    *state.last_body.lock().await = Some(body);
    let mut headers = HeaderMap::new();
    headers.insert(
        STREAM_NEXT_OFFSET,
        HeaderValue::from_static("0000000000000000_0000000000000001"),
    );
    (
        StatusCode::ACCEPTED,
        headers,
        Json(json!({
            "streamUrl": format!("/agents/super-agent/{id}"),
            "offset": "0000000000000000_0000000000000001",
            "submissionId": "sub-1",
            "uid": "uid-1",
        })),
    )
}

async fn updates(State(state): State<Arc<MockFlue>>, Path(id): Path<String>) -> impl IntoResponse {
    let n = state.gets.fetch_add(1, Ordering::SeqCst);
    if n == 0 {
        return (
            StatusCode::OK,
            offset_headers("0000000000000000_0000000000000002"),
            Json(json!([{
                "type": "message-delta",
                "conversationId": id,
                "messageId": "m1",
                "kind": "text",
                "delta": "hello from flue",
                "position": { "batch": 2, "index": 0 }
            }])),
        );
    }
    if state.hang_until_abort.load(Ordering::SeqCst)
        && !state.abort_requested.load(Ordering::SeqCst)
    {
        state.aborted.notified().await;
    }
    let outcome = if state.abort_requested.load(Ordering::SeqCst) {
        "aborted"
    } else {
        "completed"
    };
    (
        StatusCode::OK,
        offset_headers("0000000000000000_0000000000000003"),
        Json(json!([{
            "type": "submission-settled",
            "conversationId": id,
            "submissionId": "sub-1",
            "outcome": outcome,
            "position": { "batch": 3, "index": 0 }
        }])),
    )
}

async fn abort_handler(State(state): State<Arc<MockFlue>>) -> impl IntoResponse {
    state.aborts.fetch_add(1, Ordering::SeqCst);
    state.abort_requested.store(true, Ordering::SeqCst);
    state.aborted.notify_waiters();
    Json(json!({ "aborted": true }))
}

async fn spawn_mock() -> (Url, Arc<MockFlue>) {
    let state = Arc::new(MockFlue::default());
    let app = Router::new()
        .route("/agents/super-agent/{id}", post(admit).get(updates))
        .route("/agents/super-agent/{id}/abort", post(abort_handler))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock flue");
    let addr = listener.local_addr().expect("mock flue addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("mock flue serve");
    });
    (
        Url::parse(&format!("http://{addr}")).expect("mock flue url"),
        state,
    )
}

fn collect_parts(events: &[FlueTurnEvent]) -> Vec<AssistantMessagePart> {
    events
        .iter()
        .filter_map(|event| match event {
            FlueTurnEvent::Part(part) => Some(part.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn flag_off_never_calls_flue() {
    let (base_url, state) = spawn_mock().await;
    let target = resolve_flue_chat_target(None, Some(base_url.as_str()));
    assert!(target.is_none(), "flag off must not build a Flue target");
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(state.posts.load(Ordering::SeqCst), 0);
    assert_eq!(state.gets.load(Ordering::SeqCst), 0);
    assert_eq!(state.aborts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn stream_posts_user_body_and_maps_assistant_text() {
    let (base_url, state) = spawn_mock().await;
    let target = resolve_flue_chat_target(Some("true"), Some(base_url.as_str()))
        .expect("flag on with base url");
    let client = FlueChatClient::new(&target);
    let mut stream = Box::pin(stream_flue_turn(
        client,
        "chat-123".into(),
        "What is onboarding?".into(),
        CancellationToken::new(),
    ));
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event);
    }

    assert_eq!(state.posts.load(Ordering::SeqCst), 1);
    assert!(state.gets.load(Ordering::SeqCst) >= 1);
    assert_eq!(state.aborts.load(Ordering::SeqCst), 0);
    let body = state.last_body.lock().await.clone().expect("posted body");
    assert_eq!(
        body,
        json!({ "kind": "user", "body": "What is onboarding?" })
    );
    assert_eq!(
        collect_parts(&events),
        vec![AssistantMessagePart::Text {
            text: "hello from flue".into()
        }]
    );
    assert!(matches!(
        events.last(),
        Some(FlueTurnEvent::Finished { cancelled: false })
    ));
}

#[tokio::test]
async fn cancel_posts_abort() {
    let (base_url, state) = spawn_mock().await;
    let target = resolve_flue_chat_target(Some("on"), Some(base_url.as_str())).unwrap();
    let client = FlueChatClient::new(&target);
    let cancel = CancellationToken::new();
    cancel.cancel();
    let mut stream = Box::pin(stream_flue_turn(
        client,
        "chat-cancel".into(),
        "stop".into(),
        cancel,
    ));
    let mut finished = None;
    while let Some(event) = stream.next().await {
        if let FlueTurnEvent::Finished { cancelled } = event {
            finished = Some(cancelled);
        }
    }
    assert_eq!(finished, Some(true));
    assert_eq!(state.aborts.load(Ordering::SeqCst), 0);
    assert_eq!(state.posts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancel_after_admission_posts_abort() {
    let (base_url, state) = spawn_mock().await;
    state.hang_until_abort.store(true, Ordering::SeqCst);
    let target = resolve_flue_chat_target(Some("1"), Some(base_url.as_str())).unwrap();
    let client = FlueChatClient::new(&target);
    let cancel = CancellationToken::new();
    let mut stream = Box::pin(stream_flue_turn(
        client,
        "chat-abort".into(),
        "go".into(),
        cancel.clone(),
    ));
    let first = stream.next().await;
    assert!(matches!(first, Some(FlueTurnEvent::Part(_))));
    cancel.cancel();
    let mut saw_finished = false;
    while let Some(event) = stream.next().await {
        if let FlueTurnEvent::Finished { cancelled } = event {
            saw_finished = true;
            assert!(cancelled);
        }
    }
    assert!(saw_finished);
    assert!(state.aborts.load(Ordering::SeqCst) >= 1);
    assert_eq!(state.posts.load(Ordering::SeqCst), 1);
}
