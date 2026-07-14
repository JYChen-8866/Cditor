use sqlx::Row;
use sqlx::types::Uuid;

use crate::error::{PostgresStorageError, PostgresStorageResult};
use crate::types::{DocumentRow, PgDocumentId};

use super::PostgresDocumentStore;

impl PostgresDocumentStore {
    pub async fn save_document_metadata(&self, row: &DocumentRow) -> PostgresStorageResult<()> {
        self.ensure_workspace_exists(row.workspace_id).await?;

        sqlx::query(
            r#"
            INSERT INTO documents (
                id,
                workspace_id,
                title,
                structure_version,
                content_version,
                layout_version,
                schema_version,
                updated_at,
                deleted_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, now(), NULL)
            ON CONFLICT (id) DO UPDATE SET
                workspace_id = EXCLUDED.workspace_id,
                title = EXCLUDED.title,
                structure_version = EXCLUDED.structure_version,
                content_version = EXCLUDED.content_version,
                layout_version = EXCLUDED.layout_version,
                schema_version = EXCLUDED.schema_version,
                updated_at = now(),
                deleted_at = NULL
            "#,
        )
        .bind(row.id)
        .bind(row.workspace_id)
        .bind(&row.title)
        .bind(row.structure_version)
        .bind(row.content_version)
        .bind(row.layout_version)
        .bind(row.schema_version)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn load_document_metadata(
        &self,
        document_id: PgDocumentId,
    ) -> PostgresStorageResult<DocumentRow> {
        let row = sqlx::query(
            r#"
            SELECT id, workspace_id, title, structure_version, content_version, layout_version, schema_version
            FROM documents
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(document_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| PostgresStorageError::NotFound {
            entity: "document",
            id: document_id.to_string(),
        })?;

        Ok(DocumentRow {
            id: row.try_get("id")?,
            workspace_id: row.try_get("workspace_id")?,
            title: row.try_get("title")?,
            structure_version: row.try_get("structure_version")?,
            content_version: row.try_get("content_version")?,
            layout_version: row.try_get("layout_version")?,
            schema_version: row.try_get("schema_version")?,
        })
    }

    async fn ensure_workspace_exists(&self, workspace_id: Uuid) -> PostgresStorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO workspaces (id, name, updated_at)
            VALUES ($1, 'Default Workspace', now())
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(workspace_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
