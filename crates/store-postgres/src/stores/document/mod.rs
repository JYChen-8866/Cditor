//! PostgreSQL document metadata, structure, attributes, and index snapshots.

use sqlx::{PgPool, Row};

use cditor_core::document::{BlockIndexRecord, DocumentIndex};
use cditor_core::ids::DocumentId;
use cditor_core::layout::BlockLayoutMeta;
use cditor_core::rich_text::{kind_tag_for_rich_block_kind, rich_block_kind_from_tag};
use cditor_core::version::StructureVersion;
use cditor_storage::traits::DocumentIndexStore;

use crate::error::{PostgresStorageError, PostgresStorageResult};
use crate::types::{
    PgBlockId, PgDocumentId, pg_block_id_from_runtime, rich_block_kind_from_db,
    rich_block_kind_to_db, runtime_block_id_from_pg, runtime_document_id_from_pg,
};

mod attrs;
mod metadata;
mod snapshot;

#[derive(Debug, Clone)]
pub struct PostgresDocumentStore {
    pool: PgPool,
}

impl PostgresDocumentStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn save_block_index_records(
        &self,
        document_id: PgDocumentId,
        records: &[BlockIndexRecord],
        structure_version: i64,
    ) -> PostgresStorageResult<()> {
        let mut tx = self.pool.begin().await?;

        let block_ids: Vec<PgBlockId> = records
            .iter()
            .map(|record| pg_block_id_from_runtime(record.id))
            .collect();

        for (index, record) in records.iter().enumerate() {
            let block_id = block_ids[index];
            let parent_id = record.parent_id.map(pg_block_id_from_runtime);
            let prev_id = index.checked_sub(1).map(|prev| block_ids[prev]);
            let next_id = block_ids.get(index + 1).copied();
            let sort_key = sort_key_for_index(index);
            let kind = rich_block_kind_to_db(&rich_block_kind_from_tag(record.kind_tag));

            sqlx::query(
                r#"
                INSERT INTO blocks (
                    id,
                    document_id,
                    parent_id,
                    prev_id,
                    next_id,
                    sort_key,
                    depth,
                    kind,
                    flags,
                    content_version,
                    structure_version,
                    attrs_version,
                    updated_at,
                    deleted_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 1, $10, 1, now(), NULL)
                ON CONFLICT (id) DO UPDATE SET
                    document_id = EXCLUDED.document_id,
                    parent_id = EXCLUDED.parent_id,
                    prev_id = EXCLUDED.prev_id,
                    next_id = EXCLUDED.next_id,
                    sort_key = EXCLUDED.sort_key,
                    depth = EXCLUDED.depth,
                    kind = EXCLUDED.kind,
                    flags = EXCLUDED.flags,
                    structure_version = EXCLUDED.structure_version,
                    updated_at = now(),
                    deleted_at = NULL
                "#,
            )
            .bind(block_id)
            .bind(document_id)
            .bind(parent_id)
            .bind(prev_id)
            .bind(next_id)
            .bind(sort_key)
            .bind(i32::from(record.depth))
            .bind(kind)
            .bind(flags_to_i32(record.flags)?)
            .bind(structure_version)
            .execute(&mut *tx)
            .await?;
        }

        if block_ids.is_empty() {
            sqlx::query(
                r#"
                UPDATE blocks
                SET deleted_at = now(), updated_at = now(), structure_version = $2
                WHERE document_id = $1 AND deleted_at IS NULL
                "#,
            )
            .bind(document_id)
            .bind(structure_version)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                r#"
                UPDATE blocks
                SET deleted_at = now(), updated_at = now(), structure_version = $3
                WHERE document_id = $1 AND deleted_at IS NULL AND NOT (id = ANY($2))
                "#,
            )
            .bind(document_id)
            .bind(&block_ids)
            .bind(structure_version)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            r#"
            UPDATE documents
            SET structure_version = $2, updated_at = now()
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(document_id)
        .bind(structure_version)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn load_block_index_records(
        &self,
        document_id: PgDocumentId,
    ) -> PostgresStorageResult<Vec<BlockIndexRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT id, parent_id, depth, kind, flags
            FROM blocks
            WHERE document_id = $1 AND deleted_at IS NULL
            ORDER BY sort_key, id
            "#,
        )
        .bind(document_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let block_id: PgBlockId = row.try_get("id")?;
                let parent_id: Option<PgBlockId> = row.try_get("parent_id")?;
                let depth: i32 = row.try_get("depth")?;
                let kind: String = row.try_get("kind")?;
                let flags: i32 = row.try_get("flags")?;

                let id = runtime_block_id_from_pg(block_id).ok_or_else(|| {
                    PostgresStorageError::CorruptData {
                        message: format!("block id {block_id} is outside runtime namespace"),
                    }
                })?;
                let parent_id = parent_id
                    .map(|parent_id| {
                        runtime_block_id_from_pg(parent_id).ok_or_else(|| {
                            PostgresStorageError::CorruptData {
                                message: format!(
                                    "parent block id {parent_id} is outside runtime namespace"
                                ),
                            }
                        })
                    })
                    .transpose()?;
                let depth =
                    u16::try_from(depth).map_err(|_| PostgresStorageError::CorruptData {
                        message: format!("block {block_id} has invalid depth {depth}"),
                    })?;
                let flags =
                    u32::try_from(flags).map_err(|_| PostgresStorageError::CorruptData {
                        message: format!("block {block_id} has invalid flags {flags}"),
                    })?;
                let kind_tag = kind_tag_for_rich_block_kind(&rich_block_kind_from_db(&kind));

                Ok(
                    BlockIndexRecord::new(id, parent_id, depth, kind_tag, flags).with_layout_meta(
                        BlockLayoutMeta::new(id, BlockLayoutMeta::DEFAULT_ESTIMATED_HEIGHT),
                    ),
                )
            })
            .collect()
    }

    pub async fn count_live_blocks(
        &self,
        document_id: PgDocumentId,
    ) -> PostgresStorageResult<usize> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM blocks
            WHERE document_id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(document_id)
        .fetch_one(&self.pool)
        .await?;
        usize::try_from(count).map_err(|_| PostgresStorageError::CorruptData {
            message: format!("document {document_id} has invalid live block count {count}"),
        })
    }

    pub async fn soft_delete_block(&self, block_id: PgBlockId) -> PostgresStorageResult<()> {
        let result = sqlx::query(
            r#"
            UPDATE blocks
            SET deleted_at = now(), updated_at = now()
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(block_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(PostgresStorageError::NotFound {
                entity: "block",
                id: block_id.to_string(),
            });
        }

        Ok(())
    }

    pub async fn update_document_structure_version(
        &self,
        document_id: PgDocumentId,
        expected_version: i64,
        next_version: i64,
    ) -> PostgresStorageResult<()> {
        let result = sqlx::query(
            r#"
            UPDATE documents
            SET structure_version = $3, updated_at = now()
            WHERE id = $1 AND structure_version = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(document_id)
        .bind(expected_version)
        .bind(next_version)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(PostgresStorageError::Conflict {
                message: format!(
                    "document {document_id} structure version is not expected version {expected_version}"
                ),
            });
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PostgresDocumentIndexSnapshot {
    document_id: DocumentId,
    structure_version: StructureVersion,
    records: Vec<BlockIndexRecord>,
}

impl PostgresDocumentIndexSnapshot {
    pub async fn load(
        store: &PostgresDocumentStore,
        document_id: PgDocumentId,
    ) -> PostgresStorageResult<Self> {
        let document = store.load_document_metadata(document_id).await?;
        let runtime_document_id = runtime_document_id_from_pg(document_id).ok_or_else(|| {
            PostgresStorageError::CorruptData {
                message: format!("document id {document_id} is outside runtime namespace"),
            }
        })?;
        let structure_version = u64::try_from(document.structure_version).map_err(|_| {
            PostgresStorageError::CorruptData {
                message: format!(
                    "document {document_id} has negative structure version {}",
                    document.structure_version
                ),
            }
        })?;
        let records = store.load_block_index_records(document_id).await?;

        Ok(Self {
            document_id: runtime_document_id,
            structure_version,
            records,
        })
    }

    pub fn into_document_index(
        self,
    ) -> Result<DocumentIndex, cditor_core::document::DocumentIndexBuildError> {
        DocumentIndex::new(self.document_id, self.records, self.structure_version)
    }
}

impl DocumentIndexStore for PostgresDocumentIndexSnapshot {
    fn load_document_index_records(&self, document_id: DocumentId) -> Vec<BlockIndexRecord> {
        if document_id == self.document_id {
            self.records.clone()
        } else {
            Vec::new()
        }
    }

    fn document_structure_version(&self, document_id: DocumentId) -> StructureVersion {
        if document_id == self.document_id {
            self.structure_version
        } else {
            0
        }
    }
}

fn sort_key_for_index(index: usize) -> String {
    format!("{index:020}")
}

fn flags_to_i32(flags: u32) -> PostgresStorageResult<i32> {
    i32::try_from(flags).map_err(|_| PostgresStorageError::CorruptData {
        message: format!("block flags {flags} exceed PostgreSQL INTEGER range"),
    })
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
