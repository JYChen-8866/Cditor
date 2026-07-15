use sqlx::Postgres;

use cditor_core::ids::DocumentId;
use cditor_core::version::StructureVersion;
use cditor_storage::StoragePageLayoutSnapshot;
use cditor_storage::layout_cache::{PageLayoutRow, serialize_confidence};

use crate::error::{PostgresStorageError, PostgresStorageResult};
use crate::types::{PgDocumentId, pg_block_id_from_runtime, pg_document_id_from_runtime};

use super::{
    PostgresLayoutCacheStore, i32_from_usize, i64_from_u64, page_layout_row_from_pg,
    page_layout_select_sql,
};

impl PostgresLayoutCacheStore {
    pub async fn load_page_layout_rows(
        &self,
        document_id: DocumentId,
        visible_index_version: u64,
        structure_version: StructureVersion,
        layout_key_hash: &str,
        page_policy_version: u64,
    ) -> PostgresStorageResult<Vec<PageLayoutRow>> {
        let sql = page_layout_select_sql(
            r#"
            WHERE document_id = $1
              AND visible_index_version = $2
              AND structure_version = $3
              AND layout_key_hash = $4
              AND page_policy_version = $5
            ORDER BY page_index
            "#,
        );
        sqlx::query(&sql)
            .bind(pg_document_id_from_runtime(document_id))
            .bind(i64_from_u64(
                visible_index_version,
                "visible_index_version",
            )?)
            .bind(i64_from_u64(structure_version, "structure_version")?)
            .bind(layout_key_hash)
            .bind(i64_from_u64(page_policy_version, "page_policy_version")?)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(page_layout_row_from_pg)
            .collect()
    }

    pub async fn save_page_layout_snapshot_tx(
        &self,
        transaction: &mut sqlx::Transaction<'_, Postgres>,
        document_id: PgDocumentId,
        snapshot: &StoragePageLayoutSnapshot,
    ) -> PostgresStorageResult<()> {
        sqlx::query(
            r#"
            DELETE FROM page_layout
            WHERE document_id = $1
              AND visible_index_version = $2
              AND structure_version = $3
              AND layout_key_hash = $4
              AND page_policy_version = $5
            "#,
        )
        .bind(document_id)
        .bind(nonnegative_i64(
            snapshot.visible_index_version,
            "visible_index_version",
        )?)
        .bind(i64_from_u64(
            snapshot.structure_version,
            "structure_version",
        )?)
        .bind(&snapshot.layout_key_hash)
        .bind(i64_from_u64(
            snapshot.page_policy_version,
            "page_policy_version",
        )?)
        .execute(&mut **transaction)
        .await?;

        for page in &snapshot.pages {
            sqlx::query(
                r#"
                INSERT INTO page_layout (
                    document_id, visible_index_version, structure_version,
                    layout_key_hash, page_policy_version, page_index,
                    block_start_index, block_count, first_block_id, last_block_id,
                    height, measured_ratio, confidence, max_error_hint, dirty, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, now())
                "#,
            )
            .bind(document_id)
            .bind(nonnegative_i64(
                snapshot.visible_index_version,
                "visible_index_version",
            )?)
            .bind(i64_from_u64(
                snapshot.structure_version,
                "structure_version",
            )?)
            .bind(&snapshot.layout_key_hash)
            .bind(i64_from_u64(
                snapshot.page_policy_version,
                "page_policy_version",
            )?)
            .bind(i32_from_usize(page.layout.page_index, "page_index")?)
            .bind(i32_from_usize(
                page.layout.block_start,
                "block_start_index",
            )?)
            .bind(i32_from_usize(page.layout.block_count, "block_count")?)
            .bind(pg_block_id_from_runtime(page.first_block_id))
            .bind(pg_block_id_from_runtime(page.last_block_id))
            .bind(page.layout.height)
            .bind(f64::from(page.layout.measured_ratio))
            .bind(i32::from(serialize_confidence(page.layout.confidence)))
            .bind(page.layout.max_error_hint)
            .bind(page.layout.dirty)
            .execute(&mut **transaction)
            .await?;
        }
        Ok(())
    }
}

fn nonnegative_i64(value: i64, name: &'static str) -> PostgresStorageResult<i64> {
    if value >= 0 {
        Ok(value)
    } else {
        Err(PostgresStorageError::CorruptData {
            message: format!("{name} is negative: {value}"),
        })
    }
}
