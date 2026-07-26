use cditor_core::edit::{
    AssetEditOperation, AssetSnapshot, AssetState, EditOperation, EditTransaction,
};
use cditor_core::ids::{AssetId, DocumentId};
use cditor_storage::{
    AssetManifestRecord, AssetReference, AssetUploadMutation, ProvisionalAssetRequest,
    StorageError, StorageResult,
};
use sqlx::{Row, Sqlite, Transaction};
use uuid::Uuid;

use crate::error::sqlite_error;
use crate::ids::{
    asset_id_from_sqlite, asset_id_to_sqlite, block_id_from_sqlite, block_id_to_sqlite,
    document_id_from_sqlite, document_id_to_sqlite,
};
use crate::storage::SqliteDocumentStorage;
use crate::util::{checked_i64, checked_u32, checked_u64, unix_millis};

impl SqliteDocumentStorage {
    pub async fn create_provisional_asset(
        &self,
        request: ProvisionalAssetRequest,
    ) -> StorageResult<AssetManifestRecord> {
        validate_provisional_request(&request)?;
        let _writer = self.writer_gate().acquire().await?;
        let workspace_id = Uuid::from_u128(request.workspace_id as u128);
        let content_hash = request.asset.checksum.as_deref().expect("validated hash");
        if let Some(existing_id) = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM assets WHERE workspace_id = ? AND content_hash = ? \
             AND deleted_at IS NULL ORDER BY created_at LIMIT 1",
        )
        .bind(workspace_id)
        .bind(content_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlite_error)?
        {
            return self.load_asset_manifest_by_sqlite_id(existing_id).await;
        }
        let now = unix_millis()?;
        sqlx::query(
            "INSERT INTO assets \
             (id, workspace_id, file_name, media_type, size_bytes, local_source, content_hash, \
              state, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, 'local_pending', ?, ?)",
        )
        .bind(asset_id_to_sqlite(request.asset.asset_id))
        .bind(workspace_id)
        .bind(&request.asset.file_name)
        .bind(&request.asset.media_type)
        .bind(checked_i64(request.asset.size_bytes)?)
        .bind(&request.asset.source)
        .bind(content_hash)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(sqlite_error)?;
        self.load_asset_manifest_by_sqlite_id(asset_id_to_sqlite(request.asset.asset_id))
            .await
    }

    pub async fn asset_manifest(
        &self,
        asset_id: AssetId,
    ) -> StorageResult<Option<AssetManifestRecord>> {
        let row = asset_manifest_row(&self.pool, asset_id_to_sqlite(asset_id)).await?;
        row.map(asset_manifest_from_row).transpose()
    }

    pub async fn update_asset_upload(
        &self,
        asset_id: AssetId,
        mutation: AssetUploadMutation,
    ) -> StorageResult<AssetManifestRecord> {
        let _writer = self.writer_gate().acquire().await?;
        let asset_uuid = asset_id_to_sqlite(asset_id);
        let mut transaction = self.pool.begin().await.map_err(sqlite_error)?;
        let row = sqlx::query(
            "SELECT state, upload_session_id, uploaded_bytes, size_bytes \
             FROM assets WHERE id = ?",
        )
        .bind(asset_uuid)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(sqlite_error)?
        .ok_or_else(|| StorageError::NotFound {
            entity: "asset",
            id: asset_id.to_string(),
        })?;
        let state = parse_asset_state(row.try_get::<String, _>(0).map_err(sqlite_error)?.as_str())?;
        let current_session: Option<String> = row.try_get(1).map_err(sqlite_error)?;
        let uploaded_bytes = checked_u64(row.try_get(2).map_err(sqlite_error)?, "uploaded_bytes")?;
        let size_bytes = checked_u64(row.try_get(3).map_err(sqlite_error)?, "size_bytes")?;
        let now = unix_millis()?;

        match mutation {
            AssetUploadMutation::Begin { upload_session_id } => {
                require_non_empty(&upload_session_id, "upload session id")?;
                if !matches!(state, AssetState::LocalPending | AssetState::Failed) {
                    return invalid_transition(state, "begin upload");
                }
                sqlx::query(
                    "UPDATE assets SET state = 'uploading', upload_session_id = ?, \
                     uploaded_bytes = 0, attempt_count = attempt_count + 1, last_error = NULL, \
                     updated_at = ? WHERE id = ?",
                )
                .bind(upload_session_id)
                .bind(now)
                .bind(asset_uuid)
                .execute(&mut *transaction)
                .await
                .map_err(sqlite_error)?;
            }
            AssetUploadMutation::Progress {
                upload_session_id,
                uploaded_bytes: next,
            } => {
                require_upload_session(state, current_session.as_deref(), &upload_session_id)?;
                if next < uploaded_bytes || next > size_bytes {
                    return Err(StorageError::CorruptData(format!(
                        "upload progress {next} must be monotonic and no larger than {size_bytes}"
                    )));
                }
                sqlx::query("UPDATE assets SET uploaded_bytes = ?, updated_at = ? WHERE id = ?")
                    .bind(checked_i64(next)?)
                    .bind(now)
                    .bind(asset_uuid)
                    .execute(&mut *transaction)
                    .await
                    .map_err(sqlite_error)?;
            }
            AssetUploadMutation::Complete {
                upload_session_id,
                canonical_asset_id,
                remote_object_key,
                public_url,
            } => {
                require_upload_session(state, current_session.as_deref(), &upload_session_id)?;
                require_non_empty(&remote_object_key, "remote object key")?;
                if uploaded_bytes != size_bytes {
                    return Err(StorageError::CorruptData(format!(
                        "cannot complete upload at {uploaded_bytes}/{size_bytes} bytes"
                    )));
                }
                sqlx::query(
                    "UPDATE assets SET state = 'ready', canonical_asset_id = ?, \
                     remote_object_key = ?, public_url = ?, last_error = NULL, updated_at = ? \
                     WHERE id = ?",
                )
                .bind(asset_id_to_sqlite(canonical_asset_id))
                .bind(remote_object_key)
                .bind(public_url)
                .bind(now)
                .bind(asset_uuid)
                .execute(&mut *transaction)
                .await
                .map_err(sqlite_error)?;
            }
            AssetUploadMutation::Fail {
                upload_session_id,
                error,
            } => {
                require_non_empty(&error, "upload error")?;
                if state == AssetState::Uploading {
                    let session = upload_session_id.as_deref().ok_or_else(|| {
                        StorageError::CorruptData(
                            "an uploading asset requires its upload session id on failure".into(),
                        )
                    })?;
                    require_upload_session(state, current_session.as_deref(), session)?;
                } else if !matches!(state, AssetState::LocalPending | AssetState::Failed) {
                    return invalid_transition(state, "record upload failure");
                }
                sqlx::query(
                    "UPDATE assets SET state = 'failed', last_error = ?, updated_at = ? WHERE id = ?",
                )
                .bind(error)
                .bind(now)
                .bind(asset_uuid)
                .execute(&mut *transaction)
                .await
                .map_err(sqlite_error)?;
            }
            AssetUploadMutation::Delete => {
                if state == AssetState::Deleted {
                    return invalid_transition(state, "delete asset");
                }
                sqlx::query(
                    "UPDATE assets SET state = 'deleted', deleted_at = ?, updated_at = ? WHERE id = ?",
                )
                .bind(now)
                .bind(now)
                .bind(asset_uuid)
                .execute(&mut *transaction)
                .await
                .map_err(sqlite_error)?;
            }
        }
        transaction.commit().await.map_err(sqlite_error)?;
        self.load_asset_manifest_by_sqlite_id(asset_uuid).await
    }

    pub async fn pending_asset_uploads(
        &self,
        workspace_id: u64,
        limit: usize,
    ) -> StorageResult<Vec<AssetManifestRecord>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            "SELECT id, workspace_id, file_name, media_type, size_bytes, local_source, \
                    content_hash, state, canonical_asset_id, upload_session_id, uploaded_bytes, \
                    remote_object_key, public_url, attempt_count, last_error, updated_at \
             FROM assets WHERE workspace_id = ? AND state IN ('local_pending', 'failed') \
             AND deleted_at IS NULL ORDER BY updated_at, id LIMIT ?",
        )
        .bind(Uuid::from_u128(workspace_id as u128))
        .bind(i64::try_from(limit.min(1_000)).map_err(|_| {
            StorageError::CorruptData("pending asset limit exceeds SQLite range".into())
        })?)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlite_error)?;
        rows.into_iter().map(asset_manifest_from_row).collect()
    }

    pub async fn asset_references(&self, asset_id: AssetId) -> StorageResult<Vec<AssetReference>> {
        let rows = sqlx::query(
            "SELECT document_id, block_id, role FROM block_assets \
             WHERE asset_id = ? ORDER BY document_id, block_id, role",
        )
        .bind(asset_id_to_sqlite(asset_id))
        .fetch_all(&self.pool)
        .await
        .map_err(sqlite_error)?;
        rows.into_iter()
            .map(|row| {
                let document_uuid: Uuid = row.try_get(0).map_err(sqlite_error)?;
                let block_uuid: Uuid = row.try_get(1).map_err(sqlite_error)?;
                Ok(AssetReference {
                    document_id: document_id_from_sqlite(document_uuid).ok_or_else(|| {
                        StorageError::CorruptData("asset reference document id is invalid".into())
                    })?,
                    block_id: block_id_from_sqlite(block_uuid).ok_or_else(|| {
                        StorageError::CorruptData("asset reference block id is invalid".into())
                    })?,
                    asset_id,
                    role: row.try_get(2).map_err(sqlite_error)?,
                })
            })
            .collect()
    }

    async fn load_asset_manifest_by_sqlite_id(
        &self,
        asset_id: Uuid,
    ) -> StorageResult<AssetManifestRecord> {
        asset_manifest_row(&self.pool, asset_id)
            .await?
            .map(asset_manifest_from_row)
            .transpose()?
            .ok_or_else(|| StorageError::NotFound {
                entity: "asset",
                id: asset_id.to_string(),
            })
    }
}

