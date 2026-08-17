use std::sync::Mutex;

use super::*;
use crate::domain::model::{ConnectorAccount, ConnectorRecord, IngestRecord, Provider};
use crate::domain::ports::{ConnectorRepo, GraphIngest};
use chrono::Utc;
use entity_graph::domain::model::{GraphNode, UpsertNode};
use macro_uuid::Uuid;
use serde_json::json;

#[derive(Default)]
struct FakeRepo {
    accounts: Mutex<Vec<ConnectorAccount>>,
    records: Mutex<Vec<ConnectorRecord>>,
}

impl ConnectorRepo for FakeRepo {
    async fn insert_account(&self, account: &ConnectorAccount) -> Result<()> {
        self.accounts.lock().unwrap().push(account.clone());
        Ok(())
    }

    async fn get_account(&self, id: Uuid) -> Result<Option<ConnectorAccount>> {
        Ok(self
            .accounts
            .lock()
            .unwrap()
            .iter()
            .find(|a| a.id == id)
            .cloned())
    }

    async fn list_accounts(
        &self,
        org_id: Option<i32>,
        provider: Option<Provider>,
    ) -> Result<Vec<ConnectorAccount>> {
        Ok(self
            .accounts
            .lock()
            .unwrap()
            .iter()
            .filter(|a| a.org_id == org_id && provider.is_none_or(|p| a.provider == p))
            .cloned()
            .collect())
    }

    async fn upsert_record(&self, record: &ConnectorRecord) -> Result<()> {
        self.records.lock().unwrap().push(record.clone());
        Ok(())
    }

    async fn touch_sync(&self, id: Uuid, cursor: Option<&str>) -> Result<()> {
        let mut accounts = self.accounts.lock().unwrap();
        if let Some(a) = accounts.iter_mut().find(|a| a.id == id) {
            a.last_synced_at = Some(Utc::now());
            a.last_cursor = cursor.map(str::to_string);
        }
        Ok(())
    }
}

struct FakeGraph;

impl GraphIngest for FakeGraph {
    async fn upsert_node(&self, org_id: Option<i32>, node: UpsertNode) -> Result<GraphNode> {
        let now = Utc::now();
        Ok(GraphNode {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            node_type: node.node_type,
            display_name: node.display_name,
            attributes: node.attributes,
            native_entity_type: node.native_entity_type,
            native_entity_id: node.native_entity_id,
            created_at: now,
            updated_at: now,
        })
    }
}

fn svc() -> ConnectorServiceImpl<FakeRepo, FakeGraph> {
    ConnectorServiceImpl::new(FakeRepo::default(), FakeGraph)
}

#[tokio::test]
async fn okta_user_projects_to_person() {
    let svc = svc();
    let account = svc
        .register_account(Some(1), Provider::Okta, "prod".into(), "secret-ref".into())
        .await
        .unwrap();
    let records = svc
        .ingest(
            account.id,
            vec![IngestRecord {
                external_id: "okta-u-1".into(),
                record_type: "user".into(),
                display_name: "Ada".into(),
                payload: json!({"email": "ada@example.com"}),
            }],
            Some("cursor-1".into()),
        )
        .await
        .unwrap();
    assert_eq!(records.len(), 1);
    assert!(records[0].graph_node_id.is_some());
}

#[tokio::test]
async fn empty_credential_ref_rejected() {
    let svc = svc();
    let err = svc
        .register_account(Some(1), Provider::Meraki, "net".into(), " ".into())
        .await
        .unwrap_err();
    assert!(matches!(err, ConnectorError::InvalidRequest(_)));
}

#[tokio::test]
async fn iru_and_meraki_project_to_device() {
    assert_eq!(Provider::Iru.node_type("computer"), "Device");
    assert_eq!(Provider::Meraki.node_type("appliance"), "Device");
    assert_eq!(
        Provider::Okta.node_type("application"),
        "SoftwareApplication"
    );
}
