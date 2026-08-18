//! Outbound HTTP adapter for the gated DCS → Flue chat cutover.
//!
//! This module owns env-flag parsing and the Flue conversation wire protocol
//! (`POST /agents/super-agent/{id}`, `GET ?view=updates`, `POST .../abort`).
//! The chat handler maps [`FlueTurnEvent::Part`] values into existing
//! [`crate::model::stream::ChatStream`] envelopes so the web UI is unchanged.
//!
//! Off by default: unset `FLUENT_DCS_CHAT` (or a missing `FLUENT_BASE_URL`)
//! leaves the frozen [`agent::AgentLoop`] path in place.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use agent::types::AssistantMessagePart;
use async_stream::stream;
use futures::Stream;
use macro_env_var::maybe_env_var;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use url::Url;

maybe_env_var! {
    struct FluentDcsChat;
}

maybe_env_var! {
    struct FluentBaseUrl;
}

const IDLE_TIMEOUT: Duration = Duration::from_secs(3 * 60);
const STREAM_NEXT_OFFSET: &str = "stream-next-offset";

/// Resolved Flue origin for one DCS chat turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlueChatTarget {
    /// Service origin, e.g. `http://127.0.0.1:5173`.
    pub base_url: Url,
}

/// Failures talking to Flue. Never used to 500 the initiating HTTP handler;
/// the stream emits [`crate::model::stream::ChatStream::Error`] instead.
#[derive(Debug, thiserror::Error)]
pub(crate) enum FlueChatError {
    /// `FLUENT_BASE_URL` parsed at resolve time but could not be used as a base.
    #[error("invalid flue base url: {0}")]
    InvalidBaseUrl(String),
    /// Transport / reqwest failure.
    #[error("flue request failed")]
    Transport(#[source] reqwest::Error),
    /// Non-success HTTP status from Flue.
    #[error("flue rejected the request: status {status}, body {body}")]
    Rejected { status: u16, body: String },
    /// Response JSON did not match the wire protocol.
    #[error("flue response was not valid json: {0}")]
    InvalidJson(String),
    /// No assistant chunks for [`IDLE_TIMEOUT`].
    #[error("flue stream idle timeout")]
    IdleTimeout,
    /// Updates response omitted `Stream-Next-Offset`.
    #[error("flue updates response missing Stream-Next-Offset")]
    MissingOffset,
}

/// Incremental events the handler wraps in `ChatStream`.
#[derive(Debug)]
pub(crate) enum FlueTurnEvent {
    /// Assistant content (text, thinking, or tool lifecycle) to stream as-is.
    Part(AssistantMessagePart),
    /// Terminal Flue/transport failure. Handler emits `ChatStream::Error`.
    Failed(FlueChatError),
    /// The Flue submission settled or the turn otherwise ended.
    Finished {
        /// True when DCS cancelled the stream or Flue settled `aborted`.
        cancelled: bool,
    },
}

/// HTTP client for one Flue conversation. Adapter-only: no ChatStream mapping.
#[derive(Clone)]
pub(crate) struct FlueChatClient {
    http: reqwest::Client,
    base_url: Url,
}

#[derive(Debug, Serialize)]
struct DeliveredUserMessage<'a> {
    kind: &'static str,
    body: &'a str,
}

#[derive(Debug, Deserialize)]
struct AdmissionResponse {
    offset: String,
    #[serde(rename = "submissionId")]
    submission_id: String,
}

#[derive(Debug, Deserialize)]
struct AbortResponse {
    #[serde(default)]
    aborted: bool,
}

/// Flue `ConversationStreamChunk` (unknown `type` values are ignored).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub(crate) enum ConversationStreamChunk {
    /// Streamed text or reasoning delta.
    MessageDelta { kind: DeltaKind, delta: String },
    /// Tool call arguments became available.
    ToolInput {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolName")]
        tool_name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    /// Tool call succeeded.
    ToolOutput {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(default)]
        output: serde_json::Value,
    },
    /// Tool call failed.
    ToolOutputError {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "errorText")]
        error_text: String,
    },
    /// Terminal outcome for one admitted submission.
    SubmissionSettled {
        #[serde(rename = "submissionId")]
        submission_id: String,
        outcome: SettlementOutcome,
    },
    /// Any other chunk type (reset, message-started, …).
    #[serde(other)]
    Other,
}

/// `message-delta.kind`.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DeltaKind {
    Text,
    Reasoning,
}

/// `submission-settled.outcome`.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SettlementOutcome {
    Completed,
    Failed,
    Aborted,
}

struct UpdatesPage {
    chunks: Vec<ConversationStreamChunk>,
    next_offset: String,
}

/// Truthy values for `FLUENT_DCS_CHAT`: `1`, `true`, `on` (case-insensitive).
pub(crate) fn flag_is_on(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "on"
    )
}

