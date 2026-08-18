use std::sync::{Arc, Mutex};

use super::*;
use crate::domain::model::{ConnectorAccount, ConnectorRecord, IngestRecord, Provider};
use crate::domain::ports::{ConnectorRepo, GraphIngest};
use chrono::Utc;
use entity_graph::domain::model::{GraphNode, UpsertNode};
use macro_uuid::Uuid;
use serde_json::json;

#[derive(Clone, Default)]
struct FakeRepo {
    accounts: Arc<Mutex<Vec<ConnectorAccount>>>,
    records: Arc<Mutex<Vec<ConnectorRecord>>>,
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

#[derive(Clone, Default)]
struct FakeGraph {
    nodes: Arc<Mutex<Vec<GraphNode>>>,
}

impl GraphIngest for FakeGraph {
    async fn upsert_node(&self, org_id: Option<i32>, node: UpsertNode) -> Result<GraphNode> {
        let now = Utc::now();
        let stored = GraphNode {
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
        self.nodes.lock().unwrap().push(stored.clone());
        Ok(stored)
    }
}

fn setup() -> (ConnectorServiceImpl<FakeRepo, FakeGraph>, FakeGraph) {
    let graph = FakeGraph::default();
    let svc = ConnectorServiceImpl::new(FakeRepo::default(), graph.clone());
    (svc, graph)
}

#[tokio::test]
async fn okta_user_projects_to_person() {
    let (svc, graph) = setup();
    let account = svc
        .register_account(Some(1), Provider::Okta, "prod".into(), "cred-ref-1".into())
        .await
        .unwrap();
    assert_eq!(account.credential_ref, "cred-ref-1");
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
    let nodes = graph.nodes.lock().unwrap();
    assert_eq!(nodes[0].node_type, "Person");
    assert_eq!(nodes[0].org_id, Some(1));
    assert_eq!(
        nodes[0].native_entity_type.as_deref(),
        Some("connector:okta:user")
    );
}

#[tokio::test]
async fn okta_application_projects_to_software_application() {
    let (svc, graph) = setup();
    let account = svc
        .register_account(Some(1), Provider::Okta, "prod".into(), "cred-ref-1".into())
        .await
        .unwrap();
    svc.ingest(
        account.id,
        vec![IngestRecord {
            external_id: "okta-app-1".into(),
            record_type: "application".into(),
            display_name: "Salesforce".into(),
            payload: json!({}),
        }],
        None,
    )
    .await
    .unwrap();
    let nodes = graph.nodes.lock().unwrap();
    assert_eq!(nodes[0].node_type, "SoftwareApplication");
    assert_eq!(
        nodes[0].native_entity_type.as_deref(),
        Some("connector:okta:application")
    );
}

#[tokio::test]
async fn empty_credential_ref_rejected() {
    let (svc, _) = setup();
    let err = svc
        .register_account(Some(1), Provider::Meraki, "net".into(), " ".into())
        .await
        .unwrap_err();
    assert!(matches!(err, ConnectorError::InvalidRequest(_)));
}

#[tokio::test]
async fn iru_and_meraki_ingest_as_device() {
    let (svc, graph) = setup();
    let iru = svc
        .register_account(Some(2), Provider::Iru, "mdm".into(), "iru-ref".into())
        .await
        .unwrap();
    assert_eq!(iru.credential_ref, "iru-ref");
    svc.ingest(
        iru.id,
        vec![IngestRecord {
            external_id: "mac-1".into(),
            record_type: "computer".into(),
            display_name: "Ada's Mac".into(),
            payload: json!({}),
        }],
        None,
    )
    .await
    .unwrap();
    let meraki = svc
        .register_account(Some(2), Provider::Meraki, "net".into(), "meraki-ref".into())
        .await
        .unwrap();
    svc.ingest(
        meraki.id,
        vec![IngestRecord {
            external_id: "ap-1".into(),
            record_type: "appliance".into(),
            display_name: "Office AP".into(),
            payload: json!({}),
        }],
        None,
    )
    .await
    .unwrap();
    let nodes = graph.nodes.lock().unwrap();
    assert_eq!(nodes[0].node_type, "Device");
    assert_eq!(nodes[0].org_id, Some(2));
    assert_eq!(nodes[1].node_type, "Device");
    assert_eq!(
        nodes[1].native_entity_type.as_deref(),
        Some("connector:meraki:appliance")
    );
}

#[tokio::test]
async fn provider_node_type_mapping() {
    assert_eq!(Provider::Iru.node_type("computer"), "Device");
    assert_eq!(Provider::Meraki.node_type("appliance"), "Device");
    assert_eq!(
        Provider::Okta.node_type("application"),
        "SoftwareApplication"
    );
    assert_eq!(Provider::Okta.node_type("user"), "Person");
}

#[tokio::test]
async fn empty_display_name_rejected_on_ingest() {
    let (svc, _) = setup();
    let account = svc
        .register_account(Some(1), Provider::Okta, "prod".into(), "cred-ref-1".into())
        .await
        .unwrap();
    let err = svc
        .ingest(
            account.id,
            vec![IngestRecord {
                external_id: "okta-u-1".into(),
                record_type: "user".into(),
                display_name: "  ".into(),
                payload: json!({}),
            }],
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ConnectorError::InvalidRequest(_)));
}
