// P7-003/004/009 基础层测试：journal/outbox 原子性、checkpoint、crash
// marker，以及 journal -> envelope 解码 -> Runtime 回放的端到端恢复。

use cditor_core::edit::{
    ChangeOrigin, EditOperation, EditTransaction, EditTransactionKind, decode_transaction,
    encode_transaction, transaction_envelope_from_raw,
};
use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
use cditor_core::schema::{SchemaDomain, SchemaVersion};
use cditor_runtime::DocumentRuntime;
use cditor_storage::layout_cache::LayoutCacheKey;
use cditor_storage::{DocumentStorage, LoadDocumentRequest, StorageSaveBatch};
use cditor_storage_sqlite::{
    OutboxState, SqliteDocumentStorage, SqliteStorageOptions, StartupRecovery,
};
use tempfile::TempDir;

async fn open_store(dir: &TempDir) -> SqliteDocumentStorage {
    SqliteDocumentStorage::open(SqliteStorageOptions::file(dir.path().join("journal.db")))
        .await
        .expect("open sqlite store")
}

fn sample_transaction(id: u64, text: &str) -> EditTransaction {
    EditTransaction::new(
        id,
        EditTransactionKind::ExplicitCommand,
        id,
        vec![EditOperation::InsertText {
            block_id: 1,
            offset: 0,
            text: text.to_owned(),
        }],
        vec![EditOperation::DeleteText {
            block_id: 1,
            range: 0..text.len(),
        }],
    )
}

fn envelope_json(transaction: &EditTransaction) -> String {
    let envelope = encode_transaction(transaction).expect("encode");
    serde_json::to_string(&envelope).expect("serialize envelope")
}

