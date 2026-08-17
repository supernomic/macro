//! Graph ingest adapter over the entity-graph domain service.

use entity_graph::domain::model::{GraphNode, UpsertNode};
use entity_graph::domain::service::GraphService;

use crate::domain::model::{ConnectorError, Result};
use crate::domain::ports::GraphIngest;

/// Wraps a [`GraphService`] as a [`GraphIngest`] port.
#[derive(Debug, Clone)]
pub struct EntityGraphIngest<S> {
    inner: S,
}

impl<S: GraphService> EntityGraphIngest<S> {
    /// Build over a graph service.
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S: GraphService> GraphIngest for EntityGraphIngest<S> {
    async fn upsert_node(&self, org_id: Option<i32>, node: UpsertNode) -> Result<GraphNode> {
        self.inner
            .upsert_node(org_id, node)
            .await
            .map_err(|e| ConnectorError::Graph(e.to_string()))
    }
}
