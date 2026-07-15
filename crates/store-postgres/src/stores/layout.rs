use std::collections::HashMap;

use sqlx::{PgPool, Row};

use cditor_core::ids::{BlockId, DocumentId};
use cditor_core::layout::HeightConfidence;
use cditor_core::version::StructureVersion;
use cditor_storage::height_write_debounce::{HeightWrite, HeightWriteError};
use cditor_storage::layout_cache::{
    BlockLayoutRow, CacheSource, CachedHeight, CachedPageHeight, LayoutCacheKey, PageLayoutRow,
    deserialize_confidence, serialize_confidence,
};

use crate::error::{PostgresStorageError, PostgresStorageResult};
use crate::types::{
    PgBlockId, PgDocumentId, pg_block_id_from_runtime, pg_document_id_from_runtime,
    runtime_block_id_from_pg,
};

mod page_snapshot;

#[derive(Debug, Clone)]
pub struct PostgresLayoutCacheStore {
    pool: PgPool,
}

impl PostgresLayoutCacheStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn save_block_layout(
        &self,
        document_id: PgDocumentId,
        row: &BlockLayoutRow,
    ) -> PostgresStorageResult<()> {
        let block_id = pg_block_id_from_runtime(row.block_id);
        let current_content_version = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT content_version
            FROM blocks
            WHERE id = $1 AND document_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(block_id)
        .bind(document_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| PostgresStorageError::NotFound {
            entity: "block",
            id: block_id.to_string(),
        })?;

        let row_content_version =
            i64_from_u64(row.content_version, "block layout content_version")?;
        if row_content_version < current_content_version {
            return Err(PostgresStorageError::Conflict {
                message: format!(
                    "stale block layout for block {block_id}: layout content_version {row_content_version}, block content_version {current_content_version}"
                ),
            });
        }

        sqlx::query(
            r#"
            INSERT INTO block_layout (
                block_id,
                document_id,
                layout_key_hash,
                width_bucket,
                exact_width,
                content_version,
                attrs_version,
                style_version,
                font_version,
                theme_version,
                scale_factor,
                measured_height,
                estimated_height,
                confidence,
                max_error_hint,
                line_count,
                layout_cost,
                measured_at,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, CASE WHEN $18 THEN now() ELSE NULL END, now())
            ON CONFLICT (block_id, layout_key_hash) DO UPDATE SET
                document_id = EXCLUDED.document_id,
                width_bucket = EXCLUDED.width_bucket,
                exact_width = EXCLUDED.exact_width,
                content_version = EXCLUDED.content_version,
                attrs_version = EXCLUDED.attrs_version,
                style_version = EXCLUDED.style_version,
                font_version = EXCLUDED.font_version,
                theme_version = EXCLUDED.theme_version,
                scale_factor = EXCLUDED.scale_factor,
                measured_height = EXCLUDED.measured_height,
                estimated_height = EXCLUDED.estimated_height,
                confidence = EXCLUDED.confidence,
                max_error_hint = EXCLUDED.max_error_hint,
                line_count = EXCLUDED.line_count,
                layout_cost = EXCLUDED.layout_cost,
                measured_at = EXCLUDED.measured_at,
                updated_at = now()
            "#,
        )
        .bind(block_id)
        .bind(document_id)
        .bind(&row.layout_key_hash)
        .bind(i32::from(row.width_bucket))
        .bind(row.exact_width_px as f64)
        .bind(row_content_version)
        .bind(i64_from_u64(row.attrs_version, "attrs_version")?)
        .bind(i64_from_u64(row.style_version, "style_version")?)
        .bind(i64_from_u64(row.font_version, "font_version")?)
        .bind(i64_from_u64(row.theme_version, "theme_version")?)
        .bind(row.scale_factor_milli as f64 / 1000.0)
        .bind(row.measured_height)
        .bind(row.estimated_height)
        .bind(i32::from(serialize_confidence(row.confidence)))
        .bind(row.max_error_hint)
        .bind(optional_i32_from_u32(row.line_count, "line_count")?)
        .bind(i32_from_u32(row.layout_cost, "layout_cost")?)
        .bind(row.measured_at.is_some())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn load_block_height(
        &self,
        block_id: BlockId,
        key: LayoutCacheKey,
    ) -> PostgresStorageResult<CachedHeight> {
        let exact_hash = key.hash_key();
        let pg_block_id = pg_block_id_from_runtime(block_id);

        if let Some(row) = self
            .load_block_layout_row(pg_block_id, Some(&exact_hash))
            .await?
        {
            return Ok(row.load_for_key(key));
        }

        self.load_block_layout_row(pg_block_id, None)
            .await?
            .map(|row| row.load_for_key(key))
            .map(Ok)
            .unwrap_or_else(|| Ok(missing_block_height()))
    }

    /// Loads the best cached height for every requested block with one database round trip.
    ///
    /// An exact layout-key match wins. When no exact row exists, the newest measured row is
    /// returned as an estimate, matching [`Self::load_block_height`] without issuing two queries
    /// per block during large-document cold start.
    pub async fn load_block_heights(
        &self,
        block_ids: &[BlockId],
        key: LayoutCacheKey,
    ) -> PostgresStorageResult<HashMap<BlockId, CachedHeight>> {
        if block_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let pg_block_ids = block_ids
            .iter()
            .copied()
            .map(pg_block_id_from_runtime)
            .collect::<Vec<_>>();
        let exact_hash = key.hash_key();
        let sql = batch_block_layout_select_sql();
        let rows = sqlx::query(&sql)
            .bind(&pg_block_ids)
            .bind(&exact_hash)
            .fetch_all(&self.pool)
            .await?;

        let mut heights = HashMap::with_capacity(rows.len());
        for row in rows {
            let row = block_layout_row_from_pg(row)?;
            heights.insert(row.block_id, row.load_for_key(key));
        }
        Ok(heights)
    }

    pub async fn save_page_layout(&self, row: &PageLayoutRow) -> PostgresStorageResult<()> {
        let document_id = pg_document_id_from_runtime(row.document_id);
        sqlx::query(
            r#"
            INSERT INTO page_layout (
                document_id,
                visible_index_version,
                structure_version,
                layout_key_hash,
                page_policy_version,
                page_index,
                block_start_index,
                block_count,
                first_block_id,
                last_block_id,
                height,
                measured_ratio,
                confidence,
                max_error_hint,
                dirty,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, now())
            ON CONFLICT (document_id, visible_index_version, structure_version, layout_key_hash, page_policy_version, page_index)
            DO UPDATE SET
                block_start_index = EXCLUDED.block_start_index,
                block_count = EXCLUDED.block_count,
                first_block_id = EXCLUDED.first_block_id,
                last_block_id = EXCLUDED.last_block_id,
                height = EXCLUDED.height,
                measured_ratio = EXCLUDED.measured_ratio,
                confidence = EXCLUDED.confidence,
                max_error_hint = EXCLUDED.max_error_hint,
                dirty = EXCLUDED.dirty,
                updated_at = now()
            "#,
        )
        .bind(document_id)
        .bind(i64_from_u64(row.visible_index_version, "visible_index_version")?)
        .bind(i64_from_u64(row.structure_version, "structure_version")?)
        .bind(&row.layout_key_hash)
        .bind(i64_from_u64(row.page_policy_version, "page_policy_version")?)
        .bind(i32_from_usize(row.page_index, "page_index")?)
        .bind(i32_from_usize(row.block_start_index, "block_start_index")?)
        .bind(i32_from_usize(row.block_count, "block_count")?)
        .bind(row.first_block_id.map(pg_block_id_from_runtime))
        .bind(row.last_block_id.map(pg_block_id_from_runtime))
        .bind(row.height)
        .bind(row.measured_ratio)
        .bind(i32::from(serialize_confidence(row.confidence)))
        .bind(row.max_error_hint)
        .bind(row.dirty)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn flush_height_writes(
        &self,
        document_id: PgDocumentId,
        writes: &[HeightWrite],
    ) -> Result<(), HeightWriteError> {
        for write in writes {
            let result = match write {
                HeightWrite::Block(row) => self.save_block_layout(document_id, row).await,
                HeightWrite::Page(row) => self.save_page_layout(row).await,
            };
            if result.is_err() {
                return Err(HeightWriteError::SinkFailed(
                    "postgres layout cache write failed",
                ));
            }
        }
        Ok(())
    }

    pub async fn load_page_height(
        &self,
        document_id: DocumentId,
        visible_index_version: u64,
        structure_version: StructureVersion,
        layout_key_hash: &str,
        page_policy_version: u64,
        page_index: usize,
    ) -> PostgresStorageResult<CachedPageHeight> {
        let pg_document_id = pg_document_id_from_runtime(document_id);

        if let Some(row) = self
            .load_page_layout_row(
                pg_document_id,
                Some((
                    visible_index_version,
                    structure_version,
                    layout_key_hash,
                    page_policy_version,
                    page_index,
                )),
            )
            .await?
        {
            return Ok(row.load_for_context(
                structure_version,
                layout_key_hash,
                page_policy_version,
            ));
        }

        self.load_page_layout_row(pg_document_id, None)
            .await?
            .map(|row| {
                row.load_for_context(structure_version, layout_key_hash, page_policy_version)
            })
            .map(Ok)
            .unwrap_or_else(|| Ok(missing_page_height()))
    }

    async fn load_block_layout_row(
        &self,
        block_id: PgBlockId,
        exact_hash: Option<&str>,
    ) -> PostgresStorageResult<Option<BlockLayoutRow>> {
        let row = match exact_hash {
            Some(exact_hash) => {
                let sql = block_layout_select_sql("WHERE block_id = $1 AND layout_key_hash = $2");
                sqlx::query(&sql)
                    .bind(block_id)
                    .bind(exact_hash)
                    .fetch_optional(&self.pool)
                    .await?
            }
            None => {
                let sql = block_layout_select_sql(
                    "WHERE block_id = $1 ORDER BY measured_at DESC NULLS LAST, updated_at DESC LIMIT 1",
                );
                sqlx::query(&sql)
                    .bind(block_id)
                    .fetch_optional(&self.pool)
                    .await?
            }
        };

        row.map(block_layout_row_from_pg).transpose()
    }

    async fn load_page_layout_row(
        &self,
        document_id: PgDocumentId,
        exact: Option<(u64, StructureVersion, &str, u64, usize)>,
    ) -> PostgresStorageResult<Option<PageLayoutRow>> {
        let row = match exact {
            Some((
                visible_index_version,
                structure_version,
                layout_key_hash,
                page_policy_version,
                page_index,
            )) => {
                let sql = page_layout_select_sql(
                    r#"
                    WHERE document_id = $1
                      AND visible_index_version = $2
                      AND structure_version = $3
                      AND layout_key_hash = $4
                      AND page_policy_version = $5
                      AND page_index = $6
                    "#,
                );
                sqlx::query(&sql)
                    .bind(document_id)
                    .bind(i64_from_u64(
                        visible_index_version,
                        "visible_index_version",
                    )?)
                    .bind(i64_from_u64(structure_version, "structure_version")?)
                    .bind(layout_key_hash)
                    .bind(i64_from_u64(page_policy_version, "page_policy_version")?)
                    .bind(i32_from_usize(page_index, "page_index")?)
                    .fetch_optional(&self.pool)
                    .await?
            }
            None => {
                let sql = page_layout_select_sql(
                    "WHERE document_id = $1 ORDER BY updated_at DESC LIMIT 1",
                );
                sqlx::query(&sql)
                    .bind(document_id)
                    .fetch_optional(&self.pool)
                    .await?
            }
        };

        row.map(page_layout_row_from_pg).transpose()
    }
}

fn block_layout_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            block_id,
            layout_key_hash,
            width_bucket,
            exact_width,
            content_version,
            attrs_version,
            style_version,
            font_version,
            theme_version,
            scale_factor,
            measured_height,
            estimated_height,
            confidence,
            max_error_hint,
            line_count,
            layout_cost,
            FLOOR(EXTRACT(EPOCH FROM measured_at) * 1000)::BIGINT AS measured_at_ms
        FROM block_layout
        {where_clause}
        "#
    )
}

