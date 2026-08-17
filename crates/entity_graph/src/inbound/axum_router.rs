//! HTTP surface for the entity graph.

use std::sync::Arc;

use agent_identity::domain::model::IdentityError;
use agent_identity::domain::ports::AgentIdentityService;
use agent_identity::inbound::AgentBearer;
use axum::extract::{FromRef, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use macro_authorization::{MacroAuthorizationService, MacroAuthorizationState};
use macro_uuid::Uuid;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::facade::AgentGraphFacade;
use crate::domain::model::{
    GraphEdge, GraphError, GraphNode, KnowledgeDocument, UpsertEdge, UpsertKnowledge, UpsertNode,
};
use crate::domain::service::GraphService;

/// Router state.
pub struct GraphRouterState<A, I, Auth> {
    /// Agent-facing facade.
    pub facade: Arc<AgentGraphFacade<A>>,
    /// Identity service.
    pub identity: Arc<I>,
    /// Authorization state.
    pub authorization_state: MacroAuthorizationState<Auth>,
}

impl<A, I, Auth> Clone for GraphRouterState<A, I, Auth> {
    fn clone(&self) -> Self {
        Self {
            facade: self.facade.clone(),
            identity: self.identity.clone(),
            authorization_state: self.authorization_state.clone(),
        }
    }
}

impl<A, I, Auth> FromRef<GraphRouterState<A, I, Auth>> for MacroAuthorizationState<Auth> {
    fn from_ref(state: &GraphRouterState<A, I, Auth>) -> Self {
        state.authorization_state.clone()
    }
}

/// Error body.
#[derive(Debug, Serialize, ToSchema)]
pub struct GraphErrorBody {
    /// Error description.
    pub error: String,
}

fn error_response(e: GraphError) -> Response {
    let status = match &e {
        GraphError::NotFound => StatusCode::NOT_FOUND,
        GraphError::HumanAuthoredProtected => StatusCode::CONFLICT,
        GraphError::MissingScope { .. } => StatusCode::FORBIDDEN,
        GraphError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
        GraphError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(GraphErrorBody {
            error: e.to_string(),
        }),
    )
        .into_response()
}

fn identity_error_response(e: IdentityError) -> Response {
    let status = match &e {
        IdentityError::MalformedToken => StatusCode::BAD_REQUEST,
        IdentityError::Db(_) => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::UNAUTHORIZED,
    };
    (
        status,
        Json(GraphErrorBody {
            error: "agent authentication failed".to_string(),
        }),
    )
        .into_response()
}

/// Upsert-node body.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpsertNodeRequest {
    /// Node type.
    pub node_type: String,
    /// Display name.
    pub display_name: String,
    /// Attributes.
    #[serde(default)]
    pub attributes: serde_json::Value,
    /// Native entity type.
    pub native_entity_type: Option<String>,
    /// Native entity id.
    pub native_entity_id: Option<String>,
}

/// Upsert-edge body.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpsertEdgeRequest {
    /// Source node.
    pub from_node_id: Uuid,
    /// Target node.
    pub to_node_id: Uuid,
    /// Relationship.
    pub relationship: String,
    /// Attributes.
    #[serde(default)]
    pub attributes: serde_json::Value,
}

/// Upsert-knowledge body.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpsertKnowledgeRequest {
    /// Slug.
    pub slug: String,
    /// Title.
    pub title: String,
    /// Body.
    pub body: String,
    /// Sources.
    #[serde(default)]
    pub okf_sources: Vec<String>,
    /// Generated flag.
    #[serde(default)]
    pub okf_generated: bool,
    /// Human-authored flag.
    #[serde(default)]
    pub human_authored: bool,
}

/// Neighbor query.
#[derive(Debug, Deserialize)]
pub struct NeighborQuery {
    /// Optional relationship filter.
    pub relationship: Option<String>,
}

/// Neighbor pair.
#[derive(Debug, Serialize, ToSchema)]
pub struct Neighbor {
    /// Edge.
    pub edge: GraphEdge,
    /// Other node.
    pub node: GraphNode,
}

