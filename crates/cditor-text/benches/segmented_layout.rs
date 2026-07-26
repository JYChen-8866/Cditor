//! P6-015 分段布局基准：10MiB code surface 的窗口化 layout 预算。
//!
//! 证明大文本不再整块同步 layout：索引扫描、冷窗口测量、滚动步进、单段
//! 编辑重测与宽度 reflow 均在帧预算内。任一预算失败以非零码退出。
//!
//! 运行：`cargo bench -p cditor-text --bench segmented_layout -- [--quick|--full]`

use std::hint::black_box;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use cditor_core::fixtures::code::large_code_source;
use cditor_core::rich_text::{InlineSpan, RichBlockKind, TextAlign};
use cditor_text::{
    SegmentedLayoutConfig, SegmentedTextLayout, TextLayoutInput, TextLayoutOptions,
    TextLayoutSnapshot, TextLayoutSurfaceId, TextLineHeight, TextStyleConfig, TextTheme,
    build_text_layout,
};
use serde::Serialize;

const HARNESS_VERSION: &str = "segmented-layout-v1";
const INDEX_BUDGET: Duration = Duration::from_millis(50);
const COLD_WINDOW_BUDGET: Duration = Duration::from_millis(100);
const SCROLL_STEP_BUDGET: Duration = Duration::from_micros(16_700);
const EDIT_REMEASURE_BUDGET: Duration = Duration::from_millis(16);
const VIEWPORT_PX: f32 = 900.0;
const CODE_WIDTH: f32 = 760.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Quick,
    Full,
}

impl Mode {
    fn from_args() -> Self {
        if std::env::args().any(|argument| argument == "--quick") {
            Self::Quick
        } else {
            Self::Full
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Full => "full",
        }
    }

    const fn corpus_bytes(self) -> usize {
        match self {
            Self::Quick => 1024 * 1024,
            Self::Full => 10 * 1024 * 1024,
        }
    }

    const fn iterations(self) -> usize {
        match self {
            Self::Quick => 8,
            Self::Full => 40,
        }
    }
}

#[derive(Debug, Serialize)]
struct Report {
    schema_version: u32,
    harness_version: &'static str,
    mode: &'static str,
    profile: &'static str,
    target_os: &'static str,
    target_arch: &'static str,
    corpus_bytes: usize,
    segment_count: usize,
    results: Vec<Summary>,
}

#[derive(Debug, Serialize)]
struct Summary {
    name: &'static str,
    iterations: usize,
    p50_us: u128,
    p95_us: u128,
    max_us: u128,
    budget_us: u128,
    passed: bool,
}

