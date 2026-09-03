use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use web_time::Instant;

use gpui::{FrameTiming, FrameTimingCollector, WindowId};

use super::frame_telemetry::AppFrameTelemetryInput;

const COLLECT_INTERVAL: Duration = Duration::from_secs(1);
const MIN_REPORT_SPAN: Duration = Duration::from_millis(900);
const INACTIVE_FRAME_GAP: Duration = Duration::from_millis(400);
const FRAME_BUDGET_60_HZ: Duration = Duration::from_micros(16_667);
const TWO_FRAME_BUDGETS_60_HZ: Duration = Duration::from_micros(33_334);
const EDITOR_RENDER_SAMPLE_CAPACITY: usize = 512;

#[derive(Debug, Clone)]
struct TraceContext {
    interaction: String,
    document_blocks: usize,
    payload_start: usize,
    payload_end: usize,
    page_start: usize,
    page_end: usize,
    rendered_blocks: usize,
    loaded_payloads: usize,
    block_layouts: usize,
    table_cell_layouts: usize,
    auxiliary_layouts: usize,
    pending_layout_tasks: usize,
    pending_payload_loads: usize,
    pending_saves: usize,
    realtime_lane_depth: Option<usize>,
    interactive_lane_depth: Option<usize>,
    visible_lane_depth: Option<usize>,
    prefetch_lane_depth: Option<usize>,
    background_lane_depth: Option<usize>,
    payload_and_undo_bytes: usize,
    platform_layout_bytes: usize,
    image_cache_entries: usize,
    image_resident_decoded_bytes: usize,
    mermaid_cache_entries: usize,
    mermaid_resident_image_bytes: usize,
    mermaid_reserved_render_bytes: usize,
    video_resident_cpu_frame_bytes: usize,
    video_resident_render_image_bytes: usize,
    image_cache_over_budget: bool,
    mermaid_cache_over_budget: bool,
    payload_cache_over_budget: bool,
    platform_layout_cache_over_budget: bool,
    text_geometry_fallback_rate: f64,
}

