use std::{
    fs,
    hint::black_box,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, Instant},
};

use cditor_core::rich_text::{InlineSpan, RichBlockKind, TextAlign};
use cditor_text::{
    TextAlignment, TextLayoutCachePolicy, TextLayoutCachePriority, TextLayoutCacheRequest,
    TextLayoutInput, TextLayoutMemoryPressure, TextLayoutOptions, TextLayoutSurfaceId,
    TextLineHeight, TextRelayoutStrategy, TextStyleConfig, TextTheme,
    apply_text_layout_memory_pressure, build_text_layout, cached_text_layout,
    cached_text_layout_with_request, register_font_data, set_text_layout_cache_policy,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

const FIXTURE_VERSION: &str = "text-layout-v1";
const FIXTURE_FONT_FAMILY: &str = "League Spartan";
const MEBIBYTE: usize = 1024 * 1024;
const FOCUSED_P95_BUDGET: Duration = Duration::from_millis(16);
const VISIBLE_FRAME_P95_BUDGET: Duration = Duration::from_micros(16_700);

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

    const fn focused_iterations(self) -> usize {
        match self {
            Self::Quick => 40,
            Self::Standard => 300,
            Self::Full => 1_000,
        }
    }

    const fn cold_visible_iterations(self) -> usize {
        match self {
            Self::Quick => 2,
            Self::Standard => 10,
            Self::Full => 30,
        }
    }

    const fn cached_visible_iterations(self) -> usize {
        match self {
            Self::Quick => 20,
            Self::Standard => 200,
            Self::Full => 500,
        }
    }

    const fn large_code_bytes(self) -> usize {
        match self {
            Self::Quick => 256 * 1024,
            Self::Standard => MEBIBYTE,
            Self::Full => 10 * MEBIBYTE,
        }
    }

    const fn large_code_build_iterations(self) -> usize {
        match self {
            Self::Quick => 1,
            Self::Standard => 3,
            Self::Full => 3,
        }
    }

    const fn large_code_reflow_iterations(self) -> usize {
        match self {
            Self::Quick => 2,
            Self::Standard => 8,
            Self::Full => 5,
        }
    }
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    schema_version: u32,
    fixture_version: &'static str,
    mode: &'static str,
    profile: &'static str,
    target_os: &'static str,
    target_arch: &'static str,
    font_family: &'static str,
    font_sha256: String,
    large_code_bytes: usize,
    budgets: BenchmarkBudgets,
    results: Vec<BenchmarkSummary>,
}

#[derive(Debug, Serialize)]
struct BenchmarkBudgets {
    focused_relayout_p95_us: u128,
    visible_100_cached_p95_us: u128,
}

#[derive(Debug, Serialize)]
struct BenchmarkSummary {
    name: &'static str,
    iterations: usize,
    min_us: u128,
    p50_us: u128,
    p95_us: u128,
    p99_us: u128,
    max_us: u128,
    units_per_iteration: usize,
    bytes_per_iteration: usize,
}

fn main() -> ExitCode {
    let mode = BenchmarkMode::from_args();
    let font_sha256 = register_fixture_font();
    set_text_layout_cache_policy(TextLayoutCachePolicy::new(2_048, 256 * MEBIBYTE));

    let results = vec![
        benchmark_focused_relayout(mode),
        benchmark_visible_surfaces_cold(mode),
        benchmark_visible_surfaces_cached(mode),
        benchmark_large_code_full_build(mode),
        benchmark_large_code_reflow(mode),
    ];
    let report = BenchmarkReport {
        schema_version: 1,
        fixture_version: FIXTURE_VERSION,
        mode: mode.name(),
        profile: "bench",
        target_os: std::env::consts::OS,
        target_arch: std::env::consts::ARCH,
        font_family: FIXTURE_FONT_FAMILY,
        font_sha256,
        large_code_bytes: mode.large_code_bytes(),
        budgets: BenchmarkBudgets {
            focused_relayout_p95_us: FOCUSED_P95_BUDGET.as_micros(),
            visible_100_cached_p95_us: VISIBLE_FRAME_P95_BUDGET.as_micros(),
        },
        results,
    };

    print_report(&report);
    if budget_failures(&report).is_empty() {
        ExitCode::SUCCESS
    } else {
        for failure in budget_failures(&report) {
            eprintln!("budget failure: {failure}");
        }
        ExitCode::FAILURE
    }
}

