use std::sync::Mutex;

use agent_identity::domain::model::{AgentKind, AgentPrincipal, VerifiedAgent};
use chrono::Utc;
use macro_uuid::Uuid;
use serde_json::json;

use super::*;
use crate::domain::model::{
    GraphEdge, GraphNode, KnowledgeDocument, UpsertEdge, UpsertKnowledge, UpsertNode,
};
use crate::domain::service::GraphService;

#[derive(Default)]
struct Recording {
    node_orgs: Mutex<Vec<Option<i32>>>,
    edge_orgs: Mutex<Vec<Option<i32>>>,
    neighbor_orgs: Mutex<Vec<Option<i32>>>,
    knowledge_orgs: Mutex<Vec<Option<i32>>>,
}

impl GraphService for Recording {
    async fn upsert_node(&self, org_id: Option<i32>, node: UpsertNode) -> Result<GraphNode> {
        self.node_orgs.lock().unwrap().push(org_id);
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

    async fn get_node(&self, _org_id: Option<i32>, _id: Uuid) -> Result<GraphNode> {
        Err(GraphError::NotFound)
    }

    async fn upsert_edge(&self, org_id: Option<i32>, edge: UpsertEdge) -> Result<GraphEdge> {
        self.edge_orgs.lock().unwrap().push(org_id);
        Ok(GraphEdge {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            from_node_id: edge.from_node_id,
            to_node_id: edge.to_node_id,
            relationship: edge.relationship,
            attributes: edge.attributes,
            created_at: Utc::now(),
        })
    }

    async fn neighbors(
        &self,
        org_id: Option<i32>,
        _node_id: Uuid,
        _relationship: Option<&str>,
    ) -> Result<Vec<(GraphEdge, GraphNode)>> {
        self.neighbor_orgs.lock().unwrap().push(org_id);
        Ok(vec![])
    }

    async fn upsert_knowledge(
        &self,
        org_id: Option<i32>,
        doc: UpsertKnowledge,
    ) -> Result<KnowledgeDocument> {
        self.knowledge_orgs.lock().unwrap().push(org_id);
        let now = Utc::now();
        Ok(KnowledgeDocument {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            slug: doc.slug,
            title: doc.title,
            body: doc.body.clone(),
            okf_type: "knowledge".into(),
            okf_sources: doc.okf_sources,
            okf_generated: doc.okf_generated,
            okf_verified: false,
            okf_status: "draft".into(),
            stale_after: None,
            content_hash: crate::domain::model::content_hash(&doc.body),
            human_authored: doc.human_authored,
            created_at: now,
            updated_at: now,
        })
    }
}

fn agent(org_id: Option<i32>, scopes: &[&str]) -> VerifiedAgent {
    VerifiedAgent {
        principal: AgentPrincipal {
            id: macro_uuid::generate_uuid_v7(),
            org_id,
            slug: "techops".to_string(),
            display_name: "TechOps".to_string(),
            kind: AgentKind::DomainAgent,
            created_at: Utc::now(),
            disabled_at: None,
        },
        token_id: macro_uuid::generate_uuid_v7(),
        scopes: scopes.iter().map(|s| s.to_string()).collect(),
    }
}

fn person() -> UpsertNode {
    UpsertNode {
        node_type: "Person".into(),
        display_name: "Ada".into(),
        attributes: json!({}),
        native_entity_type: None,
        native_entity_id: None,
    }
}

#[tokio::test]
async fn write_requires_graph_write_and_forces_org() {
    let facade = AgentGraphFacade::new(Recording::default());
    let reader = agent(Some(1), &[SCOPE_GRAPH_READ]);
    let denied = facade.upsert_node(&reader, person()).await.unwrap_err();
    assert!(matches!(
        denied,
        GraphError::MissingScope { required } if required == SCOPE_GRAPH_WRITE
    ));

    let writer = agent(Some(7), &[SCOPE_GRAPH_WRITE]);
    facade.upsert_node(&writer, person()).await.unwrap();
    assert_eq!(
        facade.service.node_orgs.lock().unwrap().as_slice(),
        &[Some(7)]
    );
}

#[tokio::test]
async fn edge_write_requires_graph_write() {
    let facade = AgentGraphFacade::new(Recording::default());
    let edge = UpsertEdge {
        from_node_id: macro_uuid::generate_uuid_v7(),
        to_node_id: macro_uuid::generate_uuid_v7(),
        relationship: "owns_device".into(),
        attributes: json!({}),
    };
    let denied = facade
        .upsert_edge(&agent(Some(1), &[]), edge.clone())
        .await
        .unwrap_err();
    assert!(matches!(denied, GraphError::MissingScope { .. }));

    let writer = agent(Some(3), &[SCOPE_GRAPH_WRITE]);
    facade.upsert_edge(&writer, edge).await.unwrap();
    assert_eq!(
        facade.service.edge_orgs.lock().unwrap().as_slice(),
        &[Some(3)]
    );
}

#[tokio::test]
async fn neighbors_require_graph_read_and_use_agent_org() {
    let facade = AgentGraphFacade::new(Recording::default());
    let id = macro_uuid::generate_uuid_v7();
    let denied = facade
        .neighbors(&agent(Some(1), &[SCOPE_GRAPH_WRITE]), id, None)
        .await
        .unwrap_err();
    assert!(matches!(
        denied,
        GraphError::MissingScope { required } if required == SCOPE_GRAPH_READ
    ));

    let reader = agent(Some(9), &[SCOPE_GRAPH_READ]);
    facade.neighbors(&reader, id, None).await.unwrap();
    assert_eq!(
        facade.service.neighbor_orgs.lock().unwrap().as_slice(),
        &[Some(9)]
    );
}

#[tokio::test]
async fn knowledge_write_requires_graph_write() {
    let facade = AgentGraphFacade::new(Recording::default());
    let doc = UpsertKnowledge {
        slug: "vpn".into(),
        title: "VPN".into(),
        body: "body".into(),
        okf_sources: vec![],
        okf_generated: false,
        human_authored: true,
    };
    let denied = facade
        .upsert_knowledge(&agent(Some(1), &[SCOPE_GRAPH_READ]), doc.clone())
        .await
        .unwrap_err();
    assert!(matches!(denied, GraphError::MissingScope { .. }));

    let writer = agent(Some(4), &[SCOPE_GRAPH_WRITE]);
    facade.upsert_knowledge(&writer, doc).await.unwrap();
    assert_eq!(
        facade.service.knowledge_orgs.lock().unwrap().as_slice(),
        &[Some(4)]
    );
}