impl From<&AppFrameTelemetryInput> for TraceContext {
    fn from(input: &AppFrameTelemetryInput) -> Self {
        Self {
            interaction: input.interaction.clone(),
            document_blocks: input.window.document_blocks,
            payload_start: input.window.payload_start,
            payload_end: input.window.payload_end,
            page_start: input.window.page_start,
            page_end: input.window.page_end,
            rendered_blocks: input.entities.rendered_blocks,
            loaded_payloads: input.entities.loaded_payloads,
            block_layouts: input.entities.block_layouts,
            table_cell_layouts: input.entities.table_cell_layouts,
            auxiliary_layouts: input.entities.auxiliary_layouts,
            pending_layout_tasks: input.queues.pending_layout_tasks,
            pending_payload_loads: input.queues.pending_payload_loads,
            pending_saves: input.queues.pending_saves,
            realtime_lane_depth: input.queues.realtime_lane_depth,
            interactive_lane_depth: input.queues.interactive_lane_depth,
            visible_lane_depth: input.queues.visible_lane_depth,
            prefetch_lane_depth: input.queues.prefetch_lane_depth,
            background_lane_depth: input.queues.background_lane_depth,
            payload_and_undo_bytes: input.caches.payload_and_undo_bytes,
            platform_layout_bytes: input.caches.platform_layout_bytes,
            image_cache_entries: input.caches.image_cache_entries,
            image_resident_decoded_bytes: input.caches.image_resident_decoded_bytes,
            mermaid_cache_entries: input.caches.mermaid_cache_entries,
            mermaid_resident_image_bytes: input.caches.mermaid_resident_image_bytes,
            mermaid_reserved_render_bytes: input.caches.mermaid_reserved_render_bytes,
            video_resident_cpu_frame_bytes: input.caches.video_resident_cpu_frame_bytes,
            video_resident_render_image_bytes: input.caches.video_resident_render_image_bytes,
            image_cache_over_budget: input.caches.image_cache_over_budget,
            mermaid_cache_over_budget: input.caches.mermaid_cache_over_budget,
            payload_cache_over_budget: input.caches.payload_cache_over_budget,
            platform_layout_cache_over_budget: input.caches.platform_layout_cache_over_budget,
            text_geometry_fallback_rate: input.text_geometry_fallback_rate,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DrawSample {
    interval: Duration,
    draw: Duration,
    dirty_to_draw: Option<Duration>,
    clean_gap: Option<Duration>,
    invalidations: u64,
}

#[derive(Debug, Default)]
struct ActiveWindow {
    span: Duration,
    intervals_ms: Vec<f64>,
    draws_ms: Vec<f64>,
    dirty_to_draw_ms: Vec<f64>,
    clean_gaps_ms: Vec<f64>,
    invalidations: Vec<u64>,
    interval_over_budget: usize,
    interval_over_two_budgets: usize,
    draw_over_budget: usize,
}

impl ActiveWindow {
    fn record(&mut self, sample: DrawSample) {
        if sample.interval > INACTIVE_FRAME_GAP {
            self.clear();
            return;
        }

        self.span += sample.interval;
        self.intervals_ms.push(duration_ms(sample.interval));
        self.draws_ms.push(duration_ms(sample.draw));
        if let Some(dirty_to_draw) = sample.dirty_to_draw {
            self.dirty_to_draw_ms.push(duration_ms(dirty_to_draw));
        }
        if let Some(clean_gap) = sample.clean_gap {
            self.clean_gaps_ms.push(duration_ms(clean_gap));
        }
        self.invalidations.push(sample.invalidations);
        self.interval_over_budget += usize::from(sample.interval > FRAME_BUDGET_60_HZ);
        self.interval_over_two_budgets += usize::from(sample.interval > TWO_FRAME_BUDGETS_60_HZ);
        self.draw_over_budget += usize::from(sample.draw > FRAME_BUDGET_60_HZ);
    }

    fn take_statistics(&mut self) -> Option<WindowStatistics> {
        if self.span < MIN_REPORT_SPAN {
            return None;
        }

        let sample_count = self.intervals_ms.len();
        let statistics = WindowStatistics {
            dirty_draw_hz: sample_count as f64 / self.span.as_secs_f64(),
            sample_count,
            span_ms: duration_ms(self.span),
            interval: Distribution::from_f64(&self.intervals_ms),
            draw: Distribution::from_f64(&self.draws_ms),
            dirty_to_draw: Distribution::from_f64(&self.dirty_to_draw_ms),
            clean_gap: Distribution::from_f64(&self.clean_gaps_ms),
            invalidations: Distribution::from_u64(&self.invalidations),
            interval_over_budget_pct: percentage(self.interval_over_budget, sample_count),
            interval_over_two_budgets_pct: percentage(self.interval_over_two_budgets, sample_count),
            draw_over_budget_pct: percentage(self.draw_over_budget, sample_count),
        };
        self.clear();
        Some(statistics)
    }

    fn clear(&mut self) {
        self.span = Duration::ZERO;
        self.intervals_ms.clear();
        self.draws_ms.clear();
        self.dirty_to_draw_ms.clear();
        self.clean_gaps_ms.clear();
        self.invalidations.clear();
        self.interval_over_budget = 0;
        self.interval_over_two_budgets = 0;
        self.draw_over_budget = 0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Distribution {
    p50: f64,
    p95: f64,
    max: f64,
}

impl Distribution {
    fn from_f64(values: &[f64]) -> Self {
        Self::from_iter(values.iter().copied())
    }

    fn from_iter(values: impl IntoIterator<Item = f64>) -> Self {
        let mut sorted = values.into_iter().collect::<Vec<_>>();
        if sorted.is_empty() {
            return Self::default();
        }
        sorted.sort_by(f64::total_cmp);
        Self {
            p50: percentile(&sorted, 0.50),
            p95: percentile(&sorted, 0.95),
            max: sorted.last().copied().unwrap_or_default(),
        }
    }

    fn from_u64(values: &[u64]) -> Self {
        Self::from_iter(values.iter().map(|value| *value as f64))
    }
}

impl Default for Distribution {
    fn default() -> Self {
        Self {
            p50: 0.0,
            p95: 0.0,
            max: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct WindowStatistics {
    dirty_draw_hz: f64,
    sample_count: usize,
    span_ms: f64,
    interval: Distribution,
    draw: Distribution,
    dirty_to_draw: Distribution,
    clean_gap: Distribution,
    invalidations: Distribution,
    interval_over_budget_pct: f64,
    interval_over_two_budgets_pct: f64,
    draw_over_budget_pct: f64,
}

struct WindowTrace {
    previous_timing: Option<FrameTiming>,
    active: ActiveWindow,
    editor_render_ms: VecDeque<f64>,
    context: Option<TraceContext>,
}

impl Default for WindowTrace {
    fn default() -> Self {
        Self {
            previous_timing: None,
            active: ActiveWindow::default(),
            editor_render_ms: VecDeque::new(),
            context: None,
        }
    }
}

struct TraceState {
    collector: FrameTimingCollector,
    windows: HashMap<u64, WindowTrace>,
    last_poll_at: Instant,
}

impl TraceState {
    fn new(now: Instant) -> Self {
        gpui::set_frame_trace_enabled(true);
        Self {
            collector: FrameTimingCollector::new(),
            windows: HashMap::new(),
            last_poll_at: now,
        }
    }

    fn poll_if_due(
        &mut self,
        now: Instant,
        current_window_id: WindowId,
        input: &AppFrameTelemetryInput,
    ) -> Vec<FpsTraceLine> {
        let current_window = self.windows.entry(current_window_id.as_u64()).or_default();
        if current_window.editor_render_ms.len() == EDITOR_RENDER_SAMPLE_CAPACITY {
            current_window.editor_render_ms.pop_front();
        }
        current_window
            .editor_render_ms
            .push_back(duration_ms(input.elapsed));
        if !poll_due(self.last_poll_at, now) {
            return Vec::new();
        }
        self.last_poll_at = now;
        self.windows
            .entry(current_window_id.as_u64())
            .or_default()
            .context = Some(TraceContext::from(input));

        for timing in self.collector.collect_unseen() {
            let window_id = timing.window_id.as_u64();
            let window = self.windows.entry(window_id).or_default();
            if let Some(previous) = window.previous_timing {
                let sample = DrawSample {
                    interval: timing.draw_start.duration_since(previous.draw_start),
                    draw: timing.draw_duration(),
                    dirty_to_draw: timing.dirty_to_draw_duration(),
                    clean_gap: timing
                        .dirty_at
                        .and_then(|dirty_at| dirty_at.checked_duration_since(previous.draw_end)),
                    invalidations: timing.invalidations,
                };
                window.active.record(sample);
            }
            window.previous_timing = Some(timing);
        }

        let mut lines = Vec::new();
        for (window_id, window) in &mut self.windows {
            let Some(context) = window.context.clone() else {
                continue;
            };
            let Some(statistics) = window.active.take_statistics() else {
                continue;
            };
            let editor_render = Distribution::from_iter(window.editor_render_ms.iter().copied());
            window.editor_render_ms.clear();
            lines.push(FpsTraceLine {
                window_id: *window_id,
                statistics,
                editor_render,
                context,
            });
        }
        lines
    }
}

struct FpsTraceLine {
    window_id: u64,
    statistics: WindowStatistics,
    editor_render: Distribution,
    context: TraceContext,
}

impl fmt::Display for FpsTraceLine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stats = &self.statistics;
        let editor_render = &self.editor_render;
        let context = &self.context;
        write!(
            formatter,
            "[cditor][fps] window={} dirty_draw_hz={:.1} draws={} span_ms={:.1} \
draw_gap_ms[p50={:.2} p95={:.2} max={:.2} gt16.7={:.1}% gt33.3={:.1}%] \
gpui_draw_ms[p50={:.2} p95={:.2} max={:.2} gt16.7={:.1}%] \
dirty_to_draw_ms[p50={:.2} p95={:.2} max={:.2}] clean_gap_ms[p50={:.2} p95={:.2} max={:.2}] \
editor_render_ms[p50={:.2} p95={:.2} max={:.2}] \
invalidations[p50={:.0} p95={:.0} max={:.0}] interaction={} queues[layout={} payload={} save={} lanes={}/{}/{}/{}/{}] \
document[blocks={} rendered={} loaded={} payload={}..{} pages={}..{}] layouts[block={} table_cell={} auxiliary={}] \
cache_mib[payload={:.1} platform={:.1} image={:.1}/{} mermaid={:.1}+reserved:{:.1}/{} video_cpu={:.1} video_render={:.1}] pressure[payload={} platform={} image={} mermaid={}] geometry_fallback={:.3}",
            self.window_id,
            stats.dirty_draw_hz,
            stats.sample_count,
            stats.span_ms,
            stats.interval.p50,
            stats.interval.p95,
            stats.interval.max,
            stats.interval_over_budget_pct,
            stats.interval_over_two_budgets_pct,
            stats.draw.p50,
            stats.draw.p95,
            stats.draw.max,
            stats.draw_over_budget_pct,
            stats.dirty_to_draw.p50,
            stats.dirty_to_draw.p95,
            stats.dirty_to_draw.max,
            stats.clean_gap.p50,
            stats.clean_gap.p95,
            stats.clean_gap.max,
            editor_render.p50,
            editor_render.p95,
            editor_render.max,
            stats.invalidations.p50,
            stats.invalidations.p95,
            stats.invalidations.max,
            context.interaction,
            context.pending_layout_tasks,
            context.pending_payload_loads,
            context.pending_saves,
            optional_depth(context.realtime_lane_depth),
            optional_depth(context.interactive_lane_depth),
            optional_depth(context.visible_lane_depth),
            optional_depth(context.prefetch_lane_depth),
            optional_depth(context.background_lane_depth),
            context.document_blocks,
            context.rendered_blocks,
            context.loaded_payloads,
            context.payload_start,
            context.payload_end,
            context.page_start,
            context.page_end,
            context.block_layouts,
            context.table_cell_layouts,
            context.auxiliary_layouts,
            bytes_to_mib(context.payload_and_undo_bytes),
            bytes_to_mib(context.platform_layout_bytes),
            bytes_to_mib(context.image_resident_decoded_bytes),
            context.image_cache_entries,
            bytes_to_mib(context.mermaid_resident_image_bytes),
            bytes_to_mib(context.mermaid_reserved_render_bytes),
            context.mermaid_cache_entries,
            bytes_to_mib(context.video_resident_cpu_frame_bytes),
            bytes_to_mib(context.video_resident_render_image_bytes),
            context.payload_cache_over_budget,
            context.platform_layout_cache_over_budget,
            context.image_cache_over_budget,
            context.mermaid_cache_over_budget,
            context.text_geometry_fallback_rate,
        )
    }
}

pub(crate) fn trace_gpui_frames(window_id: WindowId, input: &AppFrameTelemetryInput) {
    if !fps_trace_enabled() {
        return;
    }
    let lines = trace_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .poll_if_due(Instant::now(), window_id, input);
    for line in lines {
        super::stderr::write(format_args!("{line}"));
    }
}

fn trace_state() -> &'static Mutex<TraceState> {
    static STATE: OnceLock<Mutex<TraceState>> = OnceLock::new();
    STATE.get_or_init(|| {
        super::stderr::write(format_args!(
            "[cditor][fps] enabled source=gpui_frame_timing collect_ms={} min_report_span_ms={} inactive_gap_ms={}",
            COLLECT_INTERVAL.as_millis(),
            MIN_REPORT_SPAN.as_millis(),
            INACTIVE_FRAME_GAP.as_millis(),
        ));
        Mutex::new(TraceState::new(Instant::now()))
    })
}

fn fps_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| env_value_enabled(std::env::var("CDITOR_TRACE_FPS").ok().as_deref()))
}

fn poll_due(last_poll_at: Instant, now: Instant) -> bool {
    now.duration_since(last_poll_at) >= COLLECT_INTERVAL
}

fn env_value_enabled(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn percentile(sorted: &[f64], quantile: f64) -> f64 {
    let rank = ((sorted.len() as f64 * quantile).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len().saturating_sub(1));
    sorted.get(rank).copied().unwrap_or_default()
}

fn percentage(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        count as f64 * 100.0 / total as f64
    }
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn bytes_to_mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn optional_depth(depth: Option<usize>) -> String {
    depth.map_or_else(|| "-".to_owned(), |depth| depth.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(interval_ms: u64, draw_ms: u64) -> DrawSample {
        DrawSample {
            interval: Duration::from_millis(interval_ms),
            draw: Duration::from_millis(draw_ms),
            dirty_to_draw: Some(Duration::from_millis(draw_ms + 1)),
            clean_gap: Some(Duration::from_millis(
                interval_ms.saturating_sub(draw_ms + 1),
            )),
            invalidations: 2,
        }
    }

    #[test]
    fn trace_switch_accepts_documented_truthy_values() {
        for value in ["1", "true", "TRUE", "yes", "YES", "on", " ON "] {
            assert!(env_value_enabled(Some(value)), "value={value}");
        }
        for value in ["", "0", "false", "off", "fps"] {
            assert!(!env_value_enabled(Some(value)), "value={value}");
        }
        assert!(!env_value_enabled(None));
    }

    #[test]
    fn gpui_collector_is_polled_at_most_once_per_second() {
        let started_at = Instant::now();
        assert!(!poll_due(
            started_at,
            started_at + COLLECT_INTERVAL - Duration::from_millis(1)
        ));
        assert!(poll_due(started_at, started_at + COLLECT_INTERVAL));
    }

    #[test]
    fn one_second_window_reports_fps_percentiles_and_budget_pressure() {
        let mut window = ActiveWindow::default();
        for index in 0..50 {
            let draw_ms = if index == 49 { 18 } else { 4 };
            window.record(sample(20, draw_ms));
        }
        let report = window
            .take_statistics()
            .expect("fifty samples should complete the one-second report window");

        assert_eq!(report.sample_count, 50);
        assert_eq!(report.dirty_draw_hz, 50.0);
        assert_eq!(report.interval.p50, 20.0);
        assert_eq!(report.interval.p95, 20.0);
        assert_eq!(report.interval.max, 20.0);
        assert_eq!(report.draw.p95, 4.0);
        assert_eq!(report.draw.max, 18.0);
        assert_eq!(report.dirty_to_draw.max, 19.0);
        assert_eq!(report.clean_gap.p50, 15.0);
        assert_eq!(report.invalidations.p95, 2.0);
        assert_eq!(report.interval_over_budget_pct, 100.0);
        assert_eq!(report.interval_over_two_budgets_pct, 0.0);
        assert!(report.draw_over_budget_pct > 1.0);
        assert!(window.take_statistics().is_none());
    }

    #[test]
    fn inactive_gap_resets_instead_of_reporting_caret_blink_as_low_fps() {
        let mut window = ActiveWindow::default();
        for _ in 0..20 {
            window.record(sample(20, 3));
        }
        window.record(sample(500, 3));
        assert_eq!(window.span, Duration::ZERO);
        assert!(window.intervals_ms.is_empty());
        assert!(window.take_statistics().is_none());
    }

    #[test]
    fn percentile_uses_nearest_rank() {
        let distribution = Distribution::from_f64(&[40.0, 10.0, 30.0, 20.0, 50.0]);
        assert_eq!(distribution.p50, 30.0);
        assert_eq!(distribution.p95, 50.0);
        assert_eq!(distribution.max, 50.0);
    }
}