fn batch_block_layout_select_sql() -> String {
    block_layout_select_sql(
        r#"
        WHERE block_id = ANY($1)
        ORDER BY block_id,
                 (layout_key_hash = $2) DESC,
                 measured_at DESC NULLS LAST,
                 updated_at DESC
        "#,
    )
    .replacen("SELECT", "SELECT DISTINCT ON (block_id)", 1)
}

fn page_layout_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            document_id,
            visible_index_version,
            structure_version,
            layout_key_hash,
            page_policy_version,
            page_index,
            block_start_index,
            block_count,
            first_block_id,
            last_block_id,
            height,
            measured_ratio,
            confidence,
            max_error_hint,
            dirty,
            FLOOR(EXTRACT(EPOCH FROM updated_at) * 1000)::BIGINT AS updated_at_ms
        FROM page_layout
        {where_clause}
        "#
    )
}

fn block_layout_row_from_pg(row: sqlx::postgres::PgRow) -> PostgresStorageResult<BlockLayoutRow> {
    let block_id: PgBlockId = row.try_get("block_id")?;
    let block_id =
        runtime_block_id_from_pg(block_id).ok_or_else(|| PostgresStorageError::CorruptData {
            message: format!("block id {block_id} is outside runtime namespace"),
        })?;
    let exact_width: Option<f64> = row.try_get("exact_width")?;
    let scale_factor: f64 = row.try_get("scale_factor")?;

    Ok(BlockLayoutRow {
        block_id,
        layout_key_hash: row.try_get("layout_key_hash")?,
        width_bucket: u16_from_i32(row.try_get("width_bucket")?, "width_bucket")?,
        exact_width_px: exact_width.unwrap_or(0.0).round().max(0.0) as u32,
        content_version: u64_from_i64(row.try_get("content_version")?, "content_version")?,
        attrs_version: u64_from_i64(row.try_get("attrs_version")?, "attrs_version")?,
        style_version: u64_from_i64(row.try_get("style_version")?, "style_version")?,
        font_version: u64_from_i64(row.try_get("font_version")?, "font_version")?,
        theme_version: u64_from_i64(row.try_get("theme_version")?, "theme_version")?,
        scale_factor_milli: (scale_factor * 1000.0).round().max(0.0) as u32,
        measured_height: row.try_get("measured_height")?,
        estimated_height: row.try_get("estimated_height")?,
        confidence: deserialize_confidence(u8_from_i32(row.try_get("confidence")?, "confidence")?),
        max_error_hint: row.try_get("max_error_hint")?,
        line_count: optional_u32_from_i32(row.try_get("line_count")?, "line_count")?,
        layout_cost: u32_from_i32(row.try_get("layout_cost")?, "layout_cost")?,
        measured_at: optional_u64_from_i64(row.try_get("measured_at_ms")?, "measured_at")?,
    })
}

