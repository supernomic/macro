//! Ports for lifecycle connectors.

use super::model::{ConnectorAccount, ConnectorRecord, Provider, Result};
use entity_graph::domain::model::{GraphNode, UpsertNode};

/// Storage.
pub trait ConnectorRepo: Send + Sync + 'static {
    /// Insert an account.
    fn insert_account(&self, account: &ConnectorAccount)
    -> impl Future<Output = Result<()>> + Send;

    /// Fetch an account.
    fn get_account(
        &self,
        id: macro_uuid::Uuid,
    ) -> impl Future<Output = Result<Option<ConnectorAccount>>> + Send;

    /// List accounts for an org + optional provider.
    fn list_accounts(
        &self,
        org_id: Option<i32>,
        provider: Option<Provider>,
    ) -> impl Future<Output = Result<Vec<ConnectorAccount>>> + Send;

    /// Upsert a synced record.
    fn upsert_record(&self, record: &ConnectorRecord) -> impl Future<Output = Result<()>> + Send;

    /// Mark the account synced.
    fn touch_sync(
        &self,
        id: macro_uuid::Uuid,
        cursor: Option<&str>,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// Graph projection port (implemented by the entity-graph service).
pub trait GraphIngest: Send + Sync + 'static {
    /// Upsert a graph node for a connector record.
    fn upsert_node(
        &self,
        org_id: Option<i32>,
        node: UpsertNode,
    ) -> impl Future<Output = Result<GraphNode>> + Send;
}