fn load_request(document_id: u64) -> LoadDocumentRequest {
    LoadDocumentRequest {
        document_id,
        workspace_id: 1,
        initial_payload_window_blocks: 16,
        visible_index_version: cditor_storage::DOCUMENT_INDEX_VISIBLE_VERSION,
        layout_key: LayoutCacheKey {
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

#[tokio::test]
async fn emergency_log_batch_is_ordered_idempotent_and_acknowledgeable() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    let transactions = vec![sample_transaction(11, "a"), sample_transaction(12, "b")];

    let first = store
        .append_emergency_transactions(7, &transactions)
        .await
        .expect("append emergency batch");
    assert_eq!(first.appended, 2);
    let through_sequence = first.through_sequence.expect("sequence");

    let retry = store
        .append_emergency_transactions(7, &transactions)
        .await
        .expect("deduplicate emergency batch");
    assert_eq!(retry.appended, 0);
    assert_eq!(retry.through_sequence, Some(through_sequence));

    let entries = store
        .load_emergency_transactions(7)
        .await
        .expect("load emergency log");
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.transaction_id)
            .collect::<Vec<_>>(),
        vec![11, 12]
    );
    assert!(
        entries
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence)
    );
    for (entry, expected) in entries.iter().zip(&transactions) {
        let decoded = decode_transaction(&entry.envelope)
            .expect("decode")
            .transaction()
            .expect("compatible transaction")
            .clone();
        assert_eq!(&decoded, expected);
    }

    assert_eq!(
        store
            .acknowledge_emergency_transactions(7, through_sequence)
            .await
            .expect("acknowledge"),
        2
    );
    assert!(
        store
            .load_emergency_transactions(7)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn journal_append_and_replay_order() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    for id in 1..=3u64 {
        let transaction = sample_transaction(id, &format!("t{id}:"));
        store
            .append_transaction_to_journal(
                7,
                id,
                SchemaDomain::Operation.current_version(),
                &envelope_json(&transaction),
                ChangeOrigin::User,
                true,
            )
            .await
            .expect("append");
    }

    let entries = store
        .journal_entries_after_checkpoint(7)
        .await
        .expect("read");
    assert_eq!(entries.len(), 3);
    assert!(
        entries
            .windows(2)
            .all(|pair| pair[0].journal_id < pair[1].journal_id)
    );
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.transaction_id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    // 其他文档不受影响。
    assert!(
        store
            .journal_entries_after_checkpoint(8)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn outbox_rows_are_written_atomically_with_journal() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    let transaction = sample_transaction(1, "hello");
    store
        .append_transaction_to_journal(
            7,
            1,
            SchemaDomain::Operation.current_version(),
            &envelope_json(&transaction),
            ChangeOrigin::User,
            true,
        )
        .await
        .expect("append with outbox");
    // remote 回放不入 outbox。
    store
        .append_transaction_to_journal(
            7,
            2,
            SchemaDomain::Operation.current_version(),
            &envelope_json(&sample_transaction(2, "remote")),
            ChangeOrigin::Remote,
            false,
        )
        .await
        .expect("append without outbox");

    let outbox = store.outbox_entries(7).await.expect("outbox");
    assert_eq!(outbox.len(), 1);
    assert_eq!(outbox[0].state, OutboxState::Pending);
    assert_eq!(outbox[0].attempt_count, 0);

    // 状态机：pending -> inflight（计数 +1）-> rejected（带错误）-> acked。
    store
        .set_outbox_state(outbox[0].outbox_id, OutboxState::Inflight, None)
        .await
        .unwrap();
    store
        .set_outbox_state(
            outbox[0].outbox_id,
            OutboxState::Rejected,
            Some("schema mismatch"),
        )
        .await
        .unwrap();
    let after = store.outbox_entries(7).await.unwrap();
    assert_eq!(after[0].state, OutboxState::Rejected);
    assert_eq!(after[0].attempt_count, 1);
    assert_eq!(after[0].last_error.as_deref(), Some("schema mismatch"));

    store
        .set_outbox_state(outbox[0].outbox_id, OutboxState::Acked, None)
        .await
        .unwrap();
    assert_eq!(
        store.outbox_entries(7).await.unwrap()[0].state,
        OutboxState::Acked
    );
    assert_eq!(
        store
            .sync_ack_cursor(7)
            .await
            .unwrap()
            .unwrap()
            .pushed_outbox_id,
        Some(outbox[0].outbox_id)
    );
}

#[tokio::test]
async fn inbox_is_idempotent_bounded_and_advances_pull_cursor_with_apply() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    assert!(
        store
            .enqueue_inbox(7, "batch-a", "cursor-1", r#"{"ops":[1]}"#)
            .await
            .unwrap()
    );
    assert!(
        !store
            .enqueue_inbox(7, "batch-a", "cursor-1", r#"{"ops":[1]}"#)
            .await
            .unwrap(),
        "network retry must not duplicate an inbox batch"
    );
    store
        .enqueue_inbox(7, "batch-b", "cursor-2", r#"{"ops":[2]}"#)
        .await
        .unwrap();
    store
        .enqueue_inbox(8, "other-document", "cursor-x", "{}")
        .await
        .unwrap();

    let first = store.pending_inbox(7, 1).await.unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].batch_id, "batch-a");
    assert!(
        store
            .mark_inbox_applied(first[0].inbox_id, "cursor-1")
            .await
            .unwrap()
    );
    assert!(
        !store
            .mark_inbox_applied(first[0].inbox_id, "cursor-1")
            .await
            .unwrap(),
        "an applied inbox row must not apply twice"
    );
    assert_eq!(
        store
            .sync_ack_cursor(7)
            .await
            .unwrap()
            .unwrap()
            .pulled_cursor
            .as_deref(),
        Some("cursor-1")
    );
    assert_eq!(store.pending_inbox(7, 10).await.unwrap()[0].batch_id, "batch-b");
    assert_eq!(store.pending_inbox(8, 10).await.unwrap().len(), 1);
}

#[tokio::test]
async fn materialized_commit_reuses_emergency_journal_and_creates_outbox_atomically() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;
    store.load_document(load_request(7)).await.unwrap();
    let edit = sample_transaction(41, "durable");

    let emergency = store
        .append_emergency_transactions(7, std::slice::from_ref(&edit))
        .await
        .unwrap();
    let journal_id = emergency.through_sequence.unwrap() as i64;
    assert!(store.outbox_entries(7).await.unwrap().is_empty());

    let batch = StorageSaveBatch {
        document_id: 7,
        layout_key: None,
        payloads: Vec::new(),
        index_records: Vec::new(),
        structure_version: 1,
        transactions: vec![edit.clone()],
        block_attrs: Vec::new(),
        page_layout_snapshot: None,
    };
    store.commit(batch.clone()).await.unwrap();

    let journal = store.journal_entries_after_checkpoint(7).await.unwrap();
    let outbox = store.outbox_entries(7).await.unwrap();
    assert_eq!(journal.len(), 1, "commit must reuse the emergency row");
    assert_eq!(journal[0].journal_id, journal_id);
    assert_eq!(outbox.len(), 1);
    assert_eq!(outbox[0].journal_id, journal_id);
    assert_eq!(outbox[0].state, OutboxState::Pending);
    assert!(
        store
            .load_emergency_transactions(7)
            .await
            .unwrap()
            .is_empty(),
        "the same SQLite commit must materialize the transaction"
    );

    store.commit(batch).await.unwrap();
    assert_eq!(store.journal_entries_after_checkpoint(7).await.unwrap().len(), 1);
    assert_eq!(store.outbox_entries(7).await.unwrap().len(), 1);
}