fn page_layout_row_from_pg(row: sqlx::postgres::PgRow) -> PostgresStorageResult<PageLayoutRow> {
    let document_id: PgDocumentId = row.try_get("document_id")?;
    let document_id = crate::types::runtime_document_id_from_pg(document_id).ok_or_else(|| {
        PostgresStorageError::CorruptData {
            message: format!("document id {document_id} is outside runtime namespace"),
        }
    })?;
    let first_block_id: Option<PgBlockId> = row.try_get("first_block_id")?;
    let last_block_id: Option<PgBlockId> = row.try_get("last_block_id")?;

    Ok(PageLayoutRow {
        document_id,
        visible_index_version: u64_from_i64(
            row.try_get("visible_index_version")?,
            "visible_index_version",
        )?,
        structure_version: u64_from_i64(row.try_get("structure_version")?, "structure_version")?,
        layout_key_hash: row.try_get("layout_key_hash")?,
        page_policy_version: u64_from_i64(
            row.try_get("page_policy_version")?,
            "page_policy_version",
        )?,
        page_index: usize_from_i32(row.try_get("page_index")?, "page_index")?,
        block_start_index: usize_from_i32(row.try_get("block_start_index")?, "block_start_index")?,
        block_count: usize_from_i32(row.try_get("block_count")?, "block_count")?,
        first_block_id: first_block_id
            .map(runtime_block_id_or_corrupt)
            .transpose()?,
        last_block_id: last_block_id.map(runtime_block_id_or_corrupt).transpose()?,
        height: row.try_get("height")?,
        measured_ratio: row.try_get("measured_ratio")?,
        confidence: deserialize_confidence(u8_from_i32(row.try_get("confidence")?, "confidence")?),
        max_error_hint: row.try_get("max_error_hint")?,
        dirty: row.try_get("dirty")?,
        updated_at: optional_u64_from_i64(row.try_get("updated_at_ms")?, "updated_at")?
            .unwrap_or(0),
    })
}