/// Resolve the cutover target from already-read env strings.
///
/// Returns `None` (AgentLoop) when the flag is off/unset, or when the flag is
/// on but the base URL is missing/invalid. The latter logs; it must not 500.
pub(crate) fn resolve_flue_chat_target(
    flag: Option<&str>,
    base_url: Option<&str>,
) -> Option<FlueChatTarget> {
    if !flag.map(flag_is_on).unwrap_or(false) {
        return None;
    }
    let Some(raw) = base_url.map(str::trim).filter(|s| !s.is_empty()) else {
        tracing::error!(
            "FLUENT_DCS_CHAT is on but FLUENT_BASE_URL is missing; falling back to AgentLoop"
        );
        return None;
    };
    match Url::parse(raw) {
        Ok(mut url) => {
            // Normalize so path joins are stable (`http://host:port` + `/agents/...`).
            if url.path() == "/" {
                url.set_path("");
            }
            Some(FlueChatTarget { base_url: url })
        }
        Err(error) => {
            tracing::error!(
                error = ?error,
                base_url = raw,
                "FLUENT_DCS_CHAT is on but FLUENT_BASE_URL is not a valid URL; falling back to AgentLoop"
            );
            None
        }
    }
}

/// Read `FLUENT_DCS_CHAT` / `FLUENT_BASE_URL` via `maybe_env_var!`.
pub(crate) fn flue_chat_target_from_env() -> Option<FlueChatTarget> {
    resolve_flue_chat_target(
        FluentDcsChat::new().as_ref().map(|v| v.as_ref()),
        FluentBaseUrl::new().as_ref().map(|v| v.as_ref()),
    )
}

impl FlueChatClient {
    pub(crate) fn new(target: &FlueChatTarget) -> Self {
        let http = reqwest::Client::builder()
            .build()
            .expect("reqwest client construction cannot fail with static options");
        Self {
            http,
            base_url: target.base_url.clone(),
        }
    }

    fn conversation_url(&self, chat_id: &str) -> Result<Url, FlueChatError> {
        let mut url = self.base_url.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| FlueChatError::InvalidBaseUrl(self.base_url.to_string()))?;
            segments.push("agents");
            segments.push("super-agent");
            segments.push(chat_id);
        }
        Ok(url)
    }

    fn abort_url(&self, chat_id: &str) -> Result<Url, FlueChatError> {
        let mut url = self.conversation_url(chat_id)?;
        url.path_segments_mut()
            .map_err(|_| FlueChatError::InvalidBaseUrl(self.base_url.to_string()))?
            .push("abort");
        Ok(url)
    }

    #[tracing::instrument(skip(self, body), err)]
    async fn post_user_message(
        &self,
        chat_id: &str,
        body: &str,
    ) -> Result<AdmissionResponse, FlueChatError> {
        let url = self.conversation_url(chat_id)?;
        let response = self
            .http
            .post(url)
            .json(&DeliveredUserMessage { kind: "user", body })
            .send()
            .await
            .map_err(FlueChatError::Transport)?;
        let status = response.status();
        if status.as_u16() != 202 {
            let body = response.text().await.unwrap_or_default();
            return Err(FlueChatError::Rejected {
                status: status.as_u16(),
                body,
            });
        }
        response
            .json()
            .await
            .map_err(|e| FlueChatError::InvalidJson(e.to_string()))
    }

    #[tracing::instrument(skip(self), err)]
    async fn read_updates(
        &self,
        chat_id: &str,
        offset: &str,
    ) -> Result<UpdatesPage, FlueChatError> {
        let mut url = self.conversation_url(chat_id)?;
        url.query_pairs_mut()
            .append_pair("view", "updates")
            .append_pair("offset", offset)
            .append_pair("live", "long-poll");
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(FlueChatError::Transport)?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(FlueChatError::Rejected {
                status: status.as_u16(),
                body,
            });
        }
        let next_offset = response
            .headers()
            .get(STREAM_NEXT_OFFSET)
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned)
            .ok_or(FlueChatError::MissingOffset)?;
        let chunks = response
            .json()
            .await
            .map_err(|e| FlueChatError::InvalidJson(e.to_string()))?;
        Ok(UpdatesPage {
            chunks,
            next_offset,
        })
    }

    #[tracing::instrument(skip(self), err)]
    async fn abort(&self, chat_id: &str) -> Result<bool, FlueChatError> {
        let url = self.abort_url(chat_id)?;
        let response = self
            .http
            .post(url)
            .send()
            .await
            .map_err(FlueChatError::Transport)?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(FlueChatError::Rejected {
                status: status.as_u16(),
                body,
            });
        }
        let parsed = response
            .json::<AbortResponse>()
            .await
            .map_err(|e| FlueChatError::InvalidJson(e.to_string()))?;
        Ok(parsed.aborted)
    }
}

