use sqlx::PgPool;
use sqlx::migrate::Migrator;

use super::error::{PostgresStorageError, PostgresStorageResult};

pub const INITIAL_SCHEMA_VERSION: i64 = 1;
pub const INITIAL_SCHEMA_MIGRATION: &str = "0001_initial.sql";

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub async fn run_migrations(pool: &PgPool) -> PostgresStorageResult<()> {
    MIGRATOR
        .run(pool)
        .await
        .map_err(|error| PostgresStorageError::Migration(error.to_string()))
}

pub fn initial_schema_sql() -> &'static str {
    include_str!("../migrations/0001_initial.sql")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PostgresPoolConfig, create_pg_pool, health_check};

    #[test]
    fn initial_schema_contains_all_v1_tables_and_key_indexes() {
        let sql = initial_schema_sql();
        let tables = [
            "schema_migrations_meta",
            "workspaces",
            "documents",
            "document_tree",
            "blocks",
            "block_attrs",
            "block_payloads",
            "block_code_meta",
            "block_tables",
            "block_table_rows",
            "block_table_cells",
            "assets",
            "block_assets",
            "block_layout",
            "page_layout",
            "document_index_snapshot",
            "edit_transactions",
            "undo_snapshots",
            "persistence_queue",
            "runtime_snapshots",
            "collections",
            "collection_properties",
            "collection_views",
            "collection_rows",
            "collection_cells",
            "database_block_bindings",
            "sync_outbox",
            "sync_state",
            "remote_tombstones",
            "block_search",
        ];

        for table in tables {
            let create = format!("CREATE TABLE {table}");
            let create_if_not_exists = format!("CREATE TABLE IF NOT EXISTS {table}");
            assert!(
                sql.contains(&create) || sql.contains(&create_if_not_exists),
                "missing table {table}"
            );
        }

        assert!(sql.contains("idx_blocks_document_sort"));
        assert!(sql.contains("idx_block_payloads_document_id"));
        assert!(sql.contains("idx_block_layout_document_id"));
        assert!(sql.contains("idx_sync_outbox_state_sequence"));
        assert!(sql.contains("USING GIN(search_vector)"));
    }

    #[test]
    fn initial_schema_uses_postgres_types_not_sqlite_types() {
        let sql = initial_schema_sql();

        assert!(sql.contains("UUID"));
        assert!(sql.contains("JSONB"));
        assert!(sql.contains("TIMESTAMPTZ"));
        assert!(sql.contains("TSVECTOR"));
        assert!(!sql.contains("CREATE VIRTUAL TABLE"));
        assert!(!sql.contains("FTS5"));
    }

    #[tokio::test]
    #[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
    async fn migrations_run_against_postgres_test_database() {
        let database_url = std::env::var("CDITOR_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned());
        let config = PostgresPoolConfig::for_tests(database_url);
        let pool = create_pg_pool(&config).await.unwrap();

        health_check(&pool).await.unwrap();
        run_migrations(&pool).await.unwrap();
        run_migrations(&pool).await.unwrap();
    }
}
