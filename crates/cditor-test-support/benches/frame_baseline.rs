//! Frame benchmark 基线（P0-008）。
//!
//! 在 bench profile 下驱动 Runtime 的 headless acceptance 场景（open/scroll/
//! editing/structure edit），并记录 P0-007 版本化 fixture 的 manifest，输出
//! 含 OS、架构、逻辑核数、profile、fixture version/checksum 的 versioned JSON
//! 报告。任一场景未通过其内置预算时以非零码退出。
//!
//! 运行：`cargo bench -p cditor-test-support --bench frame_baseline -- [--quick|--full]`
//! 报告：stdout JSON + `target/benchmark-reports/frame-baseline-<mode>.json`

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use cditor_core::demo_fixtures::large_mixed_rich_text_document;
use cditor_core::fixtures::{
    FIXTURE_MANIFEST_SCHEMA_VERSION, MIXED_FIXTURE_VERSION,
    bidi::{BIDI_FIXTURE_VERSION, BIDI_STRESS_BLOCKS, bidi_stress_document},
    code::{CODE_FIXTURE_VERSION, LARGE_CODE_FULL_TARGET_BYTES, large_code_document},
    fixture_manifest,
    table::{
        TABLE_FIXTURE_VERSION, TALL_TABLE_FULL_ROWS, WIDE_TABLE_FULL_COLUMNS, tall_table_document,
        wide_table_document,
    },
};
use cditor_core::rich_text::RichTextDocument;
use cditor_test_support::acceptance::{
    editing::{EditingAcceptanceConfig, EditingAcceptanceScenario, run_editing_acceptance},
    mixed::{MixedAcceptanceConfig, run_mixed_acceptance},
    open::{
        AcceptanceFixture, OpenAcceptanceConfig, fixture_10mb_code_block, fixture_50k_row_table,
        fixture_100k_one_line_blocks, fixture_100k_uneven_heights, fixture_emoji_cjk_bidi,
        fixture_image_dense, run_open_acceptance,
    },
    scroll::{ScrollAcceptanceConfig, ScrollAcceptanceScenario, run_scroll_acceptance},
    structure_edit::{
        StructureEditAcceptanceConfig, StructureEditScenario, run_structure_edit_acceptance,
    },
};
use serde::Serialize;

const HARNESS_VERSION: &str = "frame-baseline-v1";
const REPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BenchmarkMode {
    Quick,
    Standard,
    Full,
}