fn benchmark_focused_relayout(mode: BenchmarkMode) -> BenchmarkSummary {
    clear_unpinned_cache();
    let mut input = paragraph_input(
        10_000,
        "Focused editing keeps one immutable shaped snapshot and only recomputes line breaking \
         when its width or layout generation changes.",
    );
    let mut options = paragraph_options(420.0);
    let initial = cached_text_layout_with_request(&input, theme(), &options, editing_without_pin());
    assert!(matches!(
        initial.strategy,
        TextRelayoutStrategy::FullBuild(_)
    ));

    let mut samples = Vec::with_capacity(mode.focused_iterations());
    for iteration in 0..mode.focused_iterations() {
        input.layout_version = input.layout_version.saturating_add(1);
        options.width = Some(360.0 + (iteration % 31) as f32);
        let started = Instant::now();
        let result =
            cached_text_layout_with_request(&input, theme(), &options, editing_without_pin());
        let elapsed = started.elapsed();
        assert_eq!(result.strategy, TextRelayoutStrategy::Reflow);
        black_box(result.layout.height());
        samples.push(elapsed);
    }
    summarize("focused_relayout", samples, 1, input.plain_text().len())
}

fn benchmark_visible_surfaces_cold(mode: BenchmarkMode) -> BenchmarkSummary {
    let inputs = visible_surface_inputs();
    let options = paragraph_options(680.0);
    let mut samples = Vec::with_capacity(mode.cold_visible_iterations());
    for _ in 0..mode.cold_visible_iterations() {
        clear_unpinned_cache();
        let started = Instant::now();
        for input in &inputs {
            let result = cached_text_layout(input, theme(), &options);
            assert!(matches!(
                result.strategy,
                TextRelayoutStrategy::FullBuild(_)
            ));
            black_box(result.layout.height());
        }
        samples.push(started.elapsed());
    }
    summarize(
        "visible_100_cold_build",
        samples,
        inputs.len(),
        inputs
            .iter()
            .map(TextLayoutInput::plain_text)
            .map(|text| text.len())
            .sum(),
    )
}

fn benchmark_visible_surfaces_cached(mode: BenchmarkMode) -> BenchmarkSummary {
    clear_unpinned_cache();
    let inputs = visible_surface_inputs();
    let options = paragraph_options(680.0);
    for input in &inputs {
        cached_text_layout(input, theme(), &options);
    }

    let mut samples = Vec::with_capacity(mode.cached_visible_iterations());
    for _ in 0..mode.cached_visible_iterations() {
        let started = Instant::now();
        for input in &inputs {
            let result = cached_text_layout(input, theme(), &options);
            assert_eq!(result.strategy, TextRelayoutStrategy::CacheHit);
            black_box(result.layout.height());
        }
        samples.push(started.elapsed());
    }
    summarize(
        "visible_100_cached_frame",
        samples,
        inputs.len(),
        inputs
            .iter()
            .map(TextLayoutInput::plain_text)
            .map(|text| text.len())
            .sum(),
    )
}

fn benchmark_large_code_full_build(mode: BenchmarkMode) -> BenchmarkSummary {
    let text = large_code_fixture(mode.large_code_bytes());
    let input = code_input(20_000, &text);
    let options = code_options(840.0);
    let mut samples = Vec::with_capacity(mode.large_code_build_iterations());
    for _ in 0..mode.large_code_build_iterations() {
        let started = Instant::now();
        let layout = build_text_layout(&input, theme(), &options);
        samples.push(started.elapsed());
        black_box((layout.line_count(), layout.estimated_bytes()));
    }
    summarize("large_code_full_build", samples, 1, mode.large_code_bytes())
}

fn benchmark_large_code_reflow(mode: BenchmarkMode) -> BenchmarkSummary {
    let text = large_code_fixture(mode.large_code_bytes());
    let input = code_input(20_001, &text);
    let initial = build_text_layout(&input, theme(), &code_options(840.0));
    let mut samples = Vec::with_capacity(mode.large_code_reflow_iterations());
    for iteration in 0..mode.large_code_reflow_iterations() {
        let width = if iteration % 2 == 0 { 760.0 } else { 920.0 };
        let started = Instant::now();
        let layout = initial.reflow(Some(width), TextAlignment::Start);
        samples.push(started.elapsed());
        black_box((layout.line_count(), layout.estimated_bytes()));
    }
    summarize("large_code_reflow", samples, 1, mode.large_code_bytes())
}

fn summarize(
    name: &'static str,
    mut samples: Vec<Duration>,
    units_per_iteration: usize,
    bytes_per_iteration: usize,
) -> BenchmarkSummary {
    assert!(!samples.is_empty());
    samples.sort_unstable();
    BenchmarkSummary {
        name,
        iterations: samples.len(),
        min_us: samples[0].as_micros(),
        p50_us: percentile(&samples, 50).as_micros(),
        p95_us: percentile(&samples, 95).as_micros(),
        p99_us: percentile(&samples, 99).as_micros(),
        max_us: samples[samples.len() - 1].as_micros(),
        units_per_iteration,
        bytes_per_iteration,
    }
}

fn percentile(samples: &[Duration], percentile: usize) -> Duration {
    let rank = samples
        .len()
        .saturating_mul(percentile)
        .div_ceil(100)
        .saturating_sub(1)
        .min(samples.len() - 1);
    samples[rank]
}