#[tokio::test]
async fn checkpoint_excludes_absorbed_entries_and_compaction_respects_outbox() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    let mut journal_ids = Vec::new();
    for id in 1..=3u64 {
        journal_ids.push(
            store
                .append_transaction_to_journal(
                    7,
                    id,
                    SchemaDomain::Operation.current_version(),
                    &envelope_json(&sample_transaction(id, "x")),
                    ChangeOrigin::User,
                    true,
                )
                .await
                .unwrap(),
        );
    }

    // checkpoint 到第二条：replay 只看到第三条。
    store
        .record_journal_checkpoint(7, journal_ids[1], 0xC0DE)
        .await
        .unwrap();
    let entries = store.journal_entries_after_checkpoint(7).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].journal_id, journal_ids[2]);
    assert_eq!(
        store.journal_checkpoint_checksum(7).await.unwrap(),
        Some((journal_ids[1], 0xC0DE))
    );

    // outbox 未 ack：compact 不得删除任何条目。
    assert_eq!(store.compact_journal(7).await.unwrap(), 0);

    // ack 前两条后 compact 只清理 checkpoint 内的两条。
    let outbox = store.outbox_entries(7).await.unwrap();
    for entry in &outbox[..2] {
        store
            .set_outbox_state(entry.outbox_id, OutboxState::Acked, None)
            .await
            .unwrap();
    }
    assert_eq!(store.compact_journal(7).await.unwrap(), 2);
    // 未被 checkpoint 吸收的第三条仍在。
    assert_eq!(
        store
            .journal_entries_after_checkpoint(7)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn crash_marker_detects_unclean_shutdown() {
    let dir = TempDir::new().unwrap();
    {
        let store = open_store(&dir).await;
        assert_eq!(
            store.begin_session_with_crash_marker().await.unwrap(),
            StartupRecovery::CleanStart
        );
        // 不调用 mark_clean_shutdown：模拟崩溃。
    }
    {
        let store = open_store(&dir).await;
        assert!(matches!(
            store.begin_session_with_crash_marker().await.unwrap(),
            StartupRecovery::CrashDetected { .. }
        ));
        store.mark_clean_shutdown().await.unwrap();
    }
    {
        let store = open_store(&dir).await;
        assert_eq!(
            store.begin_session_with_crash_marker().await.unwrap(),
            StartupRecovery::CleanStart
        );
    }
}

/// 端到端崩溃恢复：journal 条目 -> envelope 解码 -> Runtime 原子回放 ->
/// 语义与直接编辑后的 runtime 一致。
#[tokio::test]
async fn journal_replay_reconstructs_runtime_state() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    let base_payloads = || {
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "base",
        )]
    };

    // "在线" runtime 执行两个事务并把它们写入 journal。
    let mut live = DocumentRuntime::from_payloads(9, base_payloads(), 720.0);
    let transactions = vec![
        sample_transaction(1, "hello "),
        EditTransaction::new(
            2,
            EditTransactionKind::ExplicitCommand,
            2,
            vec![
                EditOperation::SplitBlock {
                    block_id: 1,
                    offset: 5,
                    new_block_id: 30,
                },
                EditOperation::InsertText {
                    block_id: 30,
                    offset: 0,
                    text: ">>".to_owned(),
                },
            ],
            vec![],
        ),
    ];
    for transaction in &transactions {
        live.apply_external_transaction(transaction, ChangeOrigin::Remote)
            .expect("live apply");
        store
            .append_transaction_to_journal(
                9,
                transaction.id,
                SchemaDomain::Operation.current_version(),
                &envelope_json(transaction),
                ChangeOrigin::Remote,
                false,
            )
            .await
            .expect("journal");
    }

    // "崩溃后" 的 runtime 从基线 + journal replay 重建。
    let mut recovered = DocumentRuntime::from_payloads(9, base_payloads(), 720.0);
    for entry in store.journal_entries_after_checkpoint(9).await.unwrap() {
        let envelope: cditor_core::schema::VersionedEnvelope =
            serde_json::from_str(&entry.envelope_json).expect("parse envelope");
        let decoded = decode_transaction(&envelope).expect("decode");
        let transaction = decoded.transaction().expect("compatible").clone();
        recovered
            .apply_external_transaction(&transaction, ChangeOrigin::Remote)
            .expect("replay");
    }

    // 语义一致：结构顺序与全部文本。
    let state = |runtime: &DocumentRuntime| -> Vec<(u64, String)> {
        runtime
            .index_records_snapshot()
            .into_iter()
            .map(|record| {
                let block_id = record.id;
                (
                    block_id,
                    runtime.block_payload_record(block_id).unwrap().plain_text(),
                )
            })
            .collect()
    };
    assert_eq!(state(&recovered), state(&live));
    assert_eq!(state(&live)[0].1, "hello");
    assert_eq!(state(&live)[1].1, ">> base");
}

/// 未来版本的 journal 条目：解码为只读，不得回放也不得丢字节。
#[tokio::test]
async fn newer_major_journal_entry_is_preserved_not_replayed() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    let alien_body = r#"{"ops_v99": [1, 2, 3]}"#;
    let alien_envelope = transaction_envelope_from_raw(
        SchemaVersion::new(99, 0),
        serde_json::value::RawValue::from_string(alien_body.to_owned()).unwrap(),
    );
    store
        .append_transaction_to_journal(
            9,
            1,
            SchemaVersion::new(99, 0),
            &serde_json::to_string(&alien_envelope).unwrap(),
            ChangeOrigin::User,
            true,
        )
        .await
        .unwrap();

    let entries = store.journal_entries_after_checkpoint(9).await.unwrap();
    assert_eq!(entries.len(), 1);
    let envelope: cditor_core::schema::VersionedEnvelope =
        serde_json::from_str(&entries[0].envelope_json).unwrap();
    let decoded = decode_transaction(&envelope).expect("decode outcome");
    assert!(decoded.transaction().is_none(), "must not be replayable");
    assert_eq!(envelope.body_bytes(), alien_body, "raw bytes preserved");
}
