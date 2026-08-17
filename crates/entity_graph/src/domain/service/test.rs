use std::sync::Mutex;

use super::*;
use crate::domain::model::{
    GraphEdge, GraphNode, KnowledgeDocument, UpsertEdge, UpsertKnowledge, UpsertNode,
};
use crate::domain::ports::GraphRepo;
use macro_uuid::Uuid;
use serde_json::json;

#[derive(Default)]
struct Fake {
    nodes: Mutex<Vec<GraphNode>>,
    edges: Mutex<Vec<GraphEdge>>,
    docs: Mutex<Vec<KnowledgeDocument>>,
}

impl GraphRepo for Fake {
    async fn upsert_node(&self, node: &GraphNode) -> Result<GraphNode> {
        let mut nodes = self.nodes.lock().unwrap();
        if let (Some(t), Some(id)) = (&node.native_entity_type, &node.native_entity_id)
            && let Some(existing) = nodes.iter_mut().find(|n| {
                n.org_id == node.org_id
                    && n.native_entity_type.as_ref() == Some(t)
                    && n.native_entity_id.as_ref() == Some(id)
            })
        {
            existing.display_name = node.display_name.clone();
            existing.attributes = node.attributes.clone();
            existing.updated_at = node.updated_at;
            return Ok(existing.clone());
        }
        nodes.push(node.clone());
        Ok(node.clone())
    }

    async fn get_node(&self, id: Uuid) -> Result<Option<GraphNode>> {
        Ok(self
            .nodes
            .lock()
            .unwrap()
            .iter()
            .find(|n| n.id == id)
            .cloned())
    }

    async fn upsert_edge(&self, edge: &GraphEdge) -> Result<GraphEdge> {
        let mut edges = self.edges.lock().unwrap();
        if let Some(existing) = edges.iter().find(|e| {
            e.from_node_id == edge.from_node_id
                && e.to_node_id == edge.to_node_id
                && e.relationship == edge.relationship
        }) {
            return Ok(existing.clone());
        }
        edges.push(edge.clone());
        Ok(edge.clone())
    }

    async fn neighbors(
        &self,
        node_id: Uuid,
        relationship: Option<&str>,
    ) -> Result<Vec<(GraphEdge, GraphNode)>> {
        let edges = self.edges.lock().unwrap().clone();
        let nodes = self.nodes.lock().unwrap().clone();
        Ok(edges
            .into_iter()
            .filter(|e| {
                (e.from_node_id == node_id || e.to_node_id == node_id)
                    && relationship.is_none_or(|r| e.relationship == r)
            })
            .filter_map(|e| {
                let other = if e.from_node_id == node_id {
                    e.to_node_id
                } else {
                    e.from_node_id
                };
                nodes
                    .iter()
                    .find(|n| n.id == other)
                    .cloned()
                    .map(|n| (e, n))
            })
            .collect())
    }

    async fn upsert_knowledge(&self, doc: &KnowledgeDocument) -> Result<KnowledgeDocument> {
        let mut docs = self.docs.lock().unwrap();
        if let Some(existing) = docs
            .iter_mut()
            .find(|d| d.org_id == doc.org_id && d.slug == doc.slug)
        {
            *existing = doc.clone();
            return Ok(existing.clone());
        }
        docs.push(doc.clone());
        Ok(doc.clone())
    }

    async fn get_knowledge(
        &self,
        org_id: Option<i32>,
        slug: &str,
    ) -> Result<Option<KnowledgeDocument>> {
        Ok(self
            .docs
            .lock()
            .unwrap()
            .iter()
            .find(|d| d.org_id == org_id && d.slug == slug)
            .cloned())
    }
}

fn svc() -> GraphServiceImpl<Fake> {
    GraphServiceImpl::new(Fake::default())
}

#[tokio::test]
async fn upsert_node_and_edge_and_neighbors() {
    let svc = svc();
    let person = svc
        .upsert_node(
            Some(1),
            UpsertNode {
                node_type: "Person".into(),
                display_name: "Ada".into(),
                attributes: json!({}),
                native_entity_type: Some("user".into()),
                native_entity_id: Some("ada".into()),
            },
        )
        .await
        .unwrap();
    let device = svc
        .upsert_node(
            Some(1),
            UpsertNode {
                node_type: "Device".into(),
                display_name: "MacBook".into(),
                attributes: json!({}),
                native_entity_type: None,
                native_entity_id: None,
            },
        )
        .await
        .unwrap();
    svc.upsert_edge(
        Some(1),
        UpsertEdge {
            from_node_id: person.id,
            to_node_id: device.id,
            relationship: "owns_device".into(),
            attributes: json!({}),
        },
    )
    .await
    .unwrap();
    let neighbors = svc
        .neighbors(Some(1), person.id, Some("owns_device"))
        .await
        .unwrap();
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].1.display_name, "MacBook");
}