fn budget_failures(report: &BenchmarkReport) -> Vec<String> {
    let mut failures = Vec::new();
    for result in &report.results {
        let budget = match result.name {
            "focused_relayout" => Some(report.budgets.focused_relayout_p95_us),
            "visible_100_cached_frame" => Some(report.budgets.visible_100_cached_p95_us),
            _ => None,
        };
        if let Some(budget) = budget
            && result.p95_us > budget
        {
            failures.push(format!(
                "{} p95 {}us exceeds {}us",
                result.name, result.p95_us, budget
            ));
        }
    }
    failures
}

fn print_report(report: &BenchmarkReport) {
    eprintln!(
        "cditor-text benchmark ({}, {}, {} {})",
        report.mode, report.profile, report.target_os, report.target_arch
    );
    eprintln!(
        "{:<30} {:>8} {:>10} {:>10} {:>10} {:>10}",
        "scenario", "samples", "p50(us)", "p95(us)", "p99(us)", "max(us)"
    );
    for result in &report.results {
        eprintln!(
            "{:<30} {:>8} {:>10} {:>10} {:>10} {:>10}",
            result.name,
            result.iterations,
            result.p50_us,
            result.p95_us,
            result.p99_us,
            result.max_us
        );
    }
    println!("{}", serde_json::to_string_pretty(report).unwrap());
}

fn paragraph_input(block_id: u64, text: &str) -> TextLayoutInput {
    text_input(block_id, RichBlockKind::Paragraph, text, 420.0)
}

fn code_input(block_id: u64, text: &str) -> TextLayoutInput {
    text_input(
        block_id,
        RichBlockKind::Code {
            language: Some("rust".to_owned()),
        },
        text,
        840.0,
    )
}

fn text_input(block_id: u64, kind: RichBlockKind, text: &str, width_px: f64) -> TextLayoutInput {
    TextLayoutInput {
        surface_id: TextLayoutSurfaceId::Block(block_id),
        content_version: 1,
        layout_version: 1,
        kind,
        text_align: TextAlign::Start,
        spans: vec![InlineSpan::plain(text)],
        width_px,
        theme_version: 1,
        font_version: 1,
    }
}

fn paragraph_options(width: f32) -> TextLayoutOptions {
    layout_options(width, 16.0, 24.0)
}

fn code_options(width: f32) -> TextLayoutOptions {
    layout_options(width, 14.0, 24.0)
}

fn layout_options(width: f32, font_size: f32, line_height: f32) -> TextLayoutOptions {
    TextLayoutOptions {
        width: Some(width),
        quantize: true,
        base_text_color: 0x37352f,
        mono_font_family: FIXTURE_FONT_FAMILY.to_owned(),
        base_style: TextStyleConfig {
            font_family: FIXTURE_FONT_FAMILY.to_owned(),
            font_size,
            font_weight: 100.0,
            font_variations: "'wght' 450".to_owned(),
            line_height: TextLineHeight::Absolute(line_height),
            ..TextStyleConfig::default()
        },
        ..TextLayoutOptions::default()
    }
}

fn visible_surface_inputs() -> Vec<TextLayoutInput> {
    (0..100)
        .map(|index| {
            let text = format!(
                "Visible surface {index}: Parley shapes text, preserves geometry, and reuses the \
                 immutable snapshot across frames."
            );
            paragraph_input(index + 1, &text)
        })
        .collect()
}

fn large_code_fixture(target_bytes: usize) -> String {
    const LINE: &str =
        "fn layout_row(index: usize) -> usize { index.wrapping_mul(31) } // cditor benchmark\n";
    let mut text = String::with_capacity(target_bytes);
    while text.len() + LINE.len() <= target_bytes {
        text.push_str(LINE);
    }
    let remaining = target_bytes.saturating_sub(text.len());
    text.extend(std::iter::repeat_n(' ', remaining));
    assert_eq!(text.len(), target_bytes);
    text
}

fn editing_without_pin() -> TextLayoutCacheRequest {
    TextLayoutCacheRequest {
        priority: TextLayoutCachePriority::Editing,
        pin_surface: false,
    }
}

fn clear_unpinned_cache() {
    let report = apply_text_layout_memory_pressure(TextLayoutMemoryPressure::Critical);
    assert_eq!(report.remaining_entries, 0);
}

fn theme() -> TextTheme {
    TextTheme {
        link_text: 0x2383e2,
        document_link_text: 0x9065b0,
        inline_code_text: 0xeb5757,
        inline_code_background: 0xf1f1ef,
    }
}

fn register_fixture_font() -> String {
    let data = fs::read(fixture_font_path()).unwrap();
    let sha256 = format!("{:x}", Sha256::digest(&data));
    let families = register_font_data(data).unwrap();
    assert!(
        families
            .iter()
            .any(|family| family.name == FIXTURE_FONT_FAMILY)
    );
    sha256
}

fn fixture_font_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/text-layout/v1/fonts/LeagueSpartan[wght].ttf")
}
