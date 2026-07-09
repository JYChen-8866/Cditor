use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;
use sqlx::{PgPool, Row};

use cditor_core::document::{BlockIndexRecord, DocumentIndex};
use cditor_core::ids::DocumentId;
use cditor_core::layout::BlockLayoutMeta;
use cditor_core::rich_text::{kind_tag_for_rich_block_kind, rich_block_kind_from_tag};
use cditor_core::version::StructureVersion;
use cditor_storage::traits::DocumentIndexStore;

use crate::error::{PostgresStorageError, PostgresStorageResult};
use crate::types::{
    DocumentRow, PgBlockId, PgDocumentId, pg_block_id_from_runtime, rich_block_kind_from_db,
    rich_block_kind_to_db, runtime_block_id_from_pg, runtime_document_id_from_pg,
};

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

    pub async fn save_document_index_snapshot(
        &self,
        document_id: PgDocumentId,
        visible_index_version: i64,
        structure_version: i64,
        records: &[BlockIndexRecord],
    ) -> PostgresStorageResult<()> {
        let snapshot = DbDocumentIndexSnapshot {
            records: records
                .iter()
                .copied()
                .map(DbBlockIndexSnapshotRecord::from)
                .collect(),
        };
        let snapshot_bytes = serde_json::to_vec(&snapshot)?;
        let block_count =
            i32::try_from(records.len()).map_err(|_| PostgresStorageError::CorruptData {
                message: format!(
                    "document index snapshot block_count {} exceeds INTEGER range",
                    records.len()
                ),
            })?;

        sqlx::query(
            r#"
            INSERT INTO document_index_snapshot (
                document_id,
                visible_index_version,
                structure_version,
                snapshot_format,
                snapshot_bytes,
                block_count,
                created_at
            )
            VALUES ($1, $2, $3, 'block_index_json_v1', $4, $5, now())
            ON CONFLICT (document_id, visible_index_version, structure_version) DO UPDATE SET
                snapshot_format = EXCLUDED.snapshot_format,
                snapshot_bytes = EXCLUDED.snapshot_bytes,
                block_count = EXCLUDED.block_count,
                created_at = now()
            "#,
        )
        .bind(document_id)
        .bind(visible_index_version)
        .bind(structure_version)
        .bind(snapshot_bytes)
        .bind(block_count)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn load_document_index_snapshot(
        &self,
        document_id: PgDocumentId,
        visible_index_version: i64,
        structure_version: i64,
    ) -> PostgresStorageResult<Option<Vec<BlockIndexRecord>>> {
        let row = sqlx::query(
            r#"
            SELECT snapshot_format, snapshot_bytes, block_count
            FROM document_index_snapshot
            WHERE document_id = $1 AND visible_index_version = $2 AND structure_version = $3
            "#,
        )
        .bind(document_id)
        .bind(visible_index_version)
        .bind(structure_version)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };
        let snapshot_format: String = row.try_get("snapshot_format")?;
        if snapshot_format != "block_index_json_v1" {
            return Err(PostgresStorageError::CorruptData {
                message: format!("unsupported document index snapshot format {snapshot_format}"),
            });
        }
        let snapshot_bytes: Vec<u8> = row.try_get("snapshot_bytes")?;
        let expected_block_count: i32 = row.try_get("block_count")?;
        let snapshot: DbDocumentIndexSnapshot = serde_json::from_slice(&snapshot_bytes)?;
        if snapshot.records.len() != usize::try_from(expected_block_count).unwrap_or(usize::MAX) {
            return Err(PostgresStorageError::CorruptData {
                message: format!(
                    "document index snapshot block_count mismatch: expected {expected_block_count}, found {}",
                    snapshot.records.len()
                ),
            });
        }

        Ok(Some(
            snapshot
                .records
                .into_iter()
                .map(BlockIndexRecord::from)
                .collect(),
        ))
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DbDocumentIndexSnapshot {
    records: Vec<DbBlockIndexSnapshotRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct DbBlockIndexSnapshotRecord {
    id: u64,
    parent_id: Option<u64>,
    depth: u16,
    kind_tag: u16,
    flags: u32,
    estimated_height: u64,
    measured_height: Option<u64>,
    width_bucket: u16,
    layout_version: u64,
    dirty: bool,
}

impl From<BlockIndexRecord> for DbBlockIndexSnapshotRecord {
    fn from(record: BlockIndexRecord) -> Self {
        Self {
            id: record.id,
            parent_id: record.parent_id,
            depth: record.depth,
            kind_tag: record.kind_tag,
            flags: record.flags,
            estimated_height: record.layout_meta.estimated_height.to_bits(),
            measured_height: record.layout_meta.measured_height.map(f64::to_bits),
            width_bucket: record.layout_meta.width_bucket,
            layout_version: record.layout_meta.layout_version,
            dirty: record.layout_meta.dirty,
        }
    }
}

impl From<DbBlockIndexSnapshotRecord> for BlockIndexRecord {
    fn from(record: DbBlockIndexSnapshotRecord) -> Self {
        BlockIndexRecord::new(
            record.id,
            record.parent_id,
            record.depth,
            record.kind_tag,
            record.flags,
        )
        .with_layout_meta(BlockLayoutMeta {
            block_id: record.id,
            estimated_height: f64::from_bits(record.estimated_height),
            measured_height: record.measured_height.map(f64::from_bits),
            width_bucket: record.width_bucket,
            layout_version: record.layout_version,
            dirty: record.dirty,
        })
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
mod tests {
    use super::*;
    use crate::{PostgresPoolConfig, create_pg_pool, pg_document_id_from_runtime, run_migrations};
    use cditor_core::rich_text::RichBlockKind;
    use cditor_core::rich_text::kind_tag_for_rich_block_kind;

    fn test_database_url() -> String {
        std::env::var("CDITOR_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned())
    }

    async fn test_store() -> PostgresDocumentStore {
        let config = PostgresPoolConfig::for_tests(test_database_url());
        let pool = create_pg_pool(&config).await.unwrap();
        run_migrations(&pool).await.unwrap();
        PostgresDocumentStore::new(pool)
    }

    fn document_row(document_id: DocumentId, title: &str) -> DocumentRow {
        DocumentRow {
            id: pg_document_id_from_runtime(document_id),
            workspace_id: Uuid::from_u128(
                0x9000_0000_0000_0000_0000_0000_0000_0000 | document_id as u128,
            ),
            title: title.to_owned(),
            structure_version: 1,
            content_version: 1,
            layout_version: 0,
            schema_version: 1,
        }
    }

    fn sample_records(base_block_id: u64) -> Vec<BlockIndexRecord> {
        vec![
            BlockIndexRecord::new(
                base_block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Heading { level: 1 }),
                0,
            ),
            BlockIndexRecord::new(
                base_block_id + 1,
                Some(base_block_id),
                1,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                1,
            ),
            BlockIndexRecord::new(
                base_block_id + 2,
                Some(base_block_id),
                1,
                kind_tag_for_rich_block_kind(&RichBlockKind::Todo { checked: false }),
                0,
            ),
        ]
    }

    #[test]
    fn sort_keys_preserve_lexicographic_order() {
        assert!(sort_key_for_index(9) < sort_key_for_index(10));
        assert!(sort_key_for_index(99) < sort_key_for_index(100));
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_document_store_saves_and_loads_document_metadata() {
        let store = test_store().await;
        let document = document_row(30_001, "Postgres document metadata");

        store.save_document_metadata(&document).await.unwrap();

        let loaded = store.load_document_metadata(document.id).await.unwrap();
        assert_eq!(loaded, document);
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_document_store_saves_and_loads_block_structure_index() {
        let store = test_store().await;
        let document = document_row(30_002, "Postgres block index");
        let records = sample_records(300_020);

        store.save_document_metadata(&document).await.unwrap();
        store
            .save_block_index_records(document.id, &records, 2)
            .await
            .unwrap();

        let loaded = store.load_block_index_records(document.id).await.unwrap();
        assert_eq!(loaded.len(), records.len());
        assert_eq!(loaded[0].id, records[0].id);
        assert_eq!(loaded[1].parent_id, Some(300_020));
        assert_eq!(loaded[1].flags, 1);
        assert_eq!(loaded[2].kind_tag, records[2].kind_tag);

        let metadata = store.load_document_metadata(document.id).await.unwrap();
        assert_eq!(metadata.structure_version, 2);
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_document_store_soft_delete_excludes_block_from_index() {
        let store = test_store().await;
        let document = document_row(30_003, "Postgres soft delete");
        let records = sample_records(300_030);

        store.save_document_metadata(&document).await.unwrap();
        store
            .save_block_index_records(document.id, &records, 2)
            .await
            .unwrap();
        store
            .soft_delete_block(pg_block_id_from_runtime(300_031))
            .await
            .unwrap();

        let loaded = store.load_block_index_records(document.id).await.unwrap();
        assert_eq!(
            loaded.iter().map(|record| record.id).collect::<Vec<_>>(),
            vec![300_030, 300_032]
        );
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_document_store_detects_structure_version_conflicts() {
        let store = test_store().await;
        let document = document_row(30_004, "Postgres structure version");

        store.save_document_metadata(&document).await.unwrap();
        store
            .update_document_structure_version(document.id, 1, 2)
            .await
            .unwrap();

        let conflict = store
            .update_document_structure_version(document.id, 1, 3)
            .await
            .unwrap_err();
        assert!(matches!(conflict, PostgresStorageError::Conflict { .. }));
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL; inserts and loads 100k blocks"]
    async fn postgres_document_store_loads_100k_block_index_without_payloads() {
        let store = test_store().await;
        let document = document_row(30_005, "Postgres 100k block index");
        let paragraph = kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph);
        let first_block_id = 500_000;
        let records = (first_block_id..first_block_id + 100_000)
            .map(|id| BlockIndexRecord::new(id, None, 0, paragraph, 0))
            .collect::<Vec<_>>();

        store.save_document_metadata(&document).await.unwrap();
        store
            .save_block_index_records(document.id, &records, 2)
            .await
            .unwrap();

        let loaded = store.load_block_index_records(document.id).await.unwrap();
        assert_eq!(loaded.len(), 100_000);
        assert_eq!(loaded.first().map(|record| record.id), Some(first_block_id));
        assert_eq!(
            loaded.last().map(|record| record.id),
            Some(first_block_id + 99_999)
        );
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_document_index_snapshot_implements_sync_document_index_store() {
        let store = test_store().await;
        let document = document_row(30_006, "Postgres document index snapshot");
        let records = sample_records(300_060);

        store.save_document_metadata(&document).await.unwrap();
        store
            .save_block_index_records(document.id, &records, 7)
            .await
            .unwrap();

        let snapshot = PostgresDocumentIndexSnapshot::load(&store, document.id)
            .await
            .unwrap();
        let index = DocumentIndex::from_store(30_006, &snapshot).unwrap();

        assert_eq!(index.total_count(), records.len());
        assert_eq!(index.structure_version, 7);
        assert_eq!(index.id_at(2), Some(300_062));
    }
}