#[tokio::test]
async fn self_edge_rejected() {
    let svc = svc();
    let n = svc
        .upsert_node(
            Some(1),
            UpsertNode {
                node_type: "Person".into(),
                display_name: "Ada".into(),
                attributes: json!({}),
                native_entity_type: None,
                native_entity_id: None,
            },
        )
        .await
        .unwrap();
    let err = svc
        .upsert_edge(
            Some(1),
            UpsertEdge {
                from_node_id: n.id,
                to_node_id: n.id,
                relationship: "knows".into(),
                attributes: json!({}),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, GraphError::InvalidRequest(_)));
}

#[tokio::test]
async fn human_authored_knowledge_is_protected() {
    let svc = svc();
    svc.upsert_knowledge(
        Some(1),
        UpsertKnowledge {
            slug: "vpn".into(),
            title: "VPN".into(),
            body: "human".into(),
            okf_sources: vec![],
            okf_generated: false,
            human_authored: true,
        },
    )
    .await
    .unwrap();
    let err = svc
        .upsert_knowledge(
            Some(1),
            UpsertKnowledge {
                slug: "vpn".into(),
                title: "VPN".into(),
                body: "generated".into(),
                okf_sources: vec![],
                okf_generated: true,
                human_authored: false,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, GraphError::HumanAuthoredProtected));
}

#[tokio::test]
async fn non_human_upsert_cannot_overwrite_human_authored() {
    let svc = svc();
    svc.upsert_knowledge(
        Some(1),
        UpsertKnowledge {
            slug: "vpn".into(),
            title: "VPN".into(),
            body: "human".into(),
            okf_sources: vec![],
            okf_generated: false,
            human_authored: true,
        },
    )
    .await
    .unwrap();
    let err = svc
        .upsert_knowledge(
            Some(1),
            UpsertKnowledge {
                slug: "vpn".into(),
                title: "VPN".into(),
                body: "machine".into(),
                okf_sources: vec![],
                okf_generated: false,
                human_authored: false,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, GraphError::HumanAuthoredProtected));
}

#[tokio::test]
async fn human_can_update_human_authored_knowledge() {
    let svc = svc();
    svc.upsert_knowledge(
        Some(1),
        UpsertKnowledge {
            slug: "vpn".into(),
            title: "VPN".into(),
            body: "human".into(),
            okf_sources: vec![],
            okf_generated: false,
            human_authored: true,
        },
    )
    .await
    .unwrap();
    let updated = svc
        .upsert_knowledge(
            Some(1),
            UpsertKnowledge {
                slug: "vpn".into(),
                title: "VPN v2".into(),
                body: "human-edited".into(),
                okf_sources: vec![],
                okf_generated: false,
                human_authored: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.body, "human-edited");
    assert!(updated.human_authored);
}

#[tokio::test]
async fn get_node_is_org_scoped() {
    let svc = svc();
    let node = svc
        .upsert_node(
            Some(1),
            UpsertNode {
                node_type: "Person".into(),
                display_name: "Ada".into(),
                attributes: json!({}),
                native_entity_type: None,
                native_entity_id: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(svc.get_node(Some(1), node.id).await.unwrap().id, node.id);
    let err = svc.get_node(Some(2), node.id).await.unwrap_err();
    assert!(matches!(err, GraphError::NotFound));
}

#[tokio::test]
async fn cross_org_edge_rejected() {
    let svc = svc();
    let a = svc
        .upsert_node(
            Some(1),
            UpsertNode {
                node_type: "Person".into(),
                display_name: "Ada".into(),
                attributes: json!({}),
                native_entity_type: None,
                native_entity_id: None,
            },
        )
        .await
        .unwrap();
    let b = svc
        .upsert_node(
            Some(2),
            UpsertNode {
                node_type: "Device".into(),
                display_name: "Mac".into(),
                attributes: json!({}),
                native_entity_type: None,
                native_entity_id: None,
            },
        )
        .await
        .unwrap();
    let err = svc
        .upsert_edge(
            Some(1),
            UpsertEdge {
                from_node_id: a.id,
                to_node_id: b.id,
                relationship: "owns_device".into(),
                attributes: json!({}),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, GraphError::NotFound));
}

#[tokio::test]
async fn neighbors_hide_other_org_nodes() {
    let svc = svc();
    let person = svc
        .upsert_node(
            Some(1),
            UpsertNode {
                node_type: "Person".into(),
                display_name: "Ada".into(),
                attributes: json!({}),
                native_entity_type: None,
                native_entity_id: None,
            },
        )
        .await
        .unwrap();
    let err = svc.neighbors(Some(2), person.id, None).await.unwrap_err();
    assert!(matches!(err, GraphError::NotFound));
}

#[tokio::test]
async fn empty_node_fields_rejected() {
    let svc = svc();
    let err = svc
        .upsert_node(
            Some(1),
            UpsertNode {
                node_type: " ".into(),
                display_name: "Ada".into(),
                attributes: json!({}),
                native_entity_type: None,
                native_entity_id: None,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, GraphError::InvalidRequest(_)));
}
