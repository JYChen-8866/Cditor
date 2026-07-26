use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind, kind_tag_for_rich_block_kind};
use cditor_storage::{LoadDocumentRequest, StorageResult};
use uuid::Uuid;

use crate::error::sqlite_error;
use crate::ids::{block_id_to_sqlite, document_id_to_sqlite};
use crate::payload::insert_payload;
use crate::storage::SqliteDocumentStorage;
use crate::util::{sort_key, unix_millis};

impl SqliteDocumentStorage {
    pub(crate) async fn ensure_minimal_document(
        &self,
        request: &LoadDocumentRequest,
    ) -> StorageResult<()> {
        let _writer_guard = self.writer_gate().acquire().await?;
        let now = unix_millis()?;
        let workspace_id = Uuid::from_u128(request.workspace_id as u128);
        let document_id = document_id_to_sqlite(request.document_id);
        let block_id = block_id_to_sqlite(1);
        let mut transaction = self.pool.begin().await.map_err(sqlite_error)?;

        sqlx::query(
            "INSERT OR IGNORE INTO workspaces (id, name, created_at, updated_at) \
             VALUES (?, 'Default Workspace', ?, ?)",
        )
        .bind(workspace_id)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(sqlite_error)?;
        sqlx::query(
            "INSERT OR IGNORE INTO documents \
             (id, workspace_id, title, structure_version, content_version, layout_version, \
              schema_version, created_at, updated_at) \
             VALUES (?, ?, 'Untitled', 1, 1, 0, 1, ?, ?)",
        )
        .bind(document_id)
        .bind(workspace_id)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(sqlite_error)?;

        let block_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM blocks WHERE document_id = ? AND deleted_at IS NULL",
        )
        .bind(document_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(sqlite_error)?;
        if block_count == 0 {
            let kind = RichBlockKind::Heading { level: 1 };
            let payload = BlockPayloadRecord::rich_text(1, kind.clone(), "");
            sqlx::query(
                "INSERT INTO blocks \
                 (id, document_id, parent_id, sort_key, depth, kind_tag, flags, content_version, \
                  structure_version, updated_at) VALUES (?, ?, NULL, ?, 0, ?, 0, 1, 1, ?)",
            )
            .bind(block_id)
            .bind(document_id)
            .bind(sort_key(0))
            .bind(i64::from(kind_tag_for_rich_block_kind(&kind)))
            .bind(now)
            .execute(&mut *transaction)
            .await
            .map_err(sqlite_error)?;
            insert_payload(&mut transaction, document_id, &payload, now).await?;
        }
        transaction.commit().await.map_err(sqlite_error)
    }
}