pub(crate) async fn materialize_asset_operations(
    transaction: &mut Transaction<'_, Sqlite>,
    document_id: DocumentId,
    edits: &[EditTransaction],
    now: i64,
) -> StorageResult<()> {
    let document_uuid = document_id_to_sqlite(document_id);
    let workspace_uuid: Uuid =
        sqlx::query_scalar("SELECT workspace_id FROM documents WHERE id = ?")
            .bind(document_uuid)
            .fetch_one(&mut **transaction)
            .await
            .map_err(sqlite_error)?;
    for operation in edits.iter().flat_map(|edit| edit.ops.iter()) {
        let EditOperation::Asset(operation) = operation else {
            continue;
        };
        match operation {
            AssetEditOperation::Attach { block_id, asset } => {
                upsert_transaction_asset(transaction, workspace_uuid, asset, now).await?;
                sqlx::query(
                    "INSERT OR IGNORE INTO block_assets \
                     (document_id, block_id, asset_id, role, created_at) VALUES (?, ?, ?, 'main', ?)",
                )
                .bind(document_uuid)
                .bind(block_id_to_sqlite(*block_id))
                .bind(asset_id_to_sqlite(asset.asset_id))
                .bind(now)
                .execute(&mut **transaction)
                .await
                .map_err(sqlite_error)?;
            }
            AssetEditOperation::Detach { block_id, asset } => {
                sqlx::query(
                    "DELETE FROM block_assets WHERE document_id = ? AND block_id = ? \
                     AND asset_id = ?",
                )
                .bind(document_uuid)
                .bind(block_id_to_sqlite(*block_id))
                .bind(asset_id_to_sqlite(asset.asset_id))
                .execute(&mut **transaction)
                .await
                .map_err(sqlite_error)?;
            }
            AssetEditOperation::Update { after, .. } => {
                upsert_transaction_asset(transaction, workspace_uuid, after, now).await?;
            }
        }
    }
    Ok(())
}

