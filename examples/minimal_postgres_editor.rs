use std::{env, time::Duration};

use CDitor_V2::Cditor;
use CDitor_V2::core::rich_text::{RichBlockRecord, RichTextDocument};
use CDitor_V2::storage::postgres::{
    DocumentRow, PostgresDocumentStore, PostgresPayloadStore, PostgresPoolConfig,
    PostgresStorageError, PostgresStorageResult, create_pg_pool, pg_document_id_from_runtime,
    run_migrations,
};
use gpui::*;
use sqlx::types::Uuid;

const DEFAULT_DOCUMENT_ID: u64 = 1;
const DEFAULT_WORKSPACE_ID: u64 = 1;
const MINIMAL_WORKSPACE_NAMESPACE: u128 = 0x9400_0000_0000_0000_0000_0000_0000_0000;

fn main() {
    let database_url = env::var("CDITOR_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5432/cditor_dev".to_owned());
    let document_id = env_u64("CDITOR_DOCUMENT_ID").unwrap_or(DEFAULT_DOCUMENT_ID);
    let workspace_id = env_u64("CDITOR_WORKSPACE_ID").unwrap_or(DEFAULT_WORKSPACE_ID);
    let title = env::var("CDITOR_DOCUMENT_TITLE").unwrap_or_else(|_| "Untitled".to_owned());
    let autosave_secs = env_u64("CDITOR_AUTOSAVE_SECS").unwrap_or(10);

    let database_url_for_init = database_url.clone();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("create tokio runtime")
        .block_on(async {
            let pool = create_pg_pool(&minimal_postgres_pool_config(database_url_for_init.clone()))
                .await
                .unwrap_or_else(|error| {
                    panic!(
                        "connect postgres failed: {error}. Check docker compose is running and CDITOR_DATABASE_URL is correct: {database_url_for_init}"
                    )
                });
            run_migrations(&pool).await.expect("run migrations");
            ensure_minimal_document(&pool, document_id, workspace_id, &title)
                .await
                .expect("ensure minimal document");
        });

    let app = gpui_platform::application();
    app.run(move |cx: &mut App| {
        cx.activate(true);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: Size {
                        width: px(960.0),
                        height: px(640.0),
                    },
                })),
                titlebar: Some(TitlebarOptions {
                    title: Some(title.clone().into()),
                    appears_transparent: false,
                    ..Default::default()
                }),
                ..Default::default()
            },
            {
                let database_url = database_url.clone();
                move |_window, cx| {
                    cx.new(|cx| {
                        Cditor::new()
                            .with_document_id(document_id)
                            .with_postgres_url(database_url.clone())
                            .with_payload_window_size(64)
                            .with_debug_overlay(false)
                            .with_autosave(autosave_secs)
                            .build_view(cx)
                    })
                }
            },
        )
        .expect("open minimal postgres editor window");
    });
}

async fn ensure_minimal_document(
    pool: &sqlx::PgPool,
    document_id: u64,
    workspace_id: u64,
    title: &str,
) -> PostgresStorageResult<()> {
    let pg_document_id = pg_document_id_from_runtime(document_id);
    let document_store = PostgresDocumentStore::new(pool.clone());
    let payload_store = PostgresPayloadStore::new(pool.clone());

    match document_store.load_document_metadata(pg_document_id).await {
        Ok(_) => return Ok(()),
        Err(PostgresStorageError::NotFound { .. }) => {}
        Err(error) => return Err(error),
    }

    let root_block_id = initial_root_block_id(document_id);
    let mut document = RichTextDocument::empty(document_id);
    document.push_root_block(RichBlockRecord::paragraph(root_block_id, ""));

    let index_records = document.index_records();
    let payload_records = document.payload_records();
    let structure_version = i64::try_from(document.structure_version).unwrap_or(1);

    document_store
        .save_document_metadata(&DocumentRow {
            id: pg_document_id,
            workspace_id: workspace_uuid(workspace_id),
            title: title.to_owned(),
            structure_version,
            content_version: 1,
            layout_version: 0,
            schema_version: 1,
        })
        .await?;
    document_store
        .save_block_index_records(pg_document_id, &index_records, structure_version)
        .await?;
    document_store
        .save_document_index_snapshot(pg_document_id, 1, structure_version, &index_records)
        .await?;
    payload_store
        .save_block_payloads(pg_document_id, &payload_records)
        .await?;

    Ok(())
}

fn minimal_postgres_pool_config(database_url: String) -> PostgresPoolConfig {
    let mut config = PostgresPoolConfig::new(database_url);
    config.min_connections = 0;
    config.acquire_timeout =
        Duration::from_secs(env_u64("CDITOR_POSTGRES_TIMEOUT_SECS").unwrap_or(15));
    config
}

fn initial_root_block_id(document_id: u64) -> u64 {
    document_id.saturating_mul(1_000_000).saturating_add(1)
}

fn workspace_uuid(workspace_id: u64) -> Uuid {
    Uuid::from_u128(MINIMAL_WORKSPACE_NAMESPACE | workspace_id as u128)
}

fn env_u64(name: &str) -> Option<u64> {
    env::var(name).ok()?.parse().ok()
}