/// Build the graph router.
pub fn entity_graph_router<A, I, Auth, S>(state: GraphRouterState<A, I, Auth>) -> Router<S>
where
    A: GraphService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
    S: Send + Sync + Clone + 'static,
{
    Router::new()
        .route(
            "/agent-graph/nodes",
            post(upsert_node_handler::<A, I, Auth>),
        )
        .route(
            "/agent-graph/nodes/{id}/neighbors",
            get(neighbors_handler::<A, I, Auth>),
        )
        .route(
            "/agent-graph/edges",
            post(upsert_edge_handler::<A, I, Auth>),
        )
        .route(
            "/agent-graph/knowledge",
            post(upsert_knowledge_handler::<A, I, Auth>),
        )
        .with_state(state)
}

/// Upsert a graph node.
#[utoipa::path(
    post,
    path = "/agent-graph/nodes",
    request_body = UpsertNodeRequest,
    responses((status = 200, description = "The node", body = GraphNode)),
    tag = "graph"
)]
#[tracing::instrument(skip_all)]
pub async fn upsert_node_handler<A, I, Auth>(
    State(state): State<GraphRouterState<A, I, Auth>>,
    bearer: AgentBearer,
    Json(body): Json<UpsertNodeRequest>,
) -> Response
where
    A: GraphService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    match state
        .facade
        .upsert_node(
            &agent,
            UpsertNode {
                node_type: body.node_type,
                display_name: body.display_name,
                attributes: body.attributes,
                native_entity_type: body.native_entity_type,
                native_entity_id: body.native_entity_id,
            },
        )
        .await
    {
        Ok(node) => Json(node).into_response(),
        Err(e) => error_response(e),
    }
}

/// Upsert an edge.
#[utoipa::path(
    post,
    path = "/agent-graph/edges",
    request_body = UpsertEdgeRequest,
    responses((status = 200, description = "The edge", body = GraphEdge)),
    tag = "graph"
)]
#[tracing::instrument(skip_all)]
pub async fn upsert_edge_handler<A, I, Auth>(
    State(state): State<GraphRouterState<A, I, Auth>>,
    bearer: AgentBearer,
    Json(body): Json<UpsertEdgeRequest>,
) -> Response
where
    A: GraphService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    match state
        .facade
        .upsert_edge(
            &agent,
            UpsertEdge {
                from_node_id: body.from_node_id,
                to_node_id: body.to_node_id,
                relationship: body.relationship,
                attributes: body.attributes,
            },
        )
        .await
    {
        Ok(edge) => Json(edge).into_response(),
        Err(e) => error_response(e),
    }
}

/// List neighbors of a node.
#[utoipa::path(
    get,
    path = "/agent-graph/nodes/{id}/neighbors",
    responses((status = 200, description = "Neighbors", body = Vec<Neighbor>)),
    tag = "graph"
)]
#[tracing::instrument(skip_all)]
pub async fn neighbors_handler<A, I, Auth>(
    State(state): State<GraphRouterState<A, I, Auth>>,
    bearer: AgentBearer,
    Path(id): Path<Uuid>,
    Query(query): Query<NeighborQuery>,
) -> Response
where
    A: GraphService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    match state
        .facade
        .neighbors(&agent, id, query.relationship.as_deref())
        .await
    {
        Ok(pairs) => Json(
            pairs
                .into_iter()
                .map(|(edge, node)| Neighbor { edge, node })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => error_response(e),
    }
}

/// Upsert a knowledge document.
#[utoipa::path(
    post,
    path = "/agent-graph/knowledge",
    request_body = UpsertKnowledgeRequest,
    responses((status = 200, description = "The document", body = KnowledgeDocument)),
    tag = "graph"
)]
#[tracing::instrument(skip_all)]
pub async fn upsert_knowledge_handler<A, I, Auth>(
    State(state): State<GraphRouterState<A, I, Auth>>,
    bearer: AgentBearer,
    Json(body): Json<UpsertKnowledgeRequest>,
) -> Response
where
    A: GraphService,
    I: AgentIdentityService,
    Auth: MacroAuthorizationService,
{
    let agent = match state.identity.verify_bearer(&bearer.0).await {
        Ok(agent) => agent,
        Err(e) => return identity_error_response(e),
    };
    match state
        .facade
        .upsert_knowledge(
            &agent,
            UpsertKnowledge {
                slug: body.slug,
                title: body.title,
                body: body.body,
                okf_sources: body.okf_sources,
                okf_generated: body.okf_generated,
                human_authored: body.human_authored,
            },
        )
        .await
    {
        Ok(doc) => Json(doc).into_response(),
        Err(e) => error_response(e),
    }
}
