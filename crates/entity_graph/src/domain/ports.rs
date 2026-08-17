//! Ports for the entity graph.

use macro_uuid::Uuid;

use super::model::{GraphEdge, GraphNode, KnowledgeDocument, Result};

/// Storage port.
pub trait GraphRepo: Send + Sync + 'static {
    /// Insert or replace a node. When `native_entity_type`+id is set, upsert
    /// on that unique key.
    fn upsert_node(&self, node: &GraphNode) -> impl Future<Output = Result<GraphNode>> + Send;

    /// Fetch a node.
    fn get_node(&self, id: Uuid) -> impl Future<Output = Result<Option<GraphNode>>> + Send;

    /// Insert an edge; no-op (return existing) on unique conflict.
    fn upsert_edge(&self, edge: &GraphEdge) -> impl Future<Output = Result<GraphEdge>> + Send;

    /// Neighbors of a node (outgoing then incoming).
    fn neighbors(
        &self,
        node_id: Uuid,
        relationship: Option<&str>,
    ) -> impl Future<Output = Result<Vec<(GraphEdge, GraphNode)>>> + Send;

    /// Upsert a knowledge document by org+slug.
    fn upsert_knowledge(
        &self,
        doc: &KnowledgeDocument,
    ) -> impl Future<Output = Result<KnowledgeDocument>> + Send;

    /// Fetch knowledge by org+slug.
    fn get_knowledge(
        &self,
        org_id: Option<i32>,
        slug: &str,
    ) -> impl Future<Output = Result<Option<KnowledgeDocument>>> + Send;
}
