use std::collections::HashMap;

use sqlx::{PgPool, Row};

use cditor_core::ids::BlockId;
use cditor_core::rich_text::{BlockPayload, BlockPayloadRecord, RichBlockKind};

use crate::error::{PostgresStorageError, PostgresStorageResult};
use crate::types::{
    PgBlockId, PgDocumentId, decode_block_payload, encode_block_payload, pg_block_id_from_runtime,
    rich_block_kind_from_db, rich_block_kind_to_db, runtime_block_id_from_pg,
};

#[derive(Debug, Clone)]
pub struct PostgresPayloadStore {
    pool: PgPool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadBlockPayloadsResult {
    pub records: Vec<BlockPayloadRecord>,
    pub missing_block_ids: Vec<BlockId>,
}

impl PostgresPayloadStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn load_block_payloads(
        &self,
        block_ids: &[BlockId],
    ) -> PostgresStorageResult<LoadBlockPayloadsResult> {
        if block_ids.is_empty() {
            return Ok(LoadBlockPayloadsResult {
                records: Vec::new(),
                missing_block_ids: Vec::new(),
            });
        }

        let pg_block_ids = block_ids
            .iter()
            .copied()
            .map(pg_block_id_from_runtime)
            .collect::<Vec<PgBlockId>>();

        let rows = sqlx::query(
            r#"
            SELECT p.block_id, p.payload_json, p.content_version, b.kind
            FROM block_payloads p
            INNER JOIN blocks b ON b.id = p.block_id
            WHERE p.block_id = ANY($1) AND b.deleted_at IS NULL
            "#,
        )
        .bind(&pg_block_ids)
        .fetch_all(&self.pool)
        .await?;

        let mut by_block_id = HashMap::with_capacity(rows.len());
        for row in rows {
            let pg_block_id: PgBlockId = row.try_get("block_id")?;
            let block_id = runtime_block_id_from_pg(pg_block_id).ok_or_else(|| {
                PostgresStorageError::CorruptData {
                    message: format!("block id {pg_block_id} is outside runtime namespace"),
                }
            })?;
            let payload_json: serde_json::Value = row
                .try_get::<Option<serde_json::Value>, _>("payload_json")?
                .unwrap_or_else(|| serde_json::json!({ "type": "empty" }));
            let content_version: i64 = row.try_get("content_version")?;
            let kind: String = row.try_get("kind")?;
            let content_version =
                u64::try_from(content_version).map_err(|_| PostgresStorageError::CorruptData {
                    message: format!(
                        "block {pg_block_id} has negative content version {content_version}"
                    ),
                })?;
            let payload = decode_block_payload(payload_json)?;

            by_block_id.insert(
                block_id,
                BlockPayloadRecord {
                    block_id,
                    content_version,
                    kind: rich_block_kind_from_db(&kind),
                    payload,
                },
            );
        }

        let mut records = Vec::with_capacity(by_block_id.len());
        let mut missing_block_ids = Vec::new();
        for block_id in block_ids {
            match by_block_id.remove(block_id) {
                Some(record) => records.push(record),
                None => missing_block_ids.push(*block_id),
            }
        }

        Ok(LoadBlockPayloadsResult {
            records,
            missing_block_ids,
        })
    }

