//! Postgres training-export job storage.

use sqlx::PgPool;

use crate::domain::model::{ExportJob, Result};
use crate::domain::ports::ExportJobRepo;

/// Postgres-backed export-job repo.
#[derive(Debug, Clone)]
pub struct PgExportJobRepo {
    pool: PgPool,
}

impl PgExportJobRepo {
    /// Build over a MacroDB pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl ExportJobRepo for PgExportJobRepo {
    #[tracing::instrument(skip(self, job), err)]
    async fn insert(&self, job: &ExportJob) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO training_export_jobs (
                id, org_id, projection, sharing_mode, composition_id,
                from_occurred_at, to_occurred_at, status, row_count,
                artifact_uri, error, created_at, completed_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
            job.id,
            job.org_id,
            job.projection.as_str(),
            job.sharing_mode.as_str(),
            job.composition_id,
            job.from_occurred_at,
            job.to_occurred_at,
            job.status,
            job.row_count,
            job.artifact_uri,
            job.error,
            job.created_at,
            job.completed_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip(self, job), err)]
    async fn update(&self, job: &ExportJob) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE training_export_jobs
            SET status = $2,
                row_count = $3,
                artifact_uri = $4,
                error = $5,
                completed_at = $6
            WHERE id = $1
            "#,
            job.id,
            job.status,
            job.row_count,
            job.artifact_uri,
            job.error,
            job.completed_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
