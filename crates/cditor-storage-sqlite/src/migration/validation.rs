use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use cditor_storage::StorageResult;

use super::{
    SqliteMigrationChecksums, SqliteMigrationValidation, hex, migration_error, sqlite_error,
};

pub(super) async fn validate_database(
    pool: &SqlitePool,
) -> StorageResult<SqliteMigrationValidation> {
    let integrity_rows = sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
        .fetch_all(pool)
        .await
        .map_err(sqlite_error)?;
    let integrity_ok = integrity_rows.len() == 1 && integrity_rows[0] == "ok";
    let foreign_key_violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(pool)
        .await
        .map_err(sqlite_error)?
        .len() as u64;
    Ok(SqliteMigrationValidation {
        integrity_ok,
        foreign_key_violations,
        checksums: SqliteMigrationChecksums {
            semantic_sha256: checksum_queries(pool, SEMANTIC_QUERIES).await?,
            unknown_raw_sha256: checksum_queries(pool, UNKNOWN_RAW_QUERIES).await?,
            asset_refs_sha256: checksum_queries(pool, ASSET_REF_QUERIES).await?,
        },
    })
}

pub(super) fn ensure_base_validation(
    validation: &SqliteMigrationValidation,
    label: &str,
) -> StorageResult<()> {
    if !validation.integrity_ok || validation.foreign_key_violations != 0 {
        return Err(migration_error(format!(
            "{label} failed validation: integrity_ok={}, foreign_key_violations={}",
            validation.integrity_ok, validation.foreign_key_violations
        )));
    }
    Ok(())
}

const SEMANTIC_QUERIES: &[(&str, &str)] = &[
    (
        "workspaces",
        "SELECT quote(id)||'|'||quote(name)||'|'||quote(created_at)||'|'||quote(updated_at) FROM workspaces ORDER BY id",
    ),
    (
        "documents",
        "SELECT quote(id)||'|'||quote(workspace_id)||'|'||quote(title)||'|'||quote(structure_version)||'|'||quote(content_version)||'|'||quote(layout_version)||'|'||quote(schema_version)||'|'||quote(created_at)||'|'||quote(updated_at)||'|'||quote(deleted_at) FROM documents ORDER BY id",
    ),
    (
        "blocks",
        "SELECT quote(document_id)||'|'||quote(id)||'|'||quote(parent_id)||'|'||quote(sort_key)||'|'||quote(depth)||'|'||quote(kind_tag)||'|'||quote(flags)||'|'||quote(content_version)||'|'||quote(structure_version)||'|'||quote(estimated_height)||'|'||quote(measured_height)||'|'||quote(width_bucket)||'|'||quote(layout_version)||'|'||quote(layout_dirty)||'|'||quote(updated_at)||'|'||quote(deleted_at) FROM blocks ORDER BY document_id, id",
    ),
    (
        "block_attrs",
        "SELECT quote(document_id)||'|'||quote(block_id)||'|'||quote(attrs_json)||'|'||quote(updated_at) FROM block_attrs ORDER BY document_id, block_id",
    ),
    (
        "block_payloads",
        "SELECT quote(document_id)||'|'||quote(block_id)||'|'||quote(kind_json)||'|'||quote(payload_json)||'|'||quote(plain_text)||'|'||quote(content_version)||'|'||quote(byte_len)||'|'||quote(updated_at) FROM block_payloads ORDER BY document_id, block_id",
    ),
    (
        "edit_transactions",
        "SELECT quote(document_id)||'|'||quote(transaction_id)||'|'||quote(transaction_json)||'|'||quote(structure_version)||'|'||quote(created_at) FROM edit_transactions ORDER BY document_id, transaction_id",
    ),
    (
        "runtime_snapshots",
        "SELECT quote(document_id)||'|'||quote(structure_version)||'|'||quote(content_version)||'|'||quote(snapshot_json)||'|'||quote(created_at) FROM runtime_snapshots ORDER BY document_id",
    ),
    (
        "operation_journal",
        "SELECT quote(document_id)||'|'||quote(transaction_id)||'|'||quote(schema_major)||'|'||quote(schema_minor)||'|'||quote(envelope_json)||'|'||quote(origin)||'|'||quote(created_at) FROM operation_journal ORDER BY document_id, id",
    ),
];

const UNKNOWN_RAW_QUERIES: &[(&str, &str)] = &[
    (
        "block_attrs",
        "SELECT quote(document_id)||'|'||quote(block_id)||'|'||quote(attrs_json) FROM block_attrs ORDER BY document_id, block_id",
    ),
    (
        "block_payloads",
        "SELECT quote(document_id)||'|'||quote(block_id)||'|'||quote(kind_json)||'|'||quote(payload_json) FROM block_payloads ORDER BY document_id, block_id",
    ),
    (
        "edit_transactions",
        "SELECT quote(document_id)||'|'||quote(transaction_id)||'|'||quote(transaction_json) FROM edit_transactions ORDER BY document_id, transaction_id",
    ),
    (
        "runtime_snapshots",
        "SELECT quote(document_id)||'|'||quote(snapshot_json) FROM runtime_snapshots ORDER BY document_id",
    ),
    (
        "operation_journal",
        "SELECT quote(document_id)||'|'||quote(transaction_id)||'|'||quote(envelope_json) FROM operation_journal ORDER BY document_id, id",
    ),
];

const ASSET_REF_QUERIES: &[(&str, &str)] = &[
    (
        "block_assets",
        "SELECT quote(document_id)||'|'||quote(block_id)||'|'||quote(asset_id) FROM block_assets ORDER BY document_id, block_id, asset_id",
    ),
    (
        "assets",
        "SELECT quote(id)||'|'||quote(checksum)||'|'||quote(ref_count) FROM assets ORDER BY id",
    ),
];

async fn checksum_queries(pool: &SqlitePool, queries: &[(&str, &str)]) -> StorageResult<String> {
    let mut hasher = Sha256::new();
    for (table, query) in queries {
        if !table_exists(pool, table).await? {
            continue;
        }
        let values = sqlx::query_scalar::<_, String>(query)
            .fetch_all(pool)
            .await
            .map_err(sqlite_error)?;
        // 新 migration 新增的空权威表与“该域尚无数据”语义相同。
        if values.is_empty() {
            continue;
        }
        hash_field(&mut hasher, table.as_bytes());
        for value in values {
            hash_field(&mut hasher, value.as_bytes());
        }
    }
    Ok(hex(hasher.finalize().as_slice()))
}

pub(super) async fn table_exists(pool: &SqlitePool, table: &str) -> StorageResult<bool> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
    )
    .bind(table)
    .fetch_one(pool)
    .await
    .map(|count| count != 0)
    .map_err(sqlite_error)
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}
