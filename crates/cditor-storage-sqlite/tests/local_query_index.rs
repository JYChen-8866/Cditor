use cditor_core::document::BlockIndexRecord;
use cditor_core::rich_text::{
    BlockPayload, BlockPayloadRecord, InlineMark, InlineSpan, RichBlockKind,
    kind_tag_for_rich_block_kind,
};
use cditor_storage::query_index::{
    BacklinkKind, LocalIndexRebuildRequest, LocalSearchRequest,
};
use cditor_storage::{DocumentStorage, LoadDocumentRequest, StorageSaveBatch};
use cditor_storage_sqlite::{SqliteDocumentStorage, SqliteStorageOptions};
use tempfile::TempDir;

async fn open_store(dir: &TempDir) -> SqliteDocumentStorage {
    SqliteDocumentStorage::open(SqliteStorageOptions::file(dir.path().join("query.db")))
        .await
        .unwrap()
}

fn load_request(document_id: u64, workspace_id: u64) -> LoadDocumentRequest {
    LoadDocumentRequest {
        document_id,
        workspace_id,
        initial_payload_window_blocks: 16,
        visible_index_version: cditor_storage::DOCUMENT_INDEX_VISIBLE_VERSION,
        layout_key: cditor_storage::layout_cache::LayoutCacheKey {
            width_bucket: 10,
            exact_width_px: 800,
            content_version: 1,
            attrs_version: 0,
            style_version: 0,
            font_version: 0,
            theme_version: 0,
            scale_factor_milli: 1000,
        },
        page_policy_version: cditor_core::layout::PAGE_POLICY_VERSION,
    }
}

fn rich_payload(version: u64, text: &str, hrefs: &[&str]) -> BlockPayloadRecord {
    let marks = hrefs
        .iter()
        .map(|href| InlineMark::Link {
            href: (*href).to_owned(),
        })
        .collect();
    BlockPayloadRecord {
        block_id: 1,
        content_version: version,
        kind: RichBlockKind::Paragraph,
        payload: BlockPayload::RichText {
            spans: vec![InlineSpan {
                text: text.to_owned(),
                marks,
            }],
        },
    }
}

async fn commit_payload(store: &SqliteDocumentStorage, payload: BlockPayloadRecord) {
    store
        .commit(StorageSaveBatch {
            document_id: 7,
            layout_key: None,
            payloads: vec![payload],
            index_records: vec![],
            structure_version: 1,
            transactions: vec![],
            block_attrs: vec![],
            page_layout_snapshot: None,
        })
        .await
        .unwrap();
}

async fn search(store: &SqliteDocumentStorage, query: &str) -> Vec<u64> {
    store
        .search_local(LocalSearchRequest {
            workspace_id: 1,
            document_id: Some(7),
            query: query.to_owned(),
            limit: 20,
        })
        .await
        .unwrap()
        .into_iter()
        .map(|hit| hit.block_id)
        .collect()
}

#[tokio::test]
async fn committed_payload_incrementally_replaces_fts_and_backlinks() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    store.load_document(load_request(7, 1)).await.unwrap();
    store.load_document(load_request(8, 1)).await.unwrap();

    commit_payload(
        &store,
        rich_payload(
            2,
            "alpha searchable",
            &[
                "cditor://document/8",
                "cditor://document/8/block/1",
                "https://example.com/not-a-backlink",
            ],
        ),
    )
    .await;
    let initial_fts_rowid: i64 =
        sqlx::query_scalar("SELECT fts_rowid FROM block_fts_state LIMIT 1")
            .fetch_one(store.pool())
            .await
            .unwrap();
    assert_eq!(search(&store, "alpha").await, vec![1]);
    let backlinks = store.backlinks(8, None, 20).await.unwrap();
    assert_eq!(backlinks.len(), 2);
    assert!(backlinks.iter().all(|link| link.resolved));
    assert!(
        backlinks
            .iter()
            .all(|link| link.kind == BacklinkKind::InlineLink)
    );

    commit_payload(
        &store,
        rich_payload(3, "beta replacement", &["cditor://document/99"]),
    )
    .await;
    let replacement_fts_rowid: i64 =
        sqlx::query_scalar("SELECT fts_rowid FROM block_fts_state LIMIT 1")
            .fetch_one(store.pool())
            .await
            .unwrap();
    let fts_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM block_fts")
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(replacement_fts_rowid, initial_fts_rowid);
    assert_eq!(fts_rows, 1);
    assert!(search(&store, "alpha").await.is_empty());
    assert_eq!(search(&store, "beta").await, vec![1]);
    assert!(store.backlinks(8, None, 20).await.unwrap().is_empty());
    let unresolved = store.backlinks(99, None, 20).await.unwrap();
    assert_eq!(unresolved.len(), 1);
    assert!(!unresolved[0].resolved);
}

#[tokio::test]
async fn search_is_scoped_by_workspace_and_document_and_reports_version() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    store.load_document(load_request(7, 1)).await.unwrap();
    commit_payload(&store, rich_payload(4, "offline command palette", &[])).await;

    let hits = store
        .search_local(LocalSearchRequest {
            workspace_id: 1,
            document_id: Some(7),
            query: "offline palette".to_owned(),
            limit: 10,
        })
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].content_version, 4);
    assert!(
        store
            .search_local(LocalSearchRequest {
                workspace_id: 2,
                document_id: None,
                query: "offline".to_owned(),
                limit: 10,
            })
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn structural_delete_prunes_search_and_source_backlinks() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    store.load_document(load_request(7, 1)).await.unwrap();
    store.load_document(load_request(8, 1)).await.unwrap();
    commit_payload(
        &store,
        rich_payload(2, "remove this", &["cditor://document/8"]),
    )
    .await;

    let second = BlockIndexRecord::new(
        2,
        None,
        0,
        kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
        0,
    );
    store
        .commit(StorageSaveBatch {
            document_id: 7,
            layout_key: None,
            payloads: vec![BlockPayloadRecord::rich_text(
                2,
                RichBlockKind::Paragraph,
                "survivor",
            )],
            index_records: vec![second],
            structure_version: 2,
            transactions: vec![],
            block_attrs: vec![],
            page_layout_snapshot: None,
        })
        .await
        .unwrap();

    assert!(search(&store, "remove").await.is_empty());
    assert!(store.backlinks(8, None, 20).await.unwrap().is_empty());
    assert_eq!(search(&store, "survivor").await, vec![2]);
}

#[tokio::test]
async fn bounded_rebuild_repairs_missing_fts_and_backlink_projection() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    store.load_document(load_request(7, 1)).await.unwrap();
    store.load_document(load_request(8, 1)).await.unwrap();
    commit_payload(
        &store,
        rich_payload(5, "repairable index", &["cditor://document/8"]),
    )
    .await;
    sqlx::query("DELETE FROM block_fts")
        .execute(store.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM block_fts_state")
        .execute(store.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM document_links")
        .execute(store.pool())
        .await
        .unwrap();

    let rebuilt = store
        .rebuild_local_query_index(LocalIndexRebuildRequest {
            document_id: 7,
            reset: true,
            max_blocks: 1,
        })
        .await
        .unwrap();
    assert_eq!(rebuilt.applied, 1);
    assert_eq!(rebuilt.remaining, 0);
    assert_eq!(search(&store, "repairable").await, vec![1]);
    assert_eq!(store.backlinks(8, None, 10).await.unwrap().len(), 1);
}
