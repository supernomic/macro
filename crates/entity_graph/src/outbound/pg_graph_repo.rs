//! Postgres implementation of the entity graph.

use macro_uuid::Uuid;
use sqlx::PgPool;

use crate::domain::model::{GraphEdge, GraphNode, KnowledgeDocument, Result};
use crate::domain::ports::GraphRepo;

/// Postgres-backed graph repo.
#[derive(Debug, Clone)]
pub struct PgGraphRepo {
    pool: PgPool,
}

impl PgGraphRepo {
    /// Build over a MacroDB pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl GraphRepo for PgGraphRepo {
    #[tracing::instrument(skip(self, node), err)]
    async fn upsert_node(&self, node: &GraphNode) -> Result<GraphNode> {
        if let (Some(native_type), Some(native_id)) =
            (&node.native_entity_type, &node.native_entity_id)
        {
            let row = sqlx::query!(
                r#"
                INSERT INTO entity_graph_nodes (
                    id, org_id, node_type, display_name, attributes,
                    native_entity_type, native_entity_id, created_at, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                ON CONFLICT (org_id, native_entity_type, native_entity_id)
                    WHERE native_entity_type IS NOT NULL
                DO UPDATE SET
                    display_name = EXCLUDED.display_name,
                    attributes = EXCLUDED.attributes,
                    node_type = EXCLUDED.node_type,
                    updated_at = EXCLUDED.updated_at
                RETURNING id, org_id, node_type, display_name, attributes,
                          native_entity_type, native_entity_id, created_at, updated_at
                "#,
                node.id,
                node.org_id,
                node.node_type,
                node.display_name,
                node.attributes,
                native_type,
                native_id,
                node.created_at,
                node.updated_at,
            )
            .fetch_one(&self.pool)
            .await?;
            return Ok(GraphNode {
                id: row.id,
                org_id: row.org_id,
                node_type: row.node_type,
                display_name: row.display_name,
                attributes: row.attributes,
                native_entity_type: row.native_entity_type,
                native_entity_id: row.native_entity_id,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }
        sqlx::query!(
            r#"
            INSERT INTO entity_graph_nodes (
                id, org_id, node_type, display_name, attributes,
                native_entity_type, native_entity_id, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            node.id,
            node.org_id,
            node.node_type,
            node.display_name,
            node.attributes,
            node.native_entity_type.as_deref(),
            node.native_entity_id.as_deref(),
            node.created_at,
            node.updated_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(node.clone())
    }

    #[tracing::instrument(skip(self), err)]
    async fn get_node(&self, id: Uuid) -> Result<Option<GraphNode>> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, node_type, display_name, attributes,
                   native_entity_type, native_entity_id, created_at, updated_at
            FROM entity_graph_nodes
            WHERE id = $1
            "#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| GraphNode {
            id: r.id,
            org_id: r.org_id,
            node_type: r.node_type,
            display_name: r.display_name,
            attributes: r.attributes,
            native_entity_type: r.native_entity_type,
            native_entity_id: r.native_entity_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }

    #[tracing::instrument(skip(self, edge), err)]
    async fn upsert_edge(&self, edge: &GraphEdge) -> Result<GraphEdge> {
        let row = sqlx::query!(
            r#"
            INSERT INTO entity_graph_edges (
                id, org_id, from_node_id, to_node_id, relationship, attributes, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (from_node_id, to_node_id, relationship)
            DO UPDATE SET attributes = EXCLUDED.attributes
            RETURNING id, org_id, from_node_id, to_node_id, relationship, attributes, created_at
            "#,
            edge.id,
            edge.org_id,
            edge.from_node_id,
            edge.to_node_id,
            edge.relationship,
            edge.attributes,
            edge.created_at,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(GraphEdge {
            id: row.id,
            org_id: row.org_id,
            from_node_id: row.from_node_id,
            to_node_id: row.to_node_id,
            relationship: row.relationship,
            attributes: row.attributes,
            created_at: row.created_at,
        })
    }

    #[tracing::instrument(skip(self), err)]
    async fn neighbors(
        &self,
        node_id: Uuid,
        relationship: Option<&str>,
    ) -> Result<Vec<(GraphEdge, GraphNode)>> {
        let rows = sqlx::query!(
            r#"
            SELECT e.id as edge_id, e.org_id as edge_org, e.from_node_id, e.to_node_id,
                   e.relationship, e.attributes as edge_attrs, e.created_at as edge_created,
                   n.id as node_id, n.org_id as node_org, n.node_type, n.display_name,
                   n.attributes as node_attrs, n.native_entity_type, n.native_entity_id,
                   n.created_at as node_created, n.updated_at as node_updated
            FROM entity_graph_edges e
            JOIN entity_graph_nodes n
              ON n.id = CASE WHEN e.from_node_id = $1 THEN e.to_node_id ELSE e.from_node_id END
            WHERE (e.from_node_id = $1 OR e.to_node_id = $1)
              AND ($2::text IS NULL OR e.relationship = $2)
            "#,
            node_id,
            relationship,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    GraphEdge {
                        id: r.edge_id,
                        org_id: r.edge_org,
                        from_node_id: r.from_node_id,
                        to_node_id: r.to_node_id,
                        relationship: r.relationship,
                        attributes: r.edge_attrs,
                        created_at: r.edge_created,
                    },
                    GraphNode {
                        id: r.node_id,
                        org_id: r.node_org,
                        node_type: r.node_type,
                        display_name: r.display_name,
                        attributes: r.node_attrs,
                        native_entity_type: r.native_entity_type,
                        native_entity_id: r.native_entity_id,
                        created_at: r.node_created,
                        updated_at: r.node_updated,
                    },
                )
            })
            .collect())
    }

    #[tracing::instrument(skip(self, doc), err)]
    async fn upsert_knowledge(&self, doc: &KnowledgeDocument) -> Result<KnowledgeDocument> {
        let sources = serde_json::to_value(&doc.okf_sources).unwrap_or(serde_json::json!([]));
        let row = sqlx::query!(
            r#"
            INSERT INTO knowledge_documents (
                id, org_id, slug, title, body, okf_type, okf_sources,
                okf_generated, okf_verified, okf_status, stale_after,
                content_hash, human_authored, created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7,
                $8, $9, $10, $11,
                $12, $13, $14, $15
            )
            ON CONFLICT (org_id, slug)
            DO UPDATE SET
                title = EXCLUDED.title,
                body = EXCLUDED.body,
                okf_sources = EXCLUDED.okf_sources,
                okf_generated = EXCLUDED.okf_generated,
                okf_status = EXCLUDED.okf_status,
                content_hash = EXCLUDED.content_hash,
                human_authored = knowledge_documents.human_authored OR EXCLUDED.human_authored,
                updated_at = EXCLUDED.updated_at
            RETURNING id, org_id, slug, title, body, okf_type, okf_sources,
                      okf_generated, okf_verified, okf_status, stale_after,
                      content_hash, human_authored, created_at, updated_at
            "#,
            doc.id,
            doc.org_id,
            doc.slug,
            doc.title,
            doc.body,
            doc.okf_type,
            sources,
            doc.okf_generated,
            doc.okf_verified,
            doc.okf_status,
            doc.stale_after,
            doc.content_hash,
            doc.human_authored,
            doc.created_at,
            doc.updated_at,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(KnowledgeDocument {
            id: row.id,
            org_id: row.org_id,
            slug: row.slug,
            title: row.title,
            body: row.body,
            okf_type: row.okf_type,
            okf_sources: serde_json::from_value(row.okf_sources).unwrap_or_default(),
            okf_generated: row.okf_generated,
            okf_verified: row.okf_verified,
            okf_status: row.okf_status,
            stale_after: row.stale_after,
            content_hash: row.content_hash,
            human_authored: row.human_authored,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    #[tracing::instrument(skip(self), err)]
    async fn get_knowledge(
        &self,
        org_id: Option<i32>,
        slug: &str,
    ) -> Result<Option<KnowledgeDocument>> {
        let row = sqlx::query!(
            r#"
            SELECT id, org_id, slug, title, body, okf_type, okf_sources,
                   okf_generated, okf_verified, okf_status, stale_after,
                   content_hash, human_authored, created_at, updated_at
            FROM knowledge_documents
            WHERE org_id IS NOT DISTINCT FROM $1 AND slug = $2
            "#,
            org_id,
            slug,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| KnowledgeDocument {
            id: r.id,
            org_id: r.org_id,
            slug: r.slug,
            title: r.title,
            body: r.body,
            okf_type: r.okf_type,
            okf_sources: serde_json::from_value(r.okf_sources).unwrap_or_default(),
            okf_generated: r.okf_generated,
            okf_verified: r.okf_verified,
            okf_status: r.okf_status,
            stale_after: r.stale_after,
            content_hash: r.content_hash,
            human_authored: r.human_authored,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }))
    }
}
