//! Lifecycle connector domain service.

#[cfg(test)]
mod test;

use chrono::Utc;
use entity_graph::domain::model::UpsertNode;
use macro_uuid::Uuid;

use super::model::{
    ConnectorAccount, ConnectorError, ConnectorRecord, IngestRecord, Provider, Result,
};
use super::ports::{ConnectorRepo, GraphIngest};

/// Domain service.
pub trait ConnectorService: Send + Sync + 'static {
    /// Register a provider account. `credential_ref` is a bound secret id,
    /// never the secret itself.
    fn register_account(
        &self,
        org_id: Option<i32>,
        provider: Provider,
        display_name: String,
        credential_ref: String,
    ) -> impl Future<Output = Result<ConnectorAccount>> + Send;

    /// Ingest a batch of records, projecting each onto the entity graph.
    fn ingest(
        &self,
        account_id: Uuid,
        records: Vec<IngestRecord>,
        cursor: Option<String>,
    ) -> impl Future<Output = Result<Vec<ConnectorRecord>>> + Send;
}

/// Concrete service.
#[derive(Debug, Clone)]
pub struct ConnectorServiceImpl<R, G> {
    repo: R,
    graph: G,
}

impl<R: ConnectorRepo, G: GraphIngest> ConnectorServiceImpl<R, G> {
    /// Build over storage + graph ingest.
    pub fn new(repo: R, graph: G) -> Self {
        Self { repo, graph }
    }
}

impl<R: ConnectorRepo, G: GraphIngest> ConnectorService for ConnectorServiceImpl<R, G> {
    #[tracing::instrument(skip(self), err)]
    async fn register_account(
        &self,
        org_id: Option<i32>,
        provider: Provider,
        display_name: String,
        credential_ref: String,
    ) -> Result<ConnectorAccount> {
        if display_name.trim().is_empty() || credential_ref.trim().is_empty() {
            return Err(ConnectorError::InvalidRequest(
                "display_name and credential_ref are required".to_string(),
            ));
        }
        let account = ConnectorAccount {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            provider,
            display_name,
            credential_ref,
            last_synced_at: None,
            last_cursor: None,
            created_at: Utc::now(),
        };
        self.repo.insert_account(&account).await?;
        Ok(account)
    }

    #[tracing::instrument(skip(self, records), err)]
    async fn ingest(
        &self,
        account_id: Uuid,
        records: Vec<IngestRecord>,
        cursor: Option<String>,
    ) -> Result<Vec<ConnectorRecord>> {
        let account = self
            .repo
            .get_account(account_id)
            .await?
            .ok_or(ConnectorError::NotFound)?;
        let mut stored = Vec::new();
        for rec in records {
            if rec.external_id.trim().is_empty() || rec.display_name.trim().is_empty() {
                return Err(ConnectorError::InvalidRequest(
                    "external_id and display_name are required".to_string(),
                ));
            }
            let node = self
                .graph
                .upsert_node(
                    account.org_id,
                    UpsertNode {
                        node_type: account.provider.node_type(&rec.record_type).to_string(),
                        display_name: rec.display_name,
                        attributes: rec.payload.clone(),
                        native_entity_type: Some(format!(
                            "connector:{}:{}",
                            account.provider.as_str(),
                            rec.record_type
                        )),
                        native_entity_id: Some(rec.external_id.clone()),
                    },
                )
                .await?;
            let row = ConnectorRecord {
                id: macro_uuid::generate_uuid_v7(),
                account_id,
                external_id: rec.external_id,
                record_type: rec.record_type,
                payload: rec.payload,
                graph_node_id: Some(node.id),
                updated_at: Utc::now(),
            };
            self.repo.upsert_record(&row).await?;
            stored.push(row);
        }
        self.repo.touch_sync(account_id, cursor.as_deref()).await?;
        Ok(stored)
    }
}
