//! Entity-graph domain service.

#[cfg(test)]
mod test;

use chrono::Utc;
use macro_uuid::Uuid;

use super::model::{
    GraphEdge, GraphError, GraphNode, KnowledgeDocument, Result, UpsertEdge, UpsertKnowledge,
    UpsertNode, content_hash,
};
use super::ports::GraphRepo;

/// Domain service.
pub trait GraphService: Send + Sync + 'static {
    /// Upsert a node.
    fn upsert_node(
        &self,
        org_id: Option<i32>,
        node: UpsertNode,
    ) -> impl Future<Output = Result<GraphNode>> + Send;

    /// Fetch a node.
    fn get_node(&self, id: Uuid) -> impl Future<Output = Result<GraphNode>> + Send;

    /// Upsert an edge.
    fn upsert_edge(
        &self,
        org_id: Option<i32>,
        edge: UpsertEdge,
    ) -> impl Future<Output = Result<GraphEdge>> + Send;

    /// Neighbors of a node.
    fn neighbors(
        &self,
        node_id: Uuid,
        relationship: Option<&str>,
    ) -> impl Future<Output = Result<Vec<(GraphEdge, GraphNode)>>> + Send;

    /// Upsert knowledge. Generated updates never overwrite human-authored docs.
    fn upsert_knowledge(
        &self,
        org_id: Option<i32>,
        doc: UpsertKnowledge,
    ) -> impl Future<Output = Result<KnowledgeDocument>> + Send;
}

/// Concrete service.
#[derive(Debug, Clone)]
pub struct GraphServiceImpl<R> {
    repo: R,
}

impl<R: GraphRepo> GraphServiceImpl<R> {
    /// Build over a repo.
    pub fn new(repo: R) -> Self {
        Self { repo }
    }
}

impl<R: GraphRepo> GraphService for GraphServiceImpl<R> {
    #[tracing::instrument(skip(self, node), err)]
    async fn upsert_node(&self, org_id: Option<i32>, node: UpsertNode) -> Result<GraphNode> {
        if node.node_type.trim().is_empty() || node.display_name.trim().is_empty() {
            return Err(GraphError::InvalidRequest(
                "node_type and display_name are required".to_string(),
            ));
        }
        let now = Utc::now();
        let record = GraphNode {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            node_type: node.node_type,
            display_name: node.display_name,
            attributes: node.attributes,
            native_entity_type: node.native_entity_type,
            native_entity_id: node.native_entity_id,
            created_at: now,
            updated_at: now,
        };
        self.repo.upsert_node(&record).await
    }

    #[tracing::instrument(skip(self), err)]
    async fn get_node(&self, id: Uuid) -> Result<GraphNode> {
        self.repo.get_node(id).await?.ok_or(GraphError::NotFound)
    }

    #[tracing::instrument(skip(self, edge), err)]
    async fn upsert_edge(&self, org_id: Option<i32>, edge: UpsertEdge) -> Result<GraphEdge> {
        if edge.from_node_id == edge.to_node_id {
            return Err(GraphError::InvalidRequest(
                "self-edges are not allowed".to_string(),
            ));
        }
        if edge.relationship.trim().is_empty() {
            return Err(GraphError::InvalidRequest(
                "relationship is required".to_string(),
            ));
        }
        let record = GraphEdge {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            from_node_id: edge.from_node_id,
            to_node_id: edge.to_node_id,
            relationship: edge.relationship,
            attributes: edge.attributes,
            created_at: Utc::now(),
        };
        self.repo.upsert_edge(&record).await
    }

    #[tracing::instrument(skip(self), err)]
    async fn neighbors(
        &self,
        node_id: Uuid,
        relationship: Option<&str>,
    ) -> Result<Vec<(GraphEdge, GraphNode)>> {
        self.repo.neighbors(node_id, relationship).await
    }

    #[tracing::instrument(skip(self, doc), err)]
    async fn upsert_knowledge(
        &self,
        org_id: Option<i32>,
        doc: UpsertKnowledge,
    ) -> Result<KnowledgeDocument> {
        if doc.slug.trim().is_empty() {
            return Err(GraphError::InvalidRequest("slug is required".to_string()));
        }
        if let Some(existing) = self.repo.get_knowledge(org_id, &doc.slug).await? {
            if existing.human_authored && doc.okf_generated {
                return Err(GraphError::HumanAuthoredProtected);
            }
            let mut updated = existing;
            updated.title = doc.title;
            updated.body = doc.body.clone();
            updated.okf_sources = doc.okf_sources;
            updated.okf_generated = doc.okf_generated;
            updated.human_authored = updated.human_authored || doc.human_authored;
            updated.content_hash = content_hash(&doc.body);
            updated.updated_at = Utc::now();
            return self.repo.upsert_knowledge(&updated).await;
        }
        let now = Utc::now();
        let record = KnowledgeDocument {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            slug: doc.slug,
            title: doc.title,
            body: doc.body.clone(),
            okf_type: "knowledge".to_string(),
            okf_sources: doc.okf_sources,
            okf_generated: doc.okf_generated,
            okf_verified: false,
            okf_status: if doc.human_authored {
                "active".to_string()
            } else {
                "draft".to_string()
            },
            stale_after: None,
            content_hash: content_hash(&doc.body),
            human_authored: doc.human_authored,
            created_at: now,
            updated_at: now,
        };
        self.repo.upsert_knowledge(&record).await
    }
}
