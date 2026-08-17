//! Ports for the entity graph.

use macro_uuid::Uuid;

use super::model::{GraphEdge, GraphNode, KnowledgeDocument, Result};

/// Storage port.
pub trait GraphRepo: Send + Sync + 'static {
    /// Insert or replace a node. When `native_entity_type`+id is set, upsert
    /// on that unique key.
    fn upsert_node(&self, node: &GraphNode) -> impl Future<Output = Result<GraphNode>> + Send;

    /// Fetch a node in `org_id`. Null org matches null (`IS NOT DISTINCT FROM`).
    fn get_node(
        &self,
        org_id: Option<i32>,
        id: Uuid,
    ) -> impl Future<Output = Result<Option<GraphNode>>> + Send;

    /// Insert an edge; no-op (return existing) on unique conflict.
    fn upsert_edge(&self, edge: &GraphEdge) -> impl Future<Output = Result<GraphEdge>> + Send;

    /// Neighbors of a node in `org_id` (edge and neighbor org must match;
    /// null org matches null).
    fn neighbors(
        &self,
        org_id: Option<i32>,
        node_id: Uuid,
        relationship: Option<&str>,
    ) -> impl Future<Output = Result<Vec<(GraphEdge, GraphNode)>>> + Send;

    /// Upsert a knowledge document by org+slug.
    ///
    /// A non-human write must not replace a human-authored title, body,
    /// content hash, sources, or generated flag. `human_authored` is sticky
    /// (`existing OR incoming`).
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