fn main() -> ExitCode {
    let mode = Mode::from_args();
    let corpus = large_code_source(mode.corpus_bytes());

    let mut results = Vec::new();
    let mut segment_count = 0usize;

    // 1) 索引扫描：不做 shaping 的 O(n) 分段。
    {
        let mut samples = Vec::new();
        for _ in 0..mode.iterations().min(12) {
            let started = Instant::now();
            let layout = SegmentedTextLayout::new(corpus.clone(), config());
            samples.push(started.elapsed());
            segment_count = layout.segment_count();
            black_box(layout.total_height());
        }
        results.push(summarize("index_scan", samples, INDEX_BUDGET));
    }

    let mut layout = SegmentedTextLayout::new(corpus.clone(), config());
    layout.set_width(Some(CODE_WIDTH));
    let total_estimate = layout.total_height();

    // 2) 冷窗口测量：随机跳转位置，测量整个可见窗口。
    {
        let mut samples = Vec::new();
        let mut rng = Lcg(0x5eed);
        for _ in 0..mode.iterations() {
            let top =
                (rng.next(10_000) as f32 / 10_000.0) * (total_estimate - VIEWPORT_PX).max(0.0);
            let mut fresh = layout.clone();
            let window = fresh.visible_segments(top, VIEWPORT_PX);
            let started = Instant::now();
            fresh.measure_segments(window, |slice, _| build_slice(slice));
            samples.push(started.elapsed());
        }
        results.push(summarize(
            "cold_window_measure",
            samples,
            COLD_WINDOW_BUDGET,
        ));
    }

    // 3) 滚动步进：视口推进一屏，只测新暴露的段。
    {
        let mut samples = Vec::new();
        let mut top = 0.0f32;
        let mut scroll_layout = layout.clone();
        for _ in 0..mode.iterations() {
            top = (top + VIEWPORT_PX).min((total_estimate - VIEWPORT_PX).max(0.0));
            let window = scroll_layout.visible_segments(top, VIEWPORT_PX);
            let started = Instant::now();
            scroll_layout.measure_segments(window, |slice, _| build_slice(slice));
            samples.push(started.elapsed());
        }
        results.push(summarize(
            "scroll_step_measure",
            samples,
            SCROLL_STEP_BUDGET,
        ));
    }

    // 4) 单段编辑重测：可见窗口内插入一行并补测失效段。
    {
        let mut samples = Vec::new();
        let mut edit_layout = layout.clone();
        let window = edit_layout.visible_segments(0.0, VIEWPORT_PX);
        edit_layout.measure_segments(window, |slice, _| build_slice(slice));
        for iteration in 0..mode.iterations() {
            let target = edit_layout
                .segment_byte_range(iteration % edit_layout.segment_count().min(4))
                .expect("segment exists");
            let insert_at = target.start;
            let started = Instant::now();
            edit_layout.replace_range(insert_at..insert_at, "let edited = 1;\n");
            let pending: Vec<usize> = (0..edit_layout.segment_count())
                .filter(|index| {
                    !edit_layout.is_measured(*index)
                        && edit_layout
                            .segment_byte_range(*index)
                            .is_some_and(|range| range.start <= insert_at + 64)
                })
                .collect();
            for index in pending {
                edit_layout.measure_segments(index..index + 1, |slice, _| build_slice(slice));
            }
            samples.push(started.elapsed());
        }
        results.push(summarize("edit_remeasure", samples, EDIT_REMEASURE_BUDGET));
    }

    // 5) 宽度 reflow：只重新测量可见窗口。
    {
        let mut samples = Vec::new();
        let mut reflow_layout = layout.clone();
        for iteration in 0..mode.iterations() {
            let width = CODE_WIDTH + (iteration % 7) as f32 * 24.0 + 1.0;
            let started = Instant::now();
            reflow_layout.set_width(Some(width));
            let window = reflow_layout.visible_segments(0.0, VIEWPORT_PX);
            reflow_layout.measure_segments(window, |slice, _| build_slice(slice));
            samples.push(started.elapsed());
        }
        results.push(summarize(
            "width_reflow_window",
            samples,
            COLD_WINDOW_BUDGET,
        ));
    }

    let report = Report {
        schema_version: 1,
        harness_version: HARNESS_VERSION,
        mode: mode.name(),
        profile: "bench",
        target_os: std::env::consts::OS,
        target_arch: std::env::consts::ARCH,
        corpus_bytes: corpus.len(),
        segment_count,
        results,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("report serializes")
    );

    if report.results.iter().all(|summary| summary.passed) {
        ExitCode::SUCCESS
    } else {
        for summary in report.results.iter().filter(|summary| !summary.passed) {
            eprintln!(
                "budget failure: {} p95 {}us > {}us",
                summary.name, summary.p95_us, summary.budget_us
            );
        }
        ExitCode::FAILURE
    }
}

fn config() -> SegmentedLayoutConfig {
    SegmentedLayoutConfig::default()
}

fn build_slice(text: &str) -> TextLayoutSnapshot {
    let input = TextLayoutInput {
        surface_id: TextLayoutSurfaceId::Block(61_500),
        content_version: 1,
        layout_version: 1,
        kind: RichBlockKind::Code {
            language: Some("rust".to_owned()),
        },
        text_align: TextAlign::Start,
        spans: vec![InlineSpan::plain(text)],
        width_px: f64::from(CODE_WIDTH),
        theme_version: 1,
        font_version: 1,
    };
    build_text_layout(
        &input,
        TextTheme {
            link_text: 0x2383e2,
            inline_code_text: 0xeb5757,
            inline_code_background: 0xf1f1ef,
        },
        &TextLayoutOptions {
            width: Some(CODE_WIDTH),
            display_scale: 1.0,
            quantize: true,
            base_style: TextStyleConfig {
                font_size: 14.0,
                line_height: TextLineHeight::Absolute(22.0),
                ..TextStyleConfig::default()
            },
            ..TextLayoutOptions::default()
        },
    )
}

fn summarize(name: &'static str, mut samples: Vec<Duration>, budget: Duration) -> Summary {
    samples.sort();
    let percentile = |percent: usize| -> Duration {
        if samples.is_empty() {
            return Duration::ZERO;
        }
        let rank = (samples.len() * percent).div_ceil(100);
        samples[rank.saturating_sub(1).min(samples.len() - 1)]
    };
    let p95 = percentile(95);
    Summary {
        name,
        iterations: samples.len(),
        p50_us: percentile(50).as_micros(),
        p95_us: p95.as_micros(),
        max_us: samples.last().copied().unwrap_or_default().as_micros(),
        budget_us: budget.as_micros(),
        passed: p95 <= budget,
    }
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % bound.max(1)
    }
}