impl BenchmarkMode {
    fn from_args() -> Self {
        let args = std::env::args().collect::<Vec<_>>();
        if args.iter().any(|argument| argument == "--quick") {
            Self::Quick
        } else if args.iter().any(|argument| argument == "--full") {
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

    const fn open_iterations(self) -> usize {
        match self {
            Self::Quick => 2,
            Self::Standard => 5,
            Self::Full => 12,
        }
    }

    const fn scenario_runs(self) -> usize {
        match self {
            Self::Quick => 1,
            Self::Standard => 2,
            Self::Full => 3,
        }
    }

    const fn mixed_manifest_blocks(self) -> usize {
        match self {
            Self::Quick => 4_096,
            Self::Standard => 20_000,
            Self::Full => 100_000,
        }
    }

    const fn bidi_manifest_blocks(self) -> usize {
        match self {
            Self::Quick => 512,
            Self::Standard | Self::Full => BIDI_STRESS_BLOCKS,
        }
    }

    const fn code_manifest_bytes(self) -> usize {
        match self {
            Self::Quick => 128 * 1024,
            Self::Standard => 1024 * 1024,
            Self::Full => LARGE_CODE_FULL_TARGET_BYTES,
        }
    }

    const fn tall_table_rows(self) -> usize {
        match self {
            Self::Quick => 2_048,
            Self::Standard => 8_192,
            Self::Full => TALL_TABLE_FULL_ROWS,
        }
    }

    const fn wide_table_columns(self) -> usize {
        match self {
            Self::Quick => 96,
            Self::Standard => 200,
            Self::Full => WIDE_TABLE_FULL_COLUMNS,
        }
    }
}

#[derive(Debug, Serialize)]
struct FrameBenchmarkReport {
    schema_version: u32,
    harness_version: &'static str,
    manifest_schema_version: u32,
    mode: &'static str,
    profile: &'static str,
    target_os: &'static str,
    target_arch: &'static str,
    logical_cores: usize,
    fixture_manifests: Vec<FixtureManifestReport>,
    open: Vec<OpenSummary>,
    scroll: Vec<ScrollSummary>,
    editing: Vec<EditingSummary>,
    structure: Vec<StructureSummary>,
    mixed: MixedSummary,
}

#[derive(Debug, Serialize)]
struct FixtureManifestReport {
    name: &'static str,
    version: u32,
    block_count: usize,
    semantic_checksum: u64,
}

#[derive(Debug, Serialize)]
struct OpenSummary {
    fixture: String,
    iterations: usize,
    total_blocks: usize,
    first_screen_ms_min: f64,
    first_screen_ms_p50: f64,
    first_screen_ms_p95: f64,
    first_screen_ms_max: f64,
    passed: bool,
    failures: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ScrollSummary {
    scenario: String,
    runs: usize,
    frame_count: usize,
    worst_p99_frame_ms: f64,
    worst_anchor_jitter_p95: f64,
    passed: bool,
    failures: Vec<String>,
}

#[derive(Debug, Serialize)]
struct EditingSummary {
    scenario: String,
    runs: usize,
    input_count: usize,
    worst_latency_p95_ms: f64,
    worst_latency_p99_ms: f64,
    passed: bool,
    failures: Vec<String>,
}

#[derive(Debug, Serialize)]
struct StructureSummary {
    scenario: String,
    runs: usize,
    affected_blocks: usize,
    worst_ui_blocking_ms: f64,
    rebuild_passes: usize,
    passed: bool,
    failures: Vec<String>,
}

#[derive(Debug, Serialize)]
struct MixedSummary {
    fixture: &'static str,
    runs: usize,
    total_blocks: usize,
    iterations: usize,
    scroll_operations: usize,
    jump_operations: usize,
    edit_operations: usize,
    drag_operations: usize,
    worst_frame_p95_ms: f64,
    worst_frame_max_ms: f64,
    peak_rendered_blocks: usize,
    peak_resident_payloads: usize,
    peak_resident_memory_bytes: usize,
    passed: bool,
    failures: Vec<String>,
}

fn main() -> ExitCode {
    let mode = BenchmarkMode::from_args();

    let report = FrameBenchmarkReport {
        schema_version: REPORT_SCHEMA_VERSION,
        harness_version: HARNESS_VERSION,
        manifest_schema_version: FIXTURE_MANIFEST_SCHEMA_VERSION,
        mode: mode.name(),
        profile: "bench",
        target_os: std::env::consts::OS,
        target_arch: std::env::consts::ARCH,
        logical_cores: std::thread::available_parallelism().map_or(0, usize::from),
        fixture_manifests: fixture_manifest_reports(mode),
        open: run_open_benchmarks(mode),
        scroll: run_scroll_benchmarks(mode),
        editing: run_editing_benchmarks(mode),
        structure: run_structure_benchmarks(mode),
        mixed: run_mixed_benchmark(mode),
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("report serializes")
    );
    persist_report(&report);

    let failures = collect_failures(&report);
    if failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        for failure in failures {
            eprintln!("frame baseline failure: {failure}");
        }
        ExitCode::FAILURE
    }
}

fn fixture_manifest_reports(mode: BenchmarkMode) -> Vec<FixtureManifestReport> {
    let manifests = [
        (
            "mixed",
            MIXED_FIXTURE_VERSION,
            large_mixed_rich_text_document(1, mode.mixed_manifest_blocks()),
        ),
        (
            "bidi-stress",
            BIDI_FIXTURE_VERSION,
            bidi_stress_document(2, mode.bidi_manifest_blocks()),
        ),
        (
            "large-code",
            CODE_FIXTURE_VERSION,
            large_code_document(3, mode.code_manifest_bytes()),
        ),
        (
            "tall-table",
            TABLE_FIXTURE_VERSION,
            tall_table_document(4, mode.tall_table_rows()),
        ),
        (
            "wide-table",
            TABLE_FIXTURE_VERSION,
            wide_table_document(5, mode.wide_table_columns()),
        ),
    ];
    manifests
        .into_iter()
        .map(|(name, version, document)| manifest_report(name, version, &document))
        .collect()
}

fn manifest_report(
    name: &'static str,
    version: u32,
    document: &RichTextDocument,
) -> FixtureManifestReport {
    let manifest = fixture_manifest(name, version, document);
    FixtureManifestReport {
        name,
        version: manifest.version,
        block_count: manifest.block_count,
        semantic_checksum: manifest.semantic_checksum,
    }
}

type FixtureBuilder = fn(u64) -> AcceptanceFixture;

fn run_open_benchmarks(mode: BenchmarkMode) -> Vec<OpenSummary> {
    let fixtures: Vec<(&'static str, FixtureBuilder)> = vec![
        ("100k-one-line", fixture_100k_one_line_blocks),
        ("100k-uneven-heights", fixture_100k_uneven_heights),
        ("image-dense", fixture_image_dense),
        ("10mb-code-block", fixture_10mb_code_block),
        ("50k-row-table", fixture_50k_row_table),
        ("emoji-cjk-bidi", fixture_emoji_cjk_bidi),
    ];

    fixtures
        .into_iter()
        .map(|(name, build)| {
            let fixture = build(1);
            let mut samples = Vec::with_capacity(mode.open_iterations());
            let mut failures = Vec::new();
            let mut total_blocks = 0;
            for _ in 0..mode.open_iterations() {
                match run_open_acceptance(&fixture, OpenAcceptanceConfig::default()) {
                    Ok(result) => {
                        total_blocks = result.total_blocks;
                        samples.push(result.first_screen_time_ms);
                        if !result.passed() {
                            failures.push(format!(
                                "open {name}: first_screen={:.2}ms acceptable={} shapes_bounded={}",
                                result.first_screen_time_ms,
                                result.acceptable_passed,
                                result.shape_count_bounded
                            ));
                        }
                    }
                    Err(error) => failures.push(format!("open {name}: {error}")),
                }
            }
            samples.sort_by(f64::total_cmp);
            OpenSummary {
                fixture: name.to_owned(),
                iterations: samples.len(),
                total_blocks,
                first_screen_ms_min: samples.first().copied().unwrap_or(f64::NAN),
                first_screen_ms_p50: percentile(&samples, 50),
                first_screen_ms_p95: percentile(&samples, 95),
                first_screen_ms_max: samples.last().copied().unwrap_or(f64::NAN),
                passed: failures.is_empty(),
                failures,
            }
        })
        .collect()
}

fn run_scroll_benchmarks(mode: BenchmarkMode) -> Vec<ScrollSummary> {
    let scenarios = [
        ScrollAcceptanceScenario::TopToMiddle,
        ScrollAcceptanceScenario::MiddleToTop,
        ScrollAcceptanceScenario::TenMinuteContinuousScroll,
        ScrollAcceptanceScenario::RandomHeightCorrectionWhileScrolling,
        ScrollAcceptanceScenario::WindowLoadDelayWhileScrolling,
        ScrollAcceptanceScenario::ScrollbarDragWithHeightCorrections,
    ];

    scenarios
        .into_iter()
        .map(|scenario| {
            let mut worst_p99 = 0.0f64;
            let mut worst_jitter = 0.0f64;
            let mut frame_count = 0;
            let mut failures = Vec::new();
            for _ in 0..mode.scenario_runs() {
                let result = run_scroll_acceptance(scenario, ScrollAcceptanceConfig::default());
                worst_p99 = worst_p99.max(result.p99_frame_ms);
                worst_jitter = worst_jitter.max(result.anchor_jitter_p95);
                frame_count = result.frame_count;
                if !result.passed() {
                    failures.extend(result.failures);
                }
            }
            ScrollSummary {
                scenario: format!("{scenario:?}"),
                runs: mode.scenario_runs(),
                frame_count,
                worst_p99_frame_ms: worst_p99,
                worst_anchor_jitter_p95: worst_jitter,
                passed: failures.is_empty(),
                failures,
            }
        })
        .collect()
}

fn run_editing_benchmarks(mode: BenchmarkMode) -> Vec<EditingSummary> {
    let scenarios = [
        EditingAcceptanceScenario::ContinuousInput1000Chars,
        EditingAcceptanceScenario::InputCausesMultipleLineWraps,
        EditingAcceptanceScenario::ImeComposition,
        EditingAcceptanceScenario::TypingWhileScrolling,
        EditingAcceptanceScenario::TypingWhileResize,
    ];

    scenarios
        .into_iter()
        .map(|scenario| {
            let mut worst_p95 = 0.0f64;
            let mut worst_p99 = 0.0f64;
            let mut input_count = 0;
            let mut failures = Vec::new();
            for _ in 0..mode.scenario_runs() {
                let result = run_editing_acceptance(scenario, EditingAcceptanceConfig::default());
                worst_p95 = worst_p95.max(result.latency_p95_ms);
                worst_p99 = worst_p99.max(result.latency_p99_ms);
                input_count = result.input_count;
                if !result.passed() {
                    failures.extend(result.failures);
                }
            }
            EditingSummary {
                scenario: format!("{scenario:?}"),
                runs: mode.scenario_runs(),
                input_count,
                worst_latency_p95_ms: worst_p95,
                worst_latency_p99_ms: worst_p99,
                passed: failures.is_empty(),
                failures,
            }
        })
        .collect()
}

fn run_structure_benchmarks(mode: BenchmarkMode) -> Vec<StructureSummary> {
    let scenarios = [
        StructureEditScenario::Paste10kBlocks,
        StructureEditScenario::Delete50kBlocks,
        StructureEditScenario::UndoLargeDelete,
        StructureEditScenario::Move10kSubtree,
        StructureEditScenario::CollapseExpand10kSubtree,
    ];

    scenarios
        .into_iter()
        .map(|scenario| {
            let mut worst_blocking = 0.0f64;
            let mut affected_blocks = 0;
            let mut rebuild_passes = 0;
            let mut failures = Vec::new();
            for _ in 0..mode.scenario_runs() {
                let result = run_structure_edit_acceptance(
                    scenario,
                    StructureEditAcceptanceConfig::default(),
                );
                worst_blocking = worst_blocking.max(result.ui_blocking_ms);
                affected_blocks = result.affected_blocks;
                rebuild_passes = result.rebuild_passes;
                if !result.passed() {
                    failures.extend(result.failures);
                }
            }
            StructureSummary {
                scenario: format!("{scenario:?}"),
                runs: mode.scenario_runs(),
                affected_blocks,
                worst_ui_blocking_ms: worst_blocking,
                rebuild_passes,
                passed: failures.is_empty(),
                failures,
            }
        })
        .collect()
}

fn run_mixed_benchmark(mode: BenchmarkMode) -> MixedSummary {
    let fixture = fixture_100k_uneven_heights(9);
    let iterations = match mode {
        BenchmarkMode::Quick => 32,
        BenchmarkMode::Standard => 128,
        BenchmarkMode::Full => 512,
    };
    let mut summary = MixedSummary {
        fixture: "100k-uneven-heights",
        runs: mode.scenario_runs(),
        total_blocks: fixture.records.len(),
        iterations,
        scroll_operations: 0,
        jump_operations: 0,
        edit_operations: 0,
        drag_operations: 0,
        worst_frame_p95_ms: 0.0,
        worst_frame_max_ms: 0.0,
        peak_rendered_blocks: 0,
        peak_resident_payloads: 0,
        peak_resident_memory_bytes: 0,
        passed: true,
        failures: Vec::new(),
    };
    for _ in 0..mode.scenario_runs() {
        match run_mixed_acceptance(
            &fixture,
            MixedAcceptanceConfig {
                iterations,
                ..MixedAcceptanceConfig::default()
            },
        ) {
            Ok(result) => {
                summary.scroll_operations = result.scroll_operations;
                summary.jump_operations = result.jump_operations;
                summary.edit_operations = result.edit_operations;
                summary.drag_operations = result.drag_operations;
                summary.worst_frame_p95_ms = summary.worst_frame_p95_ms.max(result.frame_p95_ms);
                summary.worst_frame_max_ms = summary.worst_frame_max_ms.max(result.frame_max_ms);
                summary.peak_rendered_blocks = summary
                    .peak_rendered_blocks
                    .max(result.peak_rendered_blocks);
                summary.peak_resident_payloads = summary
                    .peak_resident_payloads
                    .max(result.peak_resident_payloads);
                summary.peak_resident_memory_bytes = summary
                    .peak_resident_memory_bytes
                    .max(result.peak_resident_memory_bytes);
                summary.failures.extend(result.failures);
            }
            Err(error) => summary.failures.push(error),
        }
    }
    summary.passed = summary.failures.is_empty();
    summary
}

fn percentile(sorted_samples: &[f64], percentile: usize) -> f64 {
    if sorted_samples.is_empty() {
        return f64::NAN;
    }
    let rank = (sorted_samples.len() * percentile).div_ceil(100);
    sorted_samples[rank.saturating_sub(1).min(sorted_samples.len() - 1)]
}

fn persist_report(report: &FrameBenchmarkReport) {
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/benchmark-reports");
    if let Err(error) = fs::create_dir_all(&directory) {
        eprintln!("cannot create report directory: {error}");
        return;
    }
    let path = directory.join(format!("frame-baseline-{}.json", report.mode));
    match serde_json::to_vec_pretty(report) {
        Ok(bytes) => {
            if let Err(error) = fs::write(&path, bytes) {
                eprintln!("cannot write report {}: {error}", path.display());
            } else {
                eprintln!("report written to {}", path.display());
            }
        }
        Err(error) => eprintln!("cannot serialize report: {error}"),
    }
}

fn collect_failures(report: &FrameBenchmarkReport) -> Vec<String> {
    let mut failures = Vec::new();
    failures.extend(report.open.iter().flat_map(|entry| entry.failures.clone()));
    failures.extend(
        report
            .scroll
            .iter()
            .flat_map(|entry| entry.failures.clone()),
    );
    failures.extend(
        report
            .editing
            .iter()
            .flat_map(|entry| entry.failures.clone()),
    );
    failures.extend(
        report
            .structure
            .iter()
            .flat_map(|entry| entry.failures.clone()),
    );
    failures.extend(report.mixed.failures.clone());
    failures
}