fn runtime_block_id_or_corrupt(block_id: PgBlockId) -> PostgresStorageResult<BlockId> {
    runtime_block_id_from_pg(block_id).ok_or_else(|| PostgresStorageError::CorruptData {
        message: format!("block id {block_id} is outside runtime namespace"),
    })
}

fn missing_block_height() -> CachedHeight {
    CachedHeight {
        height: 0.0,
        confidence: HeightConfidence::Default,
        source: CacheSource::Missing,
        max_error_hint: f64::INFINITY,
    }
}

fn missing_page_height() -> CachedPageHeight {
    CachedPageHeight {
        height: 0.0,
        confidence: HeightConfidence::Default,
        source: CacheSource::Missing,
        dirty: true,
        max_error_hint: f64::INFINITY,
    }
}

fn i64_from_u64(value: u64, name: &'static str) -> PostgresStorageResult<i64> {
    i64::try_from(value).map_err(|_| PostgresStorageError::CorruptData {
        message: format!("{name} exceeds PostgreSQL BIGINT range"),
    })
}

fn u64_from_i64(value: i64, name: &'static str) -> PostgresStorageResult<u64> {
    u64::try_from(value).map_err(|_| PostgresStorageError::CorruptData {
        message: format!("{name} is negative: {value}"),
    })
}

