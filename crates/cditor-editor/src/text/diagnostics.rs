use std::cell::Cell;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TextGeometryTelemetry {
    pub range_bounds_queries: u64,
    pub point_hit_queries: u64,
    pub navigation_queries: u64,
    pub synchronous_layout_fallbacks: u64,
    pub unavailable_queries: u64,
}

impl TextGeometryTelemetry {
    pub fn snapshot_queries(self) -> u64 {
        self.range_bounds_queries
            .saturating_add(self.point_hit_queries)
            .saturating_add(self.navigation_queries)
    }

    pub fn completed_queries(self) -> u64 {
        self.snapshot_queries()
            .saturating_add(self.synchronous_layout_fallbacks)
    }

    pub fn fallback_rate(self) -> f64 {
        let completed = self.completed_queries();
        if completed == 0 {
            0.0
        } else {
            self.synchronous_layout_fallbacks as f64 / completed as f64
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextGeometryOperation {
    RangeBounds,
    PointHit,
    Navigation,
}

thread_local! {
    static TELEMETRY: Cell<TextGeometryTelemetry> =
        Cell::new(TextGeometryTelemetry::default());
}

pub(crate) fn record_snapshot_geometry(operation: TextGeometryOperation) {
    TELEMETRY.with(|telemetry| {
        let mut stats = telemetry.get();
        match operation {
            TextGeometryOperation::RangeBounds => {
                stats.range_bounds_queries = stats.range_bounds_queries.saturating_add(1);
            }
            TextGeometryOperation::PointHit => {
                stats.point_hit_queries = stats.point_hit_queries.saturating_add(1);
            }
            TextGeometryOperation::Navigation => {
                stats.navigation_queries = stats.navigation_queries.saturating_add(1);
            }
        }
        telemetry.set(stats);
    });
}

pub(crate) fn record_synchronous_geometry_fallback() {
    TELEMETRY.with(|telemetry| {
        let mut stats = telemetry.get();
        stats.synchronous_layout_fallbacks = stats.synchronous_layout_fallbacks.saturating_add(1);
        telemetry.set(stats);
    });
}

pub(crate) fn record_unavailable_geometry() {
    TELEMETRY.with(|telemetry| {
        let mut stats = telemetry.get();
        stats.unavailable_queries = stats.unavailable_queries.saturating_add(1);
        telemetry.set(stats);
    });
}

pub(crate) fn text_geometry_telemetry() -> TextGeometryTelemetry {
    TELEMETRY.with(Cell::get)
}

#[cfg(test)]
pub(crate) fn reset_text_geometry_telemetry() {
    TELEMETRY.with(|telemetry| telemetry.set(TextGeometryTelemetry::default()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_separates_snapshot_fallback_and_unavailable_queries() {
        reset_text_geometry_telemetry();
        record_snapshot_geometry(TextGeometryOperation::RangeBounds);
        record_snapshot_geometry(TextGeometryOperation::PointHit);
        record_snapshot_geometry(TextGeometryOperation::Navigation);
        record_synchronous_geometry_fallback();
        record_unavailable_geometry();

        let stats = text_geometry_telemetry();
        assert_eq!(stats.snapshot_queries(), 3);
        assert_eq!(stats.completed_queries(), 4);
        assert_eq!(stats.synchronous_layout_fallbacks, 1);
        assert_eq!(stats.unavailable_queries, 1);
        assert_eq!(stats.fallback_rate(), 0.25);
    }

    #[test]
    fn empty_telemetry_has_zero_fallback_rate() {
        reset_text_geometry_telemetry();
        assert_eq!(text_geometry_telemetry().fallback_rate(), 0.0);
    }
}
