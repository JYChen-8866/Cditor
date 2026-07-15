use sqlx::Row;

use cditor_core::ids::BlockId;
use cditor_core::rich_text::BlockAttrs;

use crate::error::{PostgresStorageError, PostgresStorageResult};
use crate::types::{
    PgBlockId, PgDocumentId, decode_block_attrs, encode_block_attrs, pg_block_id_from_runtime,
    runtime_block_id_from_pg,
};

use super::PostgresDocumentStore;

impl PostgresDocumentStore {
    pub async fn load_block_attrs(
        &self,
        document_id: PgDocumentId,
    ) -> PostgresStorageResult<Vec<(BlockId, BlockAttrs)>> {
        sqlx::query(
            r#"
            SELECT block_attrs.block_id, block_attrs.attrs_json
            FROM block_attrs
            INNER JOIN blocks ON blocks.id = block_attrs.block_id
            WHERE blocks.document_id = $1 AND blocks.deleted_at IS NULL
            "#,
        )
        .bind(document_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| {
            let pg_block_id: PgBlockId = row.try_get("block_id")?;
            let block_id = runtime_block_id_from_pg(pg_block_id).ok_or_else(|| {
                PostgresStorageError::CorruptData {
                    message: format!("block attrs id {pg_block_id} is outside runtime namespace"),
                }
            })?;
            let attrs = decode_block_attrs(row.try_get("attrs_json")?).map_err(|error| {
                PostgresStorageError::CorruptData {
                    message: format!("invalid attrs for block {block_id}: {error}"),
                }
            })?;
            Ok((block_id, attrs))
        })
        .collect()
    }

    pub async fn save_block_attrs(
        &self,
        document_id: PgDocumentId,
        attrs: &[(BlockId, BlockAttrs)],
    ) -> PostgresStorageResult<()> {
        let mut tx = self.pool.begin().await?;
        self.save_block_attrs_tx(&mut tx, document_id, attrs)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn save_block_attrs_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        document_id: PgDocumentId,
        attrs: &[(BlockId, BlockAttrs)],
    ) -> PostgresStorageResult<()> {
        sqlx::query(
            r#"
            DELETE FROM block_attrs
            USING blocks
            WHERE block_attrs.block_id = blocks.id AND blocks.document_id = $1
            "#,
        )
        .bind(document_id)
        .execute(&mut **tx)
        .await?;
        for (block_id, value) in attrs {
            let attrs_json =
                encode_block_attrs(value).map_err(|error| PostgresStorageError::CorruptData {
                    message: format!("cannot encode attrs for block {block_id}: {error}"),
                })?;
            sqlx::query(
                r#"
                INSERT INTO block_attrs (block_id, attrs_json, attrs_version, updated_at)
                VALUES ($1, $2, 1, now())
                "#,
            )
            .bind(pg_block_id_from_runtime(*block_id))
            .bind(attrs_json)
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }
}