/// Map one Flue updates chunk to a persistable/streamable assistant part.
///
/// Tool outputs look up the name recorded on the matching `tool-input`.
/// Chunks with no ChatStream equivalent (`message-started`, reset, …) return
/// `None` — the handler does not invent UI events for them.
pub(crate) fn map_update_chunk(
    chunk: &ConversationStreamChunk,
    tool_names: &mut HashMap<String, String>,
) -> Option<AssistantMessagePart> {
    match chunk {
        ConversationStreamChunk::MessageDelta {
            kind: DeltaKind::Text,
            delta,
        } if !delta.is_empty() => Some(AssistantMessagePart::Text {
            text: delta.clone(),
        }),
        ConversationStreamChunk::MessageDelta {
            kind: DeltaKind::Reasoning,
            delta,
        } if !delta.is_empty() => Some(AssistantMessagePart::Thinking {
            thinking: delta.clone(),
        }),
        ConversationStreamChunk::ToolInput {
            tool_call_id,
            tool_name,
            input,
        } => {
            tool_names.insert(tool_call_id.clone(), tool_name.clone());
            Some(AssistantMessagePart::ToolCall {
                name: tool_name.clone(),
                json: input.clone(),
                id: tool_call_id.clone(),
            })
        }
        ConversationStreamChunk::ToolOutput {
            tool_call_id,
            output,
        } => {
            let name = tool_names.get(tool_call_id).cloned().unwrap_or_default();
            Some(AssistantMessagePart::ToolCallResponseJson {
                name,
                json: output.clone(),
                id: tool_call_id.clone(),
            })
        }
        ConversationStreamChunk::ToolOutputError {
            tool_call_id,
            error_text,
        } => {
            let name = tool_names.get(tool_call_id).cloned().unwrap_or_default();
            Some(AssistantMessagePart::ToolCallErr {
                name,
                description: error_text.clone(),
                id: tool_call_id.clone(),
            })
        }
        ConversationStreamChunk::MessageDelta { .. }
        | ConversationStreamChunk::SubmissionSettled { .. }
        | ConversationStreamChunk::Other => None,
    }
}

/// POST the user turn, long-poll `view=updates`, map chunks, abort on cancel.
pub(crate) fn stream_flue_turn(
    client: FlueChatClient,
    chat_id: String,
    user_body: String,
    cancel: CancellationToken,
) -> impl Stream<Item = FlueTurnEvent> + Send {
    stream! {
        if cancel.is_cancelled() {
            yield FlueTurnEvent::Finished { cancelled: true };
            return;
        }

        let admission = match client.post_user_message(&chat_id, &user_body).await {
            Ok(admission) => admission,
            Err(error) => {
                tracing::error!(error = ?error, chat_id = %chat_id, "flue admission failed");
                yield FlueTurnEvent::Failed(error);
                yield FlueTurnEvent::Finished { cancelled: false };
                return;
            }
        };

        let mut offset = admission.offset;
        let submission_id = admission.submission_id;
        let mut tool_names = HashMap::new();
        let mut was_cancelled = false;
        let mut last_chunk_at = Instant::now();

        loop {
            if last_chunk_at.elapsed() >= IDLE_TIMEOUT {
                yield FlueTurnEvent::Failed(FlueChatError::IdleTimeout);
                yield FlueTurnEvent::Finished { cancelled: was_cancelled };
                return;
            }

            let page = if was_cancelled {
                client.read_updates(&chat_id, &offset).await
            } else {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        was_cancelled = true;
                        client
                            .abort(&chat_id)
                            .await
                            .inspect_err(|error| {
                                tracing::error!(
                                    error = ?error,
                                    chat_id = %chat_id,
                                    "failed to abort flue conversation"
                                );
                            })
                            .ok();
                        continue;
                    }
                    page = client.read_updates(&chat_id, &offset) => page,
                }
            };

            let page = match page {
                Ok(page) => page,
                Err(error) => {
                    tracing::error!(error = ?error, chat_id = %chat_id, "flue updates read failed");
                    yield FlueTurnEvent::Failed(error);
                    yield FlueTurnEvent::Finished { cancelled: was_cancelled };
                    return;
                }
            };

            offset = page.next_offset;
            let mut settled = None;
            for chunk in &page.chunks {
                if let ConversationStreamChunk::SubmissionSettled {
                    submission_id: settled_id,
                    outcome,
                } = chunk
                    && settled_id == &submission_id
                {
                    settled = Some(*outcome);
                }
                if let Some(part) = map_update_chunk(chunk, &mut tool_names) {
                    last_chunk_at = Instant::now();
                    yield FlueTurnEvent::Part(part);
                }
            }

            match settled {
                Some(SettlementOutcome::Aborted) => {
                    yield FlueTurnEvent::Finished { cancelled: true };
                    return;
                }
                Some(SettlementOutcome::Failed) => {
                    yield FlueTurnEvent::Failed(FlueChatError::Rejected {
                        status: 0,
                        body: "submission failed".to_string(),
                    });
                    yield FlueTurnEvent::Finished { cancelled: was_cancelled };
                    return;
                }
                Some(SettlementOutcome::Completed) => {
                    yield FlueTurnEvent::Finished { cancelled: was_cancelled };
                    return;
                }
                None => {}
            }
        }
    }
}

#[cfg(test)]
mod test;
