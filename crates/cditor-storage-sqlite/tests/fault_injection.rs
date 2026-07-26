use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use cditor_core::edit::{EditOperation, EditTransaction, EditTransactionKind};
use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
use cditor_storage::layout_cache::LayoutCacheKey;
use cditor_storage::{DocumentStorage, LoadDocumentRequest, StorageSaveBatch};
use cditor_storage_sqlite::{SqliteDocumentStorage, SqliteStorageOptions};
use tempfile::TempDir;

const CHILD_ENV: &str = "CDITOR_SQLITE_FAULT_CHILD";

fn load_request() -> LoadDocumentRequest {
    LoadDocumentRequest {
        document_id: 7,
        workspace_id: 1,
        initial_payload_window_blocks: 1,
        visible_index_version: cditor_storage::DOCUMENT_INDEX_VISIBLE_VERSION,
        layout_key: LayoutCacheKey {
            width_bucket: 1,
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

async fn open(path: &std::path::Path) -> SqliteDocumentStorage {
    SqliteDocumentStorage::open(SqliteStorageOptions::file(path))
        .await
        .unwrap()
}

fn save_batch(text: &str, with_transaction: bool) -> StorageSaveBatch {
    let transactions = with_transaction
        .then(|| {
            EditTransaction::new(
                91,
                EditTransactionKind::ExplicitCommand,
                2,
                vec![EditOperation::InsertText {
                    block_id: 1,
                    offset: 0,
                    text: text.to_owned(),
                }],
                vec![],
            )
        })
        .into_iter()
        .collect();
    StorageSaveBatch {
        document_id: 7,
        layout_key: None,
        payloads: vec![BlockPayloadRecord {
            block_id: 1,
            content_version: u64::from(with_transaction) + 2,
            kind: RichBlockKind::Paragraph,
            payload: cditor_core::rich_text::BlockPayload::RichText {
                spans: vec![cditor_core::rich_text::InlineSpan::plain(text)],
            },
        }],
        index_records: vec![],
        structure_version: 1,
        transactions,
        block_attrs: vec![],
        page_layout_snapshot: None,
    }
}

#[tokio::test]
async fn fault_injection_child() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }
    let path = std::env::var_os("CDITOR_SQLITE_FAULT_DB").unwrap();
    let store = open(std::path::Path::new(&path)).await;
    store
        .commit(save_batch("committed after kill point", true))
        .await
        .unwrap();
    panic!("fault-injection child escaped its configured pause point");
}

#[cfg(unix)]
#[tokio::test]
async fn kill_9_at_every_commit_point_is_atomic_and_recoverable() {
    for (point, committed) in [
        ("transaction_opened", false),
        ("materialized_written", false),
        ("journal_outbox_written", false),
        ("sqlite_commit_returned", true),
    ] {
        let dir = TempDir::new().unwrap();
        let database = dir.path().join("fault.db");
        let marker = dir.path().join("paused.marker");
        let store = open(&database).await;
        store.load_document(load_request()).await.unwrap();
        store.commit(save_batch("baseline", false)).await.unwrap();
        store.pool().close().await;

        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("integration_tests::fault_injection::fault_injection_child")
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env(CHILD_ENV, "1")
            .env("CDITOR_SQLITE_FAULT_DB", &database)
            .env("CDITOR_SQLITE_KILL_POINT", point)
            .env("CDITOR_SQLITE_KILL_MARKER", &marker)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !marker.exists() {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("fault child exited before {point}: {status}");
            }
            assert!(Instant::now() < deadline, "fault child never reached {point}");
            std::thread::sleep(Duration::from_millis(10));
        }
        child.kill().expect("send SIGKILL to paused child");
        let status = child.wait().unwrap();
        assert!(!status.success());

        let recovered = open(&database).await;
        let loaded = recovered.load_document(load_request()).await.unwrap();
        assert_eq!(
            loaded.initial_payloads[0].plain_text(),
            if committed {
                "committed after kill point"
            } else {
                "baseline"
            },
            "unexpected materialized state after kill at {point}"
        );
        assert_eq!(
            recovered.journal_entries_after_checkpoint(7).await.unwrap().len(),
            usize::from(committed),
            "journal must agree with materialized state at {point}"
        );
        assert_eq!(
            recovered.outbox_entries(7).await.unwrap().len(),
            usize::from(committed),
            "outbox must agree with materialized state at {point}"
        );
        let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(recovered.pool())
            .await
            .unwrap();
        assert_eq!(integrity, "ok");
    }
}