    pub async fn count_live_payloads(
        &self,
        document_id: PgDocumentId,
    ) -> PostgresStorageResult<usize> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM block_payloads p
            INNER JOIN blocks b ON b.id = p.block_id
            WHERE p.document_id = $1
              AND b.document_id = $1
              AND b.deleted_at IS NULL
            "#,
        )
        .bind(document_id)
        .fetch_one(&self.pool)
        .await?;
        usize::try_from(count).map_err(|_| PostgresStorageError::CorruptData {
            message: format!("document {document_id} has invalid live payload count {count}"),
        })
    }

    pub async fn save_block_payloads(
        &self,
        document_id: PgDocumentId,
        records: &[BlockPayloadRecord],
    ) -> PostgresStorageResult<()> {
        if records.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for record in records {
            let block_id = pg_block_id_from_runtime(record.block_id);
            let payload_json = encode_block_payload(&record.payload)?;
            let plain_text = record.plain_text();
            let payload_format = payload_format_for_kind(&record.kind);
            let content_version = i64::try_from(record.content_version).map_err(|_| {
                PostgresStorageError::CorruptData {
                    message: format!(
                        "block {} content version {} exceeds PostgreSQL BIGINT range",
                        record.block_id, record.content_version
                    ),
                }
            })?;
            let byte_len = i64::try_from(payload_json.to_string().len()).map_err(|_| {
                PostgresStorageError::CorruptData {
                    message: format!(
                        "block {} payload json length exceeds BIGINT",
                        record.block_id
                    ),
                }
            })?;
            let inline_run_count =
                i32::try_from(inline_run_count(&record.payload)).map_err(|_| {
                    PostgresStorageError::CorruptData {
                        message: format!(
                            "block {} inline run count exceeds INTEGER",
                            record.block_id
                        ),
                    }
                })?;
            let kind = rich_block_kind_to_db(&record.kind);

            let updated_block = sqlx::query(
                r#"
                UPDATE blocks
                SET kind = $3, content_version = $4, updated_at = now()
                WHERE id = $1 AND document_id = $2 AND deleted_at IS NULL
                "#,
            )
            .bind(block_id)
            .bind(document_id)
            .bind(&kind)
            .bind(content_version)
            .execute(&mut *tx)
            .await?;

            if updated_block.rows_affected() == 0 {
                return Err(PostgresStorageError::NotFound {
                    entity: "block",
                    id: block_id.to_string(),
                });
            }

            sqlx::query(
                r#"
                INSERT INTO block_payloads (
                    block_id,
                    document_id,
                    payload_format,
                    payload_json,
                    plain_text,
                    content_hash,
                    content_version,
                    byte_len,
                    inline_run_count,
                    updated_at
                )
                VALUES ($1, $2, $3, $4, $5, NULL, $6, $7, $8, now())
                ON CONFLICT (block_id) DO UPDATE SET
                    document_id = EXCLUDED.document_id,
                    payload_format = EXCLUDED.payload_format,
                    payload_json = EXCLUDED.payload_json,
                    plain_text = EXCLUDED.plain_text,
                    content_hash = EXCLUDED.content_hash,
                    content_version = EXCLUDED.content_version,
                    byte_len = EXCLUDED.byte_len,
                    inline_run_count = EXCLUDED.inline_run_count,
                    updated_at = now()
                "#,
            )
            .bind(block_id)
            .bind(document_id)
            .bind(payload_format)
            .bind(&payload_json)
            .bind(&plain_text)
            .bind(content_version)
            .bind(byte_len)
            .bind(inline_run_count)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                r#"
                INSERT INTO block_search (
                    block_id,
                    document_id,
                    kind,
                    plain_text,
                    search_vector,
                    content_version,
                    indexed_at
                )
                VALUES ($1, $2, $3, $4, to_tsvector('simple', $4), $5, now())
                ON CONFLICT (block_id) DO UPDATE SET
                    document_id = EXCLUDED.document_id,
                    kind = EXCLUDED.kind,
                    plain_text = EXCLUDED.plain_text,
                    search_vector = EXCLUDED.search_vector,
                    content_version = EXCLUDED.content_version,
                    indexed_at = now()
                "#,
            )
            .bind(block_id)
            .bind(document_id)
            .bind(&kind)
            .bind(&plain_text)
            .bind(content_version)
            .execute(&mut *tx)
            .await?;
        }

        let max_content_version = records
            .iter()
            .map(|record| record.content_version)
            .max()
            .unwrap_or(1);
        let max_content_version =
            i64::try_from(max_content_version).map_err(|_| PostgresStorageError::CorruptData {
                message: "document content version exceeds PostgreSQL BIGINT range".to_owned(),
            })?;

        sqlx::query(
            r#"
            UPDATE documents
            SET content_version = GREATEST(content_version, $2), updated_at = now()
            WHERE id = $1 AND deleted_at IS NULL
            "#,
        )
        .bind(document_id)
        .bind(max_content_version)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }
}

fn payload_format_for_kind(kind: &RichBlockKind) -> &'static str {
    match kind {
        RichBlockKind::Code { .. } => "code_json_v1",
        RichBlockKind::Table => "table_json_v1",
        RichBlockKind::Image => "image_json_v1",
        RichBlockKind::File | RichBlockKind::Attachment => "file_json_v1",
        RichBlockKind::Whiteboard | RichBlockKind::MindMap => "canvas_json_v1",
        RichBlockKind::Embed => "embed_json_v1",
        RichBlockKind::Html => "html_json_v1",
        _ => "rich_text_json_v1",
    }
}

