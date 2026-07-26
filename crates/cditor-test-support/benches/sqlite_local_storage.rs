//! Real SQLite 100k local open/save/checkpoint/compact benchmark (P7-016).
//!
//! Run: `cargo bench -p cditor-test-support --bench sqlite_local_storage -- --full`
//! Report: `target/benchmark-reports/sqlite-local-storage-<mode>.json`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use cditor_core::edit::{EditOperation, EditTransaction, EditTransactionKind};
use cditor_core::rich_text::{BlockPayloadRecord, RichBlockKind};
use cditor_storage::layout_cache::LayoutCacheKey;
use cditor_storage::{
    DOCUMENT_INDEX_VISIBLE_VERSION, DocumentStorage, LoadDocumentRequest, StorageSaveBatch,
};
use cditor_storage_sqlite::{OutboxState, SqliteDocumentStorage, SqliteStorageOptions};
use cditor_test_support::seed_mixed_storage_document;
use serde::Serialize;
use tempfile::TempDir;

const REPORT_SCHEMA_VERSION: u32 = 1;
const HARNESS_VERSION: &str = "sqlite-local-storage-v1";
const DOCUMENT_ID: u64 = 70_016;
const INITIAL_PAYLOAD_WINDOW: usize = 128;

type BenchResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone, Copy)]
enum Mode {
    Quick,
    Standard,
    Full,
}

impl Mode {
    fn from_args() -> Self {
        let args = std::env::args().collect::<Vec<_>>();
        if args.iter().any(|arg| arg == "--quick") {
            Self::Quick
        } else if args.iter().any(|arg| arg == "--full") {
            Self::Full
        } else {
            Self::Standard
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Standard => "standard",
            Self::Full => "full",
        }
    }

    const fn block_count(self) -> usize {
        match self {
            Self::Quick => 4_096,
            Self::Standard => 20_000,
            Self::Full => 100_000,
        }
    }

    const fn open_samples(self) -> usize {
        match self {
            Self::Quick => 3,
            Self::Standard => 6,
            Self::Full => 12,
        }
    }

    const fn save_samples(self) -> usize {
        match self {
            Self::Quick => 8,
            Self::Standard => 20,
            Self::Full => 50,
        }
    }

    const fn checkpoint_samples(self) -> usize {
        match self {
            Self::Quick => 1,
            Self::Standard => 2,
            Self::Full => 3,
        }
    }
}

#[derive(Debug, Serialize)]
struct Distribution {
    samples: usize,
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
}

