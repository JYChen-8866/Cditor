use sqlx::types::Uuid;

use cditor_core::demo_fixtures::{
    large_mixed_demo_index_records, large_mixed_demo_payload_records,
};

use super::{
    DocumentRow, PgDocumentId, PostgresDocumentStore, PostgresPayloadStore, PostgresStorageResult,
};

const DEFAULT_PAYLOAD_SEED_BATCH_SIZE: usize = 1_000;
const DEMO_WORKSPACE_NAMESPACE: u128 = 0x9300_0000_0000_0000_0000_0000_0000_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LargeDemoSeedOptions {
    pub document_id: PgDocumentId,
    pub workspace_id: u64,
    pub block_count: usize,
    pub force_reseed: bool,
    pub payload_batch_size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LargeDemoSeedReport {
    pub document_id: PgDocumentId,
    pub block_count: usize,
    pub payload_count: usize,
    pub skipped_existing: bool,
}

impl LargeDemoSeedOptions {
    pub fn new(document_id: PgDocumentId, workspace_id: u64, block_count: usize) -> Self {
        Self {
            document_id,
            workspace_id,
            block_count: block_count.max(1),
            force_reseed: false,
            payload_batch_size: DEFAULT_PAYLOAD_SEED_BATCH_SIZE,
        }
    }

    pub fn force_reseed(mut self, force: bool) -> Self {
        self.force_reseed = force;
        self
    }
}

pub async fn ensure_large_mixed_demo_seeded(
    document_store: &PostgresDocumentStore,
    payload_store: &PostgresPayloadStore,
    options: LargeDemoSeedOptions,
) -> PostgresStorageResult<LargeDemoSeedReport> {
    if !options.force_reseed
        && document_store
            .load_document_metadata(options.document_id)
            .await
            .is_ok()
    {
        return Ok(LargeDemoSeedReport {
            document_id: options.document_id,
            block_count: 0,
            payload_count: 0,
            skipped_existing: true,
        });
    }

    let block_count = options.block_count.max(1);
    let structure_version = 1_i64;
    let workspace_id = workspace_uuid(options.workspace_id);
    let document = DocumentRow {
        id: options.document_id,
        workspace_id,
        title: format!("CDitor V2 PostgreSQL 10w mixed demo ({block_count} blocks)"),
        structure_version,
        content_version: 1,
        layout_version: 0,
        schema_version: 1,
    };

    document_store.save_document_metadata(&document).await?;

    let records = large_mixed_demo_index_records(block_count);
    document_store
        .save_block_index_records(options.document_id, &records, structure_version)
        .await?;
    document_store
        .save_document_index_snapshot(options.document_id, 1, structure_version, &records)
        .await?;

    let batch_size = options.payload_batch_size.max(1);
    let mut payload_count = 0usize;
    let mut start = 0usize;
    while start < block_count {
        let end = (start + batch_size).min(block_count);
        let payloads = large_mixed_demo_payload_records(start..end, block_count);
        payload_store
            .save_block_payloads(options.document_id, &payloads)
            .await?;
        payload_count += payloads.len();
        start = end;
    }

    Ok(LargeDemoSeedReport {
        document_id: options.document_id,
        block_count,
        payload_count,
        skipped_existing: false,
    })
}

fn workspace_uuid(workspace_id: u64) -> Uuid {
    Uuid::from_u128(DEMO_WORKSPACE_NAMESPACE | workspace_id as u128)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PostgresPoolConfig, create_pg_pool, pg_document_id_from_runtime, run_migrations};

    #[test]
    fn large_demo_seed_options_clamp_sizes() {
        let options = LargeDemoSeedOptions::new(pg_document_id_from_runtime(1), 7, 0);

        assert_eq!(options.block_count, 1);
        assert_eq!(options.payload_batch_size, DEFAULT_PAYLOAD_SEED_BATCH_SIZE);
        assert_eq!(
            workspace_uuid(7),
            Uuid::from_u128(DEMO_WORKSPACE_NAMESPACE | 7)
        );
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn postgres_large_demo_seed_writes_blocks_payloads_and_skips_existing() {
        let database_url = std::env::var("CDITOR_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned());
        let pool = create_pg_pool(&PostgresPoolConfig::for_tests(database_url))
            .await
            .unwrap();
        run_migrations(&pool).await.unwrap();
        let document_store = PostgresDocumentStore::new(pool.clone());
        let payload_store = PostgresPayloadStore::new(pool);
        let document_id = pg_document_id_from_runtime(880_001);

        let report = ensure_large_mixed_demo_seeded(
            &document_store,
            &payload_store,
            LargeDemoSeedOptions::new(document_id, 1, 36).force_reseed(true),
        )
        .await
        .unwrap();
        assert_eq!(report.block_count, 36);
        assert_eq!(report.payload_count, 36);
        assert!(!report.skipped_existing);

        let blocks = document_store
            .load_block_index_records(document_id)
            .await
            .unwrap();
        assert_eq!(blocks.len(), 36);
        let payloads = payload_store
            .load_block_payloads(&[1, 13, 36])
            .await
            .unwrap();
        assert_eq!(payloads.records.len(), 3);

        let skipped = ensure_large_mixed_demo_seeded(
            &document_store,
            &payload_store,
            LargeDemoSeedOptions::new(document_id, 1, 36),
        )
        .await
        .unwrap();
        assert!(skipped.skipped_existing);
    }
}