async fn upsert_transaction_asset(
    transaction: &mut Transaction<'_, Sqlite>,
    workspace_id: Uuid,
    asset: &AssetSnapshot,
    now: i64,
) -> StorageResult<()> {
    sqlx::query(
        "INSERT INTO assets \
         (id, workspace_id, file_name, media_type, size_bytes, local_source, content_hash, state, \
          created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET file_name = excluded.file_name, \
         media_type = excluded.media_type, size_bytes = excluded.size_bytes, \
         local_source = excluded.local_source, content_hash = excluded.content_hash, \
         updated_at = excluded.updated_at",
    )
    .bind(asset_id_to_sqlite(asset.asset_id))
    .bind(workspace_id)
    .bind(&asset.file_name)
    .bind(&asset.media_type)
    .bind(checked_i64(asset.size_bytes)?)
    .bind(&asset.source)
    .bind(&asset.checksum)
    .bind(asset_state_as_str(asset.state))
    .bind(now)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(sqlite_error)?;
    Ok(())
}

async fn asset_manifest_row(
    pool: &sqlx::SqlitePool,
    asset_id: Uuid,
) -> StorageResult<Option<sqlx::sqlite::SqliteRow>> {
    sqlx::query(
        "SELECT id, workspace_id, file_name, media_type, size_bytes, local_source, \
                content_hash, state, canonical_asset_id, upload_session_id, uploaded_bytes, \
                remote_object_key, public_url, attempt_count, last_error, updated_at \
         FROM assets WHERE id = ?",
    )
    .bind(asset_id)
    .fetch_optional(pool)
    .await
    .map_err(sqlite_error)
}