#[derive(Debug, Serialize)]
struct StorageFootprint {
    database_bytes: u64,
    wal_bytes: u64,
    shm_bytes: u64,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    schema_version: u32,
    harness_version: &'static str,
    mode: &'static str,
    profile: &'static str,
    target_os: &'static str,
    target_arch: &'static str,
    logical_cores: usize,
    filesystem_cache_policy: &'static str,
    block_count: usize,
    initial_payload_window: usize,
    seed_elapsed_ms: f64,
    footprint_before: StorageFootprint,
    reopen_and_load: Distribution,
    loaded_index_blocks: usize,
    loaded_initial_payloads: usize,
    durable_single_block_save: Distribution,
    full_structure_save: Distribution,
    materialized_checkpoint: Distribution,
    compact_journal: Distribution,
    compacted_operations: u64,
    wal_flush: Distribution,
    footprint_after: StorageFootprint,
    passed: bool,
    failures: Vec<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(report) => {
            let json = serde_json::to_string_pretty(&report).expect("serialize benchmark report");
            println!("{json}");
            if let Err(error) = write_report(report.mode, &json) {
                eprintln!("failed to write benchmark report: {error}");
                return ExitCode::FAILURE;
            }
            if report.passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("SQLite local-storage benchmark failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> BenchResult<BenchmarkReport> {
    let mode = Mode::from_args();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(run_async(mode))
}

async fn run_async(mode: Mode) -> BenchResult<BenchmarkReport> {
    let temp = TempDir::new()?;
    let database_path = temp.path().join("local-storage.cditor.db");
    let options = SqliteStorageOptions::file(&database_path).max_connections(1);
    let storage = SqliteDocumentStorage::open(options.clone()).await?;

    eprintln!("stage=seed blocks={}", mode.block_count());
    let seed_started = Instant::now();
    seed_mixed_storage_document(&storage, DOCUMENT_ID, mode.block_count()).await?;
    storage.flush().await?;
    let seed_elapsed_ms = elapsed_ms(seed_started.elapsed());
    let footprint_before = footprint(&database_path);
    drop(storage);

    eprintln!("stage=reopen samples={}", mode.open_samples());
    let mut open_samples = Vec::with_capacity(mode.open_samples());
    let mut loaded_index_blocks = 0;
    let mut loaded_initial_payloads = 0;
    for _ in 0..mode.open_samples() {
        let started = Instant::now();
        let storage = SqliteDocumentStorage::open(options.clone()).await?;
        let loaded = storage.load_document(load_request()).await?;
        open_samples.push(elapsed_ms(started.elapsed()));
        loaded_index_blocks = loaded.records.len();
        loaded_initial_payloads = loaded.initial_payloads.len();
        drop(storage);
    }

    eprintln!("stage=durable-save samples={}", mode.save_samples());
    let storage = SqliteDocumentStorage::open(options).await?;
    let loaded = storage.load_document(load_request()).await?;
    let block_id = loaded.records[0].id;
    let mut content_version = loaded.initial_payloads[0].content_version;
    let mut transaction_id = 1_000_000u64;
    let mut save_samples = Vec::with_capacity(mode.save_samples());
    for sample in 0..mode.save_samples() {
        content_version = content_version.saturating_add(1);
        transaction_id = transaction_id.saturating_add(1);
        let text = format!("durable benchmark edit {sample}");
        let mut payload =
            BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, text.clone());
        payload.content_version = content_version;
        let transaction = EditTransaction::new(
            transaction_id,
            EditTransactionKind::Typing,
            transaction_id,
            vec![EditOperation::InsertText {
                block_id,
                offset: 0,
                text,
            }],
            Vec::new(),
        );
        let started = Instant::now();
        storage
            .commit(StorageSaveBatch {
                document_id: DOCUMENT_ID,
                layout_key: None,
                payloads: vec![payload],
                index_records: Vec::new(),
                structure_version: loaded.metadata.structure_version,
                transactions: vec![transaction],
                block_attrs: Vec::new(),
                page_layout_snapshot: None,
            })
            .await?;
        save_samples.push(elapsed_ms(started.elapsed()));
    }

    eprintln!("stage=full-structure-save");
    let started = Instant::now();
    storage
        .commit(StorageSaveBatch {
            document_id: DOCUMENT_ID,
            layout_key: None,
            payloads: Vec::new(),
            index_records: loaded.records,
            structure_version: loaded.metadata.structure_version.saturating_add(1),
            transactions: Vec::new(),
            block_attrs: Vec::new(),
            page_layout_snapshot: None,
        })
        .await?;
    let structure_samples = vec![elapsed_ms(started.elapsed())];

    eprintln!("stage=checkpoint samples={}", mode.checkpoint_samples());
    let mut checkpoint_samples = Vec::with_capacity(mode.checkpoint_samples());
    for _ in 0..mode.checkpoint_samples() {
        let started = Instant::now();
        storage.create_materialized_checkpoint(DOCUMENT_ID).await?;
        checkpoint_samples.push(elapsed_ms(started.elapsed()));
    }

    eprintln!("stage=compact-and-flush");
    for entry in storage.outbox_entries(DOCUMENT_ID).await? {
        storage
            .set_outbox_state(entry.outbox_id, OutboxState::Acked, None)
            .await?;
    }
    let compact_started = Instant::now();
    let compacted_operations = storage.compact_journal(DOCUMENT_ID).await?;
    let compact_samples = vec![elapsed_ms(compact_started.elapsed())];

    let flush_started = Instant::now();
    storage.flush().await?;
    let flush_samples = vec![elapsed_ms(flush_started.elapsed())];
    let footprint_after = footprint(&database_path);

    let reopen_and_load = distribution(open_samples);
    let durable_single_block_save = distribution(save_samples);
    let full_structure_save = distribution(structure_samples);
    let materialized_checkpoint = distribution(checkpoint_samples);
    let compact_journal = distribution(compact_samples);
    let wal_flush = distribution(flush_samples);
    let mut failures = Vec::new();
    if loaded_index_blocks != mode.block_count() {
        failures.push(format!(
            "loaded {loaded_index_blocks} index blocks, expected {}",
            mode.block_count()
        ));
    }
    if loaded_initial_payloads > INITIAL_PAYLOAD_WINDOW {
        failures.push(format!(
            "initial hydration loaded {loaded_initial_payloads} payloads, limit {INITIAL_PAYLOAD_WINDOW}"
        ));
    }
    if reopen_and_load.p95_ms >= 250.0 {
        failures.push(format!(
            "reopen+load p95 {:.2}ms exceeds 250ms",
            reopen_and_load.p95_ms
        ));
    }
    if durable_single_block_save.p95_ms >= 50.0 {
        failures.push(format!(
            "durable save p95 {:.2}ms exceeds 50ms",
            durable_single_block_save.p95_ms
        ));
    }
    if compacted_operations != mode.save_samples() as u64 {
        failures.push(format!(
            "compacted {compacted_operations} operations, expected {}",
            mode.save_samples()
        ));
    }

    Ok(BenchmarkReport {
        schema_version: REPORT_SCHEMA_VERSION,
        harness_version: HARNESS_VERSION,
        mode: mode.name(),
        profile: "bench",
        target_os: std::env::consts::OS,
        target_arch: std::env::consts::ARCH,
        logical_cores: std::thread::available_parallelism().map_or(0, usize::from),
        filesystem_cache_policy: "uncontrolled OS page cache; every sample reopens SQLite",
        block_count: mode.block_count(),
        initial_payload_window: INITIAL_PAYLOAD_WINDOW,
        seed_elapsed_ms,
        footprint_before,
        reopen_and_load,
        loaded_index_blocks,
        loaded_initial_payloads,
        durable_single_block_save,
        full_structure_save,
        materialized_checkpoint,
        compact_journal,
        compacted_operations,
        wal_flush,
        footprint_after,
        passed: failures.is_empty(),
        failures,
    })
}

fn load_request() -> LoadDocumentRequest {
    LoadDocumentRequest {
        document_id: DOCUMENT_ID,
        workspace_id: 1,
        initial_payload_window_blocks: INITIAL_PAYLOAD_WINDOW,
        visible_index_version: DOCUMENT_INDEX_VISIBLE_VERSION,
        layout_key: LayoutCacheKey {
            width_bucket: 10,
            exact_width_px: 800,
            content_version: 1,
            attrs_version: 0,
            style_version: 0,
            font_version: 0,
            theme_version: 0,
            scale_factor_milli: 1_000,
        },
        page_policy_version: cditor_core::layout::PAGE_POLICY_VERSION,
    }
}

fn distribution(mut samples: Vec<f64>) -> Distribution {
    samples.sort_by(f64::total_cmp);
    Distribution {
        samples: samples.len(),
        p50_ms: percentile(&samples, 50),
        p95_ms: percentile(&samples, 95),
        max_ms: samples.last().copied().unwrap_or_default(),
    }
}

fn percentile(samples: &[f64], percentile: usize) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let rank = (samples.len() * percentile).div_ceil(100);
    samples[rank.saturating_sub(1).min(samples.len() - 1)]
}

fn elapsed_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn footprint(database: &Path) -> StorageFootprint {
    StorageFootprint {
        database_bytes: file_len(database),
        wal_bytes: file_len(&sidecar(database, "-wal")),
        shm_bytes: file_len(&sidecar(database, "-shm")),
    }
}

fn file_len(path: &Path) -> u64 {
    fs::metadata(path).map_or(0, |metadata| metadata.len())
}

fn sidecar(database: &Path, suffix: &str) -> PathBuf {
    let mut value = database.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

fn write_report(mode: &str, json: &str) -> std::io::Result<()> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("test-support crate must be under workspace/crates");
    let directory = workspace.join("target/benchmark-reports");
    fs::create_dir_all(&directory)?;
    fs::write(
        directory.join(format!("sqlite-local-storage-{mode}.json")),
        json,
    )
}
