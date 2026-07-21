use sqlx::{Row, Sqlite};
use uuid::Uuid;

use cditor_core::layout::{PageLayout, PageLayoutIndex, PagePolicy};
use cditor_storage::layout_cache::{LayoutCacheKey, deserialize_confidence, serialize_confidence};
use cditor_storage::{StoragePageLayoutPage, StoragePageLayoutSnapshot, StorageResult};

use crate::error::sqlite_error;
use crate::ids::{block_id_from_sqlite, block_id_to_sqlite, document_id_to_sqlite};
use crate::storage::SqliteDocumentStorage;
use crate::util::{checked_i64, checked_u64};

impl SqliteDocumentStorage {
    pub(crate) async fn load_page_layout_snapshot(
        &self,
        document_id: cditor_core::ids::DocumentId,
        visible_index_version: i64,
        structure_version: u64,
        layout_key: LayoutCacheKey,
        page_policy_version: u64,
    ) -> StorageResult<Option<StoragePageLayoutSnapshot>> {
        let layout_key_hash = layout_key.hash_key();
        let rows = sqlx::query(
            r#"
            SELECT page_index, block_start_index, block_count,
                   first_block_id, last_block_id, height, measured_ratio,
                   confidence, max_error_hint, dirty
            FROM page_layout
            WHERE document_id = ?
              AND visible_index_version = ?
              AND structure_version = ?
              AND layout_key_hash = ?
              AND page_policy_version = ?
            ORDER BY page_index
            "#,
        )
        .bind(document_id_to_sqlite(document_id))
        .bind(visible_index_version)
        .bind(checked_i64(structure_version)?)
        .bind(&layout_key_hash)
        .bind(checked_i64(page_policy_version)?)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlite_error)?;
        if rows.is_empty() {
            return Ok(None);
        }

        let mut pages = Vec::with_capacity(rows.len());
        for row in rows {
            let Ok(page) = page_from_row(&row) else {
                return Ok(None);
            };
            pages.push(page);
        }

        let Some(covered_blocks) = pages
            .last()
            .and_then(|page| page.layout.block_start.checked_add(page.layout.block_count))
        else {
            return Ok(None);
        };
        if PageLayoutIndex::from_cached_pages(
            pages.iter().map(|page| page.layout).collect(),
            PagePolicy::default(),
            covered_blocks,
        )
        .is_err()
        {
            return Ok(None);
        }

        Ok(Some(StoragePageLayoutSnapshot {
            visible_index_version,
            structure_version,
            layout_key_hash,
            page_policy_version,
            pages,
        }))
    }
}

pub(crate) async fn save_page_layout_snapshot(
    transaction: &mut sqlx::Transaction<'_, Sqlite>,
    document_id: Uuid,
    snapshot: &StoragePageLayoutSnapshot,
    now: i64,
) -> StorageResult<()> {
    sqlx::query(
        r#"
        DELETE FROM page_layout
        WHERE document_id = ?
          AND visible_index_version = ?
          AND structure_version = ?
          AND layout_key_hash = ?
          AND page_policy_version = ?
        "#,
    )
    .bind(document_id)
    .bind(snapshot.visible_index_version)
    .bind(checked_i64(snapshot.structure_version)?)
    .bind(&snapshot.layout_key_hash)
    .bind(checked_i64(snapshot.page_policy_version)?)
    .execute(&mut **transaction)
    .await
    .map_err(sqlite_error)?;

    for page in &snapshot.pages {
        sqlx::query(
            r#"
            INSERT INTO page_layout (
                document_id, visible_index_version, structure_version,
                layout_key_hash, page_policy_version, page_index,
                block_start_index, block_count, first_block_id, last_block_id,
                height, measured_ratio, confidence, max_error_hint, dirty, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(document_id)
        .bind(snapshot.visible_index_version)
        .bind(checked_i64(snapshot.structure_version)?)
        .bind(&snapshot.layout_key_hash)
        .bind(checked_i64(snapshot.page_policy_version)?)
        .bind(i64::try_from(page.layout.page_index).map_err(cache_range_error)?)
        .bind(i64::try_from(page.layout.block_start).map_err(cache_range_error)?)
        .bind(i64::try_from(page.layout.block_count).map_err(cache_range_error)?)
        .bind(block_id_to_sqlite(page.first_block_id))
        .bind(block_id_to_sqlite(page.last_block_id))
        .bind(page.layout.height)
        .bind(f64::from(page.layout.measured_ratio))
        .bind(i64::from(serialize_confidence(page.layout.confidence)))
        .bind(page.layout.max_error_hint)
        .bind(i64::from(page.layout.dirty))
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(sqlite_error)?;
    }
    Ok(())
}

fn page_from_row(row: &sqlx::sqlite::SqliteRow) -> StorageResult<StoragePageLayoutPage> {
    let confidence: i64 = row.try_get("confidence").map_err(sqlite_error)?;
    let confidence = u8::try_from(confidence)
        .ok()
        .filter(|value| *value <= 3)
        .map(deserialize_confidence)
        .ok_or_else(|| cache_corrupt("page confidence is outside 0..=3"))?;
    let dirty: i64 = row.try_get("dirty").map_err(sqlite_error)?;
    if !(0..=1).contains(&dirty) {
        return Err(cache_corrupt("page dirty flag is outside 0..=1"));
    }
    let first_block_id: Option<Uuid> = row.try_get("first_block_id").map_err(sqlite_error)?;
    let last_block_id: Option<Uuid> = row.try_get("last_block_id").map_err(sqlite_error)?;
    let first_block_id = first_block_id
        .and_then(block_id_from_sqlite)
        .ok_or_else(|| cache_corrupt("page first block id is missing or invalid"))?;
    let last_block_id = last_block_id
        .and_then(block_id_from_sqlite)
        .ok_or_else(|| cache_corrupt("page last block id is missing or invalid"))?;

    Ok(StoragePageLayoutPage {
        layout: PageLayout {
            page_index: usize::try_from(checked_u64(
                row.try_get("page_index").map_err(sqlite_error)?,
                "page_index",
            )?)
            .map_err(cache_range_error)?,
            block_start: usize::try_from(checked_u64(
                row.try_get("block_start_index").map_err(sqlite_error)?,
                "block_start_index",
            )?)
            .map_err(cache_range_error)?,
            block_count: usize::try_from(checked_u64(
                row.try_get("block_count").map_err(sqlite_error)?,
                "block_count",
            )?)
            .map_err(cache_range_error)?,
            height: row.try_get("height").map_err(sqlite_error)?,
            measured_ratio: row
                .try_get::<f64, _>("measured_ratio")
                .map_err(sqlite_error)? as f32,
            confidence,
            max_error_hint: row.try_get("max_error_hint").map_err(sqlite_error)?,
            dirty: dirty != 0,
        },
        first_block_id,
        last_block_id,
    })
}

fn cache_corrupt(message: &str) -> cditor_storage::StorageError {
    cditor_storage::StorageError::CorruptData(message.to_owned())
}

fn cache_range_error<T>(_error: T) -> cditor_storage::StorageError {
    cache_corrupt("page layout value exceeds the supported integer range")
}
