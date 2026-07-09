use sqlx::types::Uuid;
use sqlx::{PgPool, Row};

use cditor_core::ids::BlockId;
use cditor_core::layout::StableBox;
use cditor_storage::layout_cache::{deserialize_confidence, serialize_confidence};

use crate::error::{PostgresStorageError, PostgresStorageResult};
use crate::types::{PgBlockId, PgDocumentId, pg_block_id_from_runtime, runtime_block_id_from_pg};

#[derive(Debug, Clone)]
pub struct PostgresAssetStore {
    pool: PgPool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssetRecord {
    pub id: Option<Uuid>,
    pub workspace_id: Uuid,
    pub kind: String,
    pub object_key: String,
    pub public_url: Option<String>,
    pub media_type: Option<String>,
    pub checksum: Option<String>,
    pub size_bytes: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub duration_ms: Option<i32>,
    pub metadata_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredAssetRecord {
    pub id: Uuid,
    pub record: AssetRecord,
    pub ref_count: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockAssetRecord {
    pub block_id: BlockId,
    pub asset_id: Uuid,
    pub role: String,
    pub stable_width: Option<f64>,
    pub stable_box: Option<StableBox>,
    pub aspect_ratio: Option<f64>,
    pub caption_payload_json: Option<serde_json::Value>,
}

impl PostgresAssetStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn upsert_asset(&self, asset: &AssetRecord) -> PostgresStorageResult<Uuid> {
        if let Some(checksum) = &asset.checksum {
            if let Some(existing) = sqlx::query_scalar::<_, Uuid>(
                r#"
                SELECT id
                FROM assets
                WHERE workspace_id = $1 AND hash = $2 AND deleted_at IS NULL
                ORDER BY created_at
                LIMIT 1
                "#,
            )
            .bind(asset.workspace_id)
            .bind(checksum)
            .fetch_optional(&self.pool)
            .await?
            {
                sqlx::query(
                    r#"
                    UPDATE assets
                    SET kind = $2,
                        object_key = $3,
                        public_url = COALESCE($4, public_url),
                        media_type = COALESCE($5, media_type),
                        size_bytes = COALESCE($6, size_bytes),
                        width = COALESCE($7, width),
                        height = COALESCE($8, height),
                        duration_ms = COALESCE($9, duration_ms),
                        metadata_json = COALESCE($10, metadata_json),
                        updated_at = now()
                    WHERE id = $1
                    "#,
                )
                .bind(existing)
                .bind(&asset.kind)
                .bind(&asset.object_key)
                .bind(&asset.public_url)
                .bind(&asset.media_type)
                .bind(asset.size_bytes)
                .bind(asset.width)
                .bind(asset.height)
                .bind(asset.duration_ms)
                .bind(&asset.metadata_json)
                .execute(&self.pool)
                .await?;
                return Ok(existing);
            }
        }

        let id = asset.id.unwrap_or_else(Uuid::new_v4);
        sqlx::query(
            r#"
            INSERT INTO assets (
                id,
                workspace_id,
                kind,
                object_key,
                public_url,
                media_type,
                hash,
                size_bytes,
                width,
                height,
                duration_ms,
                metadata_json,
                updated_at,
                deleted_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, now(), NULL)
            ON CONFLICT (id) DO UPDATE SET
                workspace_id = EXCLUDED.workspace_id,
                kind = EXCLUDED.kind,
                object_key = EXCLUDED.object_key,
                public_url = EXCLUDED.public_url,
                media_type = EXCLUDED.media_type,
                hash = EXCLUDED.hash,
                size_bytes = EXCLUDED.size_bytes,
                width = EXCLUDED.width,
                height = EXCLUDED.height,
                duration_ms = EXCLUDED.duration_ms,
                metadata_json = EXCLUDED.metadata_json,
                updated_at = now(),
                deleted_at = NULL
            "#,
        )
        .bind(id)
        .bind(asset.workspace_id)
        .bind(&asset.kind)
        .bind(&asset.object_key)
        .bind(&asset.public_url)
        .bind(&asset.media_type)
        .bind(&asset.checksum)
        .bind(asset.size_bytes)
        .bind(asset.width)
        .bind(asset.height)
        .bind(asset.duration_ms)
        .bind(&asset.metadata_json)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    pub async fn load_asset(&self, asset_id: Uuid) -> PostgresStorageResult<StoredAssetRecord> {
        let row = sqlx::query(
            r#"
            SELECT
                a.id,
                a.workspace_id,
                a.kind,
                a.object_key,
                a.public_url,
                a.media_type,
                a.hash,
                a.size_bytes,
                a.width,
                a.height,
                a.duration_ms,
                a.metadata_json,
                COUNT(ba.block_id)::BIGINT AS ref_count
            FROM assets a
            LEFT JOIN block_assets ba ON ba.asset_id = a.id
            WHERE a.id = $1 AND a.deleted_at IS NULL
            GROUP BY a.id
            "#,
        )
        .bind(asset_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| PostgresStorageError::NotFound {
            entity: "asset",
            id: asset_id.to_string(),
        })?;
        stored_asset_from_row(row)
    }

    pub async fn bind_block_asset(
        &self,
        document_id: PgDocumentId,
        record: &BlockAssetRecord,
    ) -> PostgresStorageResult<()> {
        let block_id = pg_block_id_from_runtime(record.block_id);
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM blocks WHERE id = $1 AND document_id = $2 AND deleted_at IS NULL)",
        )
        .bind(block_id)
        .bind(document_id)
        .fetch_one(&self.pool)
        .await?;
        if !exists {
            return Err(PostgresStorageError::NotFound {
                entity: "block",
                id: block_id.to_string(),
            });
        }

        sqlx::query(
            r#"
            INSERT INTO block_assets (
                block_id,
                asset_id,
                role,
                stable_width,
                stable_estimated_height,
                stable_min_height,
                stable_max_height,
                stable_confidence,
                aspect_ratio,
                caption_payload_json
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (block_id) DO UPDATE SET
                asset_id = EXCLUDED.asset_id,
                role = EXCLUDED.role,
                stable_width = EXCLUDED.stable_width,
                stable_estimated_height = EXCLUDED.stable_estimated_height,
                stable_min_height = EXCLUDED.stable_min_height,
                stable_max_height = EXCLUDED.stable_max_height,
                stable_confidence = EXCLUDED.stable_confidence,
                aspect_ratio = EXCLUDED.aspect_ratio,
                caption_payload_json = EXCLUDED.caption_payload_json
            "#,
        )
        .bind(block_id)
        .bind(record.asset_id)
        .bind(&record.role)
        .bind(record.stable_width)
        .bind(record.stable_box.map(|stable| stable.estimated_height))
        .bind(record.stable_box.map(|stable| stable.min_height))
        .bind(record.stable_box.and_then(|stable| stable.max_height))
        .bind(
            record
                .stable_box
                .map(|stable| i32::from(serialize_confidence(stable.confidence))),
        )
        .bind(record.aspect_ratio)
        .bind(&record.caption_payload_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_block_asset(
        &self,
        block_id: BlockId,
    ) -> PostgresStorageResult<Option<BlockAssetRecord>> {
        let row = sqlx::query(
            r#"
            SELECT
                block_id,
                asset_id,
                role,
                stable_width,
                stable_estimated_height,
                stable_min_height,
                stable_max_height,
                stable_confidence,
                aspect_ratio,
                caption_payload_json
            FROM block_assets
            WHERE block_id = $1
            "#,
        )
        .bind(pg_block_id_from_runtime(block_id))
        .fetch_optional(&self.pool)
        .await?;
        row.map(block_asset_from_row).transpose()
    }

    pub async fn delete_block_asset(&self, block_id: BlockId) -> PostgresStorageResult<bool> {
        let result = sqlx::query("DELETE FROM block_assets WHERE block_id = $1")
            .bind(pg_block_id_from_runtime(block_id))
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn asset_ref_count(&self, asset_id: Uuid) -> PostgresStorageResult<i64> {
        sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM block_assets WHERE asset_id = $1")
            .bind(asset_id)
            .fetch_one(&self.pool)
            .await
            .map_err(PostgresStorageError::from)
    }

    pub async fn cleanup_candidates(
        &self,
        workspace_id: Uuid,
        limit: i64,
    ) -> PostgresStorageResult<Vec<StoredAssetRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT
                a.id,
                a.workspace_id,
                a.kind,
                a.object_key,
                a.public_url,
                a.media_type,
                a.hash,
                a.size_bytes,
                a.width,
                a.height,
                a.duration_ms,
                a.metadata_json,
                COUNT(ba.block_id)::BIGINT AS ref_count
            FROM assets a
            LEFT JOIN block_assets ba ON ba.asset_id = a.id
            WHERE a.workspace_id = $1 AND a.deleted_at IS NULL
            GROUP BY a.id
            HAVING COUNT(ba.block_id) = 0
            ORDER BY a.updated_at, a.id
            LIMIT $2
            "#,
        )
        .bind(workspace_id)
        .bind(limit.max(0))
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(stored_asset_from_row).collect()
    }

    pub async fn soft_delete_asset(&self, asset_id: Uuid) -> PostgresStorageResult<()> {
        let refs = self.asset_ref_count(asset_id).await?;
        if refs > 0 {
            return Err(PostgresStorageError::Conflict {
                message: format!("asset {asset_id} still has {refs} block references"),
            });
        }
        let result = sqlx::query("UPDATE assets SET deleted_at = now(), updated_at = now() WHERE id = $1 AND deleted_at IS NULL")
            .bind(asset_id)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(PostgresStorageError::NotFound {
                entity: "asset",
                id: asset_id.to_string(),
            });
        }
        Ok(())
    }
}

fn stored_asset_from_row(row: sqlx::postgres::PgRow) -> PostgresStorageResult<StoredAssetRecord> {
    let id: Uuid = row.try_get("id")?;
    Ok(StoredAssetRecord {
        id,
        record: AssetRecord {
            id: Some(id),
            workspace_id: row.try_get("workspace_id")?,
            kind: row.try_get("kind")?,
            object_key: row.try_get("object_key")?,
            public_url: row.try_get("public_url")?,
            media_type: row.try_get("media_type")?,
            checksum: row.try_get("hash")?,
            size_bytes: row.try_get("size_bytes")?,
            width: row.try_get("width")?,
            height: row.try_get("height")?,
            duration_ms: row.try_get("duration_ms")?,
            metadata_json: row.try_get("metadata_json")?,
        },
        ref_count: row.try_get("ref_count")?,
    })
}

fn block_asset_from_row(row: sqlx::postgres::PgRow) -> PostgresStorageResult<BlockAssetRecord> {
    let pg_block_id: PgBlockId = row.try_get("block_id")?;
    let block_id =
        runtime_block_id_from_pg(pg_block_id).ok_or_else(|| PostgresStorageError::CorruptData {
            message: format!("block id {pg_block_id} is outside runtime namespace"),
        })?;
    let confidence: Option<i32> = row.try_get("stable_confidence")?;
    let estimated_height: Option<f64> = row.try_get("stable_estimated_height")?;
    let min_height: Option<f64> = row.try_get("stable_min_height")?;
    let max_height: Option<f64> = row.try_get("stable_max_height")?;
    let stable_box = match (estimated_height, min_height, confidence) {
        (Some(estimated_height), Some(min_height), Some(confidence)) => Some(StableBox {
            estimated_height,
            min_height,
            max_height,
            confidence: deserialize_confidence(u8::try_from(confidence).unwrap_or(0)),
        }),
        _ => None,
    };

    Ok(BlockAssetRecord {
        block_id,
        asset_id: row.try_get("asset_id")?,
        role: row.try_get("role")?,
        stable_width: row.try_get("stable_width")?,
        stable_box,
        aspect_ratio: row.try_get("aspect_ratio")?,
        caption_payload_json: row.try_get("caption_payload_json")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DocumentRow, PostgresDocumentStore, PostgresPoolConfig, create_pg_pool,
        pg_document_id_from_runtime, run_migrations,
    };
    use cditor_core::document::BlockIndexRecord;
    use cditor_core::layout::HeightConfidence;
    use cditor_core::rich_text::{RichBlockKind, kind_tag_for_rich_block_kind};

    fn test_database_url() -> String {
        std::env::var("CDITOR_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned())
    }

    async fn test_stores() -> (
        PostgresDocumentStore,
        PostgresAssetStore,
        DocumentRow,
        BlockId,
    ) {
        let pool = create_pg_pool(&PostgresPoolConfig::for_tests(test_database_url()))
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();
        let document_store = PostgresDocumentStore::new(pool.clone());
        let asset_store = PostgresAssetStore::new(pool);
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as u64;
        let runtime_document_id = 100_000 + suffix;
        let document = DocumentRow {
            id: pg_document_id_from_runtime(runtime_document_id),
            workspace_id: Uuid::from_u128(
                0x9700_0000_0000_0000_0000_0000_0000_0000 | runtime_document_id as u128,
            ),
            title: format!("Asset Store {runtime_document_id}"),
            structure_version: 1,
            content_version: 1,
            layout_version: 0,
            schema_version: 1,
        };
        document_store
            .save_document_metadata(&document)
            .await
            .unwrap();
        let block_id = runtime_document_id * 10;
        document_store
            .save_block_index_records(
                document.id,
                &[BlockIndexRecord::new(
                    block_id,
                    None,
                    0,
                    kind_tag_for_rich_block_kind(&RichBlockKind::Image),
                    0,
                )],
                1,
            )
            .await
            .unwrap();
        (document_store, asset_store, document, block_id)
    }

    fn asset(workspace_id: Uuid, checksum: &str) -> AssetRecord {
        AssetRecord {
            id: None,
            workspace_id,
            kind: "image".to_owned(),
            object_key: format!("objects/{checksum}.png"),
            public_url: None,
            media_type: Some("image/png".to_owned()),
            checksum: Some(checksum.to_owned()),
            size_bytes: Some(1024),
            width: Some(640),
            height: Some(480),
            duration_ms: None,
            metadata_json: Some(serde_json::json!({ "source": "test" })),
        }
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_asset_store_deduplicates_by_checksum_and_loads_registry_record() {
        let (_document_store, asset_store, document, _block_id) = test_stores().await;
        let first = asset_store
            .upsert_asset(&asset(document.workspace_id, "sha256-a"))
            .await
            .unwrap();
        let second = asset_store
            .upsert_asset(&asset(document.workspace_id, "sha256-a"))
            .await
            .unwrap();

        assert_eq!(first, second);
        let loaded = asset_store.load_asset(first).await.unwrap();
        assert_eq!(loaded.record.checksum.as_deref(), Some("sha256-a"));
        assert_eq!(loaded.ref_count, 0);
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_asset_store_binds_block_asset_with_stable_box_and_ref_count() {
        let (_document_store, asset_store, document, block_id) = test_stores().await;
        let asset_id = asset_store
            .upsert_asset(&asset(document.workspace_id, "sha256-b"))
            .await
            .unwrap();
        let stable_box = StableBox {
            estimated_height: 360.0,
            min_height: 240.0,
            max_height: Some(480.0),
            confidence: HeightConfidence::Predictive,
        };

        asset_store
            .bind_block_asset(
                document.id,
                &BlockAssetRecord {
                    block_id,
                    asset_id,
                    role: "main".to_owned(),
                    stable_width: Some(640.0),
                    stable_box: Some(stable_box),
                    aspect_ratio: Some(4.0 / 3.0),
                    caption_payload_json: Some(serde_json::json!({ "text": "caption" })),
                },
            )
            .await
            .unwrap();

        let loaded = asset_store
            .load_block_asset(block_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.asset_id, asset_id);
        assert_eq!(loaded.stable_box, Some(stable_box));
        assert_eq!(asset_store.asset_ref_count(asset_id).await.unwrap(), 1);
        assert_eq!(asset_store.load_asset(asset_id).await.unwrap().ref_count, 1);
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_asset_store_cleanup_candidates_exclude_referenced_assets() {
        let (_document_store, asset_store, document, block_id) = test_stores().await;
        let referenced = asset_store
            .upsert_asset(&asset(document.workspace_id, "sha256-c"))
            .await
            .unwrap();
        let unreferenced = asset_store
            .upsert_asset(&asset(document.workspace_id, "sha256-d"))
            .await
            .unwrap();
        asset_store
            .bind_block_asset(
                document.id,
                &BlockAssetRecord {
                    block_id,
                    asset_id: referenced,
                    role: "main".to_owned(),
                    stable_width: None,
                    stable_box: None,
                    aspect_ratio: None,
                    caption_payload_json: None,
                },
            )
            .await
            .unwrap();

        let candidates = asset_store
            .cleanup_candidates(document.workspace_id, 10)
            .await
            .unwrap();
        let candidate_ids = candidates.iter().map(|asset| asset.id).collect::<Vec<_>>();
        assert!(candidate_ids.contains(&unreferenced));
        assert!(!candidate_ids.contains(&referenced));

        assert!(asset_store.delete_block_asset(block_id).await.unwrap());
        asset_store.soft_delete_asset(referenced).await.unwrap();
        assert!(matches!(
            asset_store.load_asset(referenced).await.unwrap_err(),
            PostgresStorageError::NotFound { .. }
        ));
    }
}
