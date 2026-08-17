//! Agent-facing graph facade.

use agent_identity::domain::model::VerifiedAgent;
use macro_uuid::Uuid;

use super::model::{
    GraphEdge, GraphError, GraphNode, KnowledgeDocument, Result, UpsertEdge, UpsertKnowledge,
    UpsertNode,
};
use super::service::GraphService;

/// Scope required to read the graph.
pub const SCOPE_GRAPH_READ: &str = "graph:read";
/// Scope required to write the graph.
pub const SCOPE_GRAPH_WRITE: &str = "graph:write";

fn require_scope(agent: &VerifiedAgent, scope: &str) -> Result<()> {
    agent
        .require_scope(scope)
        .map_err(|_| GraphError::MissingScope {
            required: scope.to_string(),
        })
}

/// Agent-facing facade.
#[derive(Debug, Clone)]
pub struct AgentGraphFacade<S> {
    service: S,
}

impl<S: GraphService> AgentGraphFacade<S> {
    /// Build over the graph service.
    pub fn new(service: S) -> Self {
        Self { service }
    }

    /// Upsert a node in the agent's org.
    pub async fn upsert_node(&self, agent: &VerifiedAgent, node: UpsertNode) -> Result<GraphNode> {
        require_scope(agent, SCOPE_GRAPH_WRITE)?;
        self.service.upsert_node(agent.principal.org_id, node).await
    }

    /// Upsert an edge in the agent's org.
    pub async fn upsert_edge(&self, agent: &VerifiedAgent, edge: UpsertEdge) -> Result<GraphEdge> {
        require_scope(agent, SCOPE_GRAPH_WRITE)?;
        self.service.upsert_edge(agent.principal.org_id, edge).await
    }

    /// Neighbors of a node.
    pub async fn neighbors(
        &self,
        agent: &VerifiedAgent,
        node_id: Uuid,
        relationship: Option<&str>,
    ) -> Result<Vec<(GraphEdge, GraphNode)>> {
        require_scope(agent, SCOPE_GRAPH_READ)?;
        let node = self.service.get_node(node_id).await?;
        if node.org_id != agent.principal.org_id {
            return Err(GraphError::NotFound);
        }
        self.service.neighbors(node_id, relationship).await
    }

    /// Upsert a knowledge document.
    pub async fn upsert_knowledge(
        &self,
        agent: &VerifiedAgent,
        doc: UpsertKnowledge,
    ) -> Result<KnowledgeDocument> {
        require_scope(agent, SCOPE_GRAPH_WRITE)?;
        self.service
            .upsert_knowledge(agent.principal.org_id, doc)
            .await
    }
}