fn inline_run_count(payload: &BlockPayload) -> usize {
    match payload {
        BlockPayload::RichText { spans } => spans.len(),
        BlockPayload::Code { .. } => 1,
        BlockPayload::Table(table) => table
            .rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .map(|cell| cell.spans.len())
            .sum(),
        BlockPayload::Image(_) | BlockPayload::File(_) | BlockPayload::Whiteboard(_) => 0,
        BlockPayload::Embed(_) | BlockPayload::Html { .. } => 1,
        BlockPayload::Empty => 0,
    }
}

#[cfg(test)]
mod tests {
    use sqlx::types::Uuid;

    use super::*;
    use crate::{
        DocumentRow, PostgresDocumentStore, PostgresPoolConfig, create_pg_pool,
        pg_document_id_from_runtime, run_migrations,
    };
    use cditor_core::document::BlockIndexRecord;
    use cditor_core::rich_text::{
        ImagePayload, InlineMark, InlineSpan, TableCellPayload, TableHeaderStyle, TablePayload,
        TableRowPayload, kind_tag_for_rich_block_kind,
    };

    fn test_database_url() -> String {
        std::env::var("CDITOR_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned())
    }

    async fn test_stores() -> (PostgresDocumentStore, PostgresPayloadStore) {
        let config = PostgresPoolConfig::for_tests(test_database_url());
        let pool = create_pg_pool(&config).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (
            PostgresDocumentStore::new(pool.clone()),
            PostgresPayloadStore::new(pool),
        )
    }

    fn document_row(document_id: u64) -> DocumentRow {
        DocumentRow {
            id: pg_document_id_from_runtime(document_id),
            workspace_id: Uuid::from_u128(
                0x9100_0000_0000_0000_0000_0000_0000_0000 | document_id as u128,
            ),
            title: format!("Payload Store {document_id}"),
            structure_version: 1,
            content_version: 1,
            layout_version: 0,
            schema_version: 1,
        }
    }

    fn block_records(base_block_id: u64, kinds: &[RichBlockKind]) -> Vec<BlockIndexRecord> {
        kinds
            .iter()
            .enumerate()
            .map(|(index, kind)| {
                BlockIndexRecord::new(
                    base_block_id + index as u64,
                    None,
                    0,
                    kind_tag_for_rich_block_kind(kind),
                    0,
                )
            })
            .collect()
    }

    async fn seed_document_with_blocks(
        document_id: u64,
        base_block_id: u64,
        kinds: &[RichBlockKind],
    ) -> (PostgresDocumentStore, PostgresPayloadStore, DocumentRow) {
        let (document_store, payload_store) = test_stores().await;
        let document = document_row(document_id);
        let blocks = block_records(base_block_id, kinds);
        document_store
            .save_document_metadata(&document)
            .await
            .unwrap();
        document_store
            .save_block_index_records(document.id, &blocks, 2)
            .await
            .unwrap();
        (document_store, payload_store, document)
    }

    fn rich_text_record(block_id: u64) -> BlockPayloadRecord {
        BlockPayloadRecord {
            block_id,
            content_version: 2,
            kind: RichBlockKind::Paragraph,
            payload: BlockPayload::RichText {
                spans: vec![
                    InlineSpan {
                        text: "hello ".to_owned(),
                        marks: vec![InlineMark::Bold],
                    },
                    InlineSpan {
                        text: "postgres".to_owned(),
                        marks: vec![InlineMark::Link {
                            href: "https://postgresql.org".to_owned(),
                        }],
                    },
                ],
            },
        }
    }

    fn code_record(block_id: u64) -> BlockPayloadRecord {
        BlockPayloadRecord {
            block_id,
            content_version: 3,
            kind: RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            payload: BlockPayload::Code {
                language: Some("rust".to_owned()),
                text: "fn main() {}".to_owned(),
            },
        }
    }

    fn table_record(block_id: u64) -> BlockPayloadRecord {
        BlockPayloadRecord {
            block_id,
            content_version: 4,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(TablePayload {
                header_rows: 1,
                header_cols: 0,
                header_style: TableHeaderStyle::default(),
                columns: Vec::new(),
                rows: vec![TableRowPayload {
                    cells: vec![
                        TableCellPayload::plain("name"),
                        TableCellPayload::plain("value"),
                    ],
                    height: Default::default(),
                }],
            }),
        }
    }

    fn image_record(block_id: u64) -> BlockPayloadRecord {
        BlockPayloadRecord {
            block_id,
            content_version: 5,
            kind: RichBlockKind::Image,
            payload: BlockPayload::Image(ImagePayload {
                source: "asset://image-1".to_owned(),
                alt: "diagram".to_owned(),
                caption: "architecture".to_owned(),
                display_width_ratio_milli: None,
            }),
        }
    }

    #[test]
    fn inline_run_count_handles_nested_payloads() {
        assert_eq!(inline_run_count(&rich_text_record(1).payload), 2);
        assert_eq!(inline_run_count(&table_record(2).payload), 2);
        assert_eq!(inline_run_count(&image_record(3).payload), 0);
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_payload_store_round_trips_rich_text_code_table_and_image() {
        let base = 600_000;
        let kinds = [
            RichBlockKind::Paragraph,
            RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            RichBlockKind::Table,
            RichBlockKind::Image,
        ];
        let (_document_store, payload_store, document) =
            seed_document_with_blocks(40_001, base, &kinds).await;
        let records = vec![
            rich_text_record(base),
            code_record(base + 1),
            table_record(base + 2),
            image_record(base + 3),
        ];

        payload_store
            .save_block_payloads(document.id, &records)
            .await
            .unwrap();

        let loaded = payload_store
            .load_block_payloads(&[base + 3, base, base + 2, base + 1])
            .await
            .unwrap();

        assert!(loaded.missing_block_ids.is_empty());
        assert_eq!(loaded.records.len(), 4);
        assert_eq!(loaded.records[0], records[3]);
        assert_eq!(loaded.records[1], records[0]);
        assert_eq!(loaded.records[2], records[2]);
        assert_eq!(loaded.records[3], records[1]);
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_payload_store_reports_missing_payloads_without_failing_window_load() {
        let base = 601_000;
        let kinds = [RichBlockKind::Paragraph, RichBlockKind::Paragraph];
        let (_document_store, payload_store, document) =
            seed_document_with_blocks(40_002, base, &kinds).await;
        let saved = rich_text_record(base);

        payload_store
            .save_block_payloads(document.id, &[saved.clone()])
            .await
            .unwrap();

        let loaded = payload_store
            .load_block_payloads(&[base, base + 1, base + 99])
            .await
            .unwrap();

        assert_eq!(loaded.records, vec![saved]);
        assert_eq!(loaded.missing_block_ids, vec![base + 1, base + 99]);
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_payload_store_syncs_plain_text_to_payload_and_search_tables() {
        let base = 602_000;
        let kinds = [RichBlockKind::Paragraph];
        let (_document_store, payload_store, document) =
            seed_document_with_blocks(40_003, base, &kinds).await;
        let saved = rich_text_record(base);

        payload_store
            .save_block_payloads(document.id, &[saved.clone()])
            .await
            .unwrap();

        let payload_plain_text: String =
            sqlx::query_scalar("SELECT plain_text FROM block_payloads WHERE block_id = $1")
                .bind(pg_block_id_from_runtime(base))
                .fetch_one(payload_store.pool())
                .await
                .unwrap();
        let search_plain_text: String =
            sqlx::query_scalar("SELECT plain_text FROM block_search WHERE block_id = $1")
                .bind(pg_block_id_from_runtime(base))
                .fetch_one(payload_store.pool())
                .await
                .unwrap();

        assert_eq!(payload_plain_text, saved.plain_text());
        assert_eq!(search_plain_text, saved.plain_text());
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_payload_store_rejects_payload_for_missing_or_deleted_block() {
        let (_document_store, payload_store) = test_stores().await;
        let error = payload_store
            .save_block_payloads(
                pg_document_id_from_runtime(40_004),
                &[rich_text_record(603_000)],
            )
            .await
            .unwrap_err();

        assert!(matches!(error, PostgresStorageError::NotFound { .. }));
    }
}
