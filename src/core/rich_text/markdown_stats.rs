use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

#[derive(Debug, Default)]
pub struct MarkdownParseStats {
    full_parse_count: AtomicUsize,
    incremental_parse_count: AtomicUsize,
    full_parse_chars: AtomicU64,
    incremental_parse_chars: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkdownParseStatsSnapshot {
    pub full_parse_count: usize,
    pub incremental_parse_count: usize,
    pub full_parse_chars: u64,
    pub incremental_parse_chars: u64,
}

impl MarkdownParseStats {
    pub const fn new() -> Self {
        Self {
            full_parse_count: AtomicUsize::new(0),
            incremental_parse_count: AtomicUsize::new(0),
            full_parse_chars: AtomicU64::new(0),
            incremental_parse_chars: AtomicU64::new(0),
        }
    }

    pub fn record_full_parse(&self, char_count: usize) {
        self.full_parse_count.fetch_add(1, Ordering::Relaxed);
        self.full_parse_chars
            .fetch_add(char_count as u64, Ordering::Relaxed);
    }

    pub fn record_incremental_parse(&self, char_count: usize) {
        self.incremental_parse_count.fetch_add(1, Ordering::Relaxed);
        self.incremental_parse_chars
            .fetch_add(char_count as u64, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> MarkdownParseStatsSnapshot {
        MarkdownParseStatsSnapshot {
            full_parse_count: self.full_parse_count.load(Ordering::Relaxed),
            incremental_parse_count: self.incremental_parse_count.load(Ordering::Relaxed),
            full_parse_chars: self.full_parse_chars.load(Ordering::Relaxed),
            incremental_parse_chars: self.incremental_parse_chars.load(Ordering::Relaxed),
        }
    }

    pub fn reset(&self) {
        self.full_parse_count.store(0, Ordering::Relaxed);
        self.incremental_parse_count.store(0, Ordering::Relaxed);
        self.full_parse_chars.store(0, Ordering::Relaxed);
        self.incremental_parse_chars.store(0, Ordering::Relaxed);
    }
}

pub static MARKDOWN_PARSE_STATS: MarkdownParseStats = MarkdownParseStats::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_parse_stats_on_local_instance_like_v1() {
        let stats = MarkdownParseStats::new();
        stats.record_incremental_parse(12);
        stats.record_full_parse(100);

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.incremental_parse_count, 1);
        assert_eq!(snapshot.incremental_parse_chars, 12);
        assert_eq!(snapshot.full_parse_count, 1);
        assert_eq!(snapshot.full_parse_chars, 100);

        stats.reset();
        let snapshot = stats.snapshot();
        assert_eq!(snapshot.incremental_parse_count, 0);
        assert_eq!(snapshot.full_parse_count, 0);
    }
}