fn optional_u64_from_i64(
    value: Option<i64>,
    name: &'static str,
) -> PostgresStorageResult<Option<u64>> {
    value.map(|value| u64_from_i64(value, name)).transpose()
}

fn i32_from_u32(value: u32, name: &'static str) -> PostgresStorageResult<i32> {
    i32::try_from(value).map_err(|_| PostgresStorageError::CorruptData {
        message: format!("{name} exceeds PostgreSQL INTEGER range"),
    })
}

fn optional_i32_from_u32(
    value: Option<u32>,
    name: &'static str,
) -> PostgresStorageResult<Option<i32>> {
    value.map(|value| i32_from_u32(value, name)).transpose()
}

fn i32_from_usize(value: usize, name: &'static str) -> PostgresStorageResult<i32> {
    i32::try_from(value).map_err(|_| PostgresStorageError::CorruptData {
        message: format!("{name} exceeds PostgreSQL INTEGER range"),
    })
}

fn u32_from_i32(value: i32, name: &'static str) -> PostgresStorageResult<u32> {
    u32::try_from(value).map_err(|_| PostgresStorageError::CorruptData {
        message: format!("{name} is negative: {value}"),
    })
}

fn optional_u32_from_i32(
    value: Option<i32>,
    name: &'static str,
) -> PostgresStorageResult<Option<u32>> {
    value.map(|value| u32_from_i32(value, name)).transpose()
}

fn u16_from_i32(value: i32, name: &'static str) -> PostgresStorageResult<u16> {
    u16::try_from(value).map_err(|_| PostgresStorageError::CorruptData {
        message: format!("{name} is outside u16 range: {value}"),
    })
}

fn u8_from_i32(value: i32, name: &'static str) -> PostgresStorageResult<u8> {
    u8::try_from(value).map_err(|_| PostgresStorageError::CorruptData {
        message: format!("{name} is outside u8 range: {value}"),
    })
}

fn usize_from_i32(value: i32, name: &'static str) -> PostgresStorageResult<usize> {
    usize::try_from(value).map_err(|_| PostgresStorageError::CorruptData {
        message: format!("{name} is negative: {value}"),
    })
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