fn asset_manifest_from_row(row: sqlx::sqlite::SqliteRow) -> StorageResult<AssetManifestRecord> {
    let asset_uuid: Uuid = row.try_get(0).map_err(sqlite_error)?;
    let workspace_uuid: Uuid = row.try_get(1).map_err(sqlite_error)?;
    let canonical_uuid: Option<Uuid> = row.try_get(8).map_err(sqlite_error)?;
    Ok(AssetManifestRecord {
        workspace_id: u64::try_from(workspace_uuid.as_u128()).map_err(|_| {
            StorageError::CorruptData("asset workspace id is outside runtime namespace".into())
        })?,
        asset: AssetSnapshot {
            asset_id: asset_id_from_sqlite(asset_uuid).ok_or_else(|| {
                StorageError::CorruptData("asset id is outside runtime namespace".into())
            })?,
            file_name: row.try_get(2).map_err(sqlite_error)?,
            media_type: row.try_get(3).map_err(sqlite_error)?,
            size_bytes: checked_u64(row.try_get(4).map_err(sqlite_error)?, "size_bytes")?,
            source: row.try_get(5).map_err(sqlite_error)?,
            checksum: row.try_get(6).map_err(sqlite_error)?,
            state: parse_asset_state(row.try_get::<String, _>(7).map_err(sqlite_error)?.as_str())?,
        },
        canonical_asset_id: canonical_uuid
            .map(|id| {
                asset_id_from_sqlite(id).ok_or_else(|| {
                    StorageError::CorruptData("canonical asset id is invalid".into())
                })
            })
            .transpose()?,
        upload_session_id: row.try_get(9).map_err(sqlite_error)?,
        uploaded_bytes: checked_u64(row.try_get(10).map_err(sqlite_error)?, "uploaded_bytes")?,
        remote_object_key: row.try_get(11).map_err(sqlite_error)?,
        public_url: row.try_get(12).map_err(sqlite_error)?,
        attempt_count: checked_u32(row.try_get(13).map_err(sqlite_error)?, "attempt_count")?,
        last_error: row.try_get(14).map_err(sqlite_error)?,
        updated_at_ms: row.try_get(15).map_err(sqlite_error)?,
    })
}

fn validate_provisional_request(request: &ProvisionalAssetRequest) -> StorageResult<()> {
    let asset = &request.asset;
    if asset.state != AssetState::LocalPending {
        return Err(StorageError::CorruptData(
            "a provisional asset must start in LocalPending".into(),
        ));
    }
    require_non_empty(&asset.file_name, "asset file name")?;
    require_non_empty(&asset.media_type, "asset media type")?;
    require_non_empty(&asset.source, "asset local source")?;
    let hash = asset.checksum.as_deref().ok_or_else(|| {
        StorageError::CorruptData("a provisional asset requires a SHA-256 content hash".into())
    })?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(StorageError::CorruptData(
            "asset content hash must be 64 hexadecimal SHA-256 characters".into(),
        ));
    }
    Ok(())
}

fn require_upload_session(
    state: AssetState,
    current: Option<&str>,
    supplied: &str,
) -> StorageResult<()> {
    if state != AssetState::Uploading || current != Some(supplied) {
        return Err(StorageError::CorruptData(
            "stale or mismatched asset upload session".into(),
        ));
    }
    Ok(())
}

fn invalid_transition<T>(state: AssetState, action: &str) -> StorageResult<T> {
    Err(StorageError::CorruptData(format!(
        "cannot {action} while asset is {state:?}"
    )))
}

fn require_non_empty(value: &str, field: &str) -> StorageResult<()> {
    if value.trim().is_empty() {
        return Err(StorageError::CorruptData(format!(
            "{field} cannot be empty"
        )));
    }
    Ok(())
}

fn parse_asset_state(value: &str) -> StorageResult<AssetState> {
    match value {
        "local_pending" => Ok(AssetState::LocalPending),
        "uploading" => Ok(AssetState::Uploading),
        "ready" => Ok(AssetState::Ready),
        "failed" => Ok(AssetState::Failed),
        "deleted" => Ok(AssetState::Deleted),
        other => Err(StorageError::CorruptData(format!(
            "unknown asset state {other:?}"
        ))),
    }
}

fn asset_state_as_str(state: AssetState) -> &'static str {
    match state {
        AssetState::LocalPending => "local_pending",
        AssetState::Uploading => "uploading",
        AssetState::Ready => "ready",
        AssetState::Failed => "failed",
        AssetState::Deleted => "deleted",
    }
}
