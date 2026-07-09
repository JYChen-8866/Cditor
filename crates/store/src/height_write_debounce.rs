use std::collections::HashMap;

use crate::layout_cache::{BlockLayoutRow, InMemoryLayoutCacheStore, PageLayoutRow};
use cditor_core::ids::{BlockId, DocumentId};

pub const DEFAULT_HEIGHT_WRITE_DEBOUNCE_MS: u64 = 500;

#[derive(Debug, Clone, PartialEq)]
pub enum HeightWrite {
    Block(BlockLayoutRow),
    Page(PageLayoutRow),
}

impl HeightWrite {
    fn key(&self) -> HeightWriteKey {
        match self {
            Self::Block(row) => HeightWriteKey::Block {
                block_id: row.block_id,
                layout_key_hash: row.layout_key_hash.clone(),
            },
            Self::Page(row) => HeightWriteKey::Page {
                document_id: row.document_id,
                visible_index_version: row.visible_index_version,
                structure_version: row.structure_version,
                layout_key_hash: row.layout_key_hash.clone(),
                page_policy_version: row.page_policy_version,
                page_index: row.page_index,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum HeightWriteKey {
    Block {
        block_id: BlockId,
        layout_key_hash: String,
    },
    Page {
        document_id: DocumentId,
        visible_index_version: u64,
        structure_version: u64,
        layout_key_hash: String,
        page_policy_version: u64,
        page_index: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeightWriteTrigger {
    DebounceElapsed,
    CloseFlush,
    RetryDirty,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeightWriteError {
    SinkFailed(&'static str),
}

pub trait HeightWriteSink {
    fn write_batch(&mut self, writes: &[HeightWrite]) -> Result<(), HeightWriteError>;
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct InMemoryHeightWriteSink {
    pub writes: Vec<Vec<HeightWrite>>,
    pub fail_next: bool,
}

impl HeightWriteSink for InMemoryHeightWriteSink {
    fn write_batch(&mut self, writes: &[HeightWrite]) -> Result<(), HeightWriteError> {
        if self.fail_next {
            self.fail_next = false;
            return Err(HeightWriteError::SinkFailed(
                "simulated sqlite write failure",
            ));
        }
        self.writes.push(writes.to_vec());
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeightWriteDebouncer {
    debounce_ms: u64,
    memory_cache: InMemoryLayoutCacheStore,
    pending: HashMap<HeightWriteKey, HeightWrite>,
    dirty_queue: Vec<HeightWrite>,
    last_measure_at_ms: Option<u64>,
    pub synchronous_ui_writes: usize,
    pub scheduled_persistence_jobs: usize,
}

impl HeightWriteDebouncer {
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            debounce_ms,
            memory_cache: InMemoryLayoutCacheStore::default(),
            pending: HashMap::new(),
            dirty_queue: Vec::new(),
            last_measure_at_ms: None,
            synchronous_ui_writes: 0,
            scheduled_persistence_jobs: 0,
        }
    }

    pub fn memory_cache(&self) -> &InMemoryLayoutCacheStore {
        &self.memory_cache
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn dirty_len(&self) -> usize {
        self.dirty_queue.len()
    }

    pub fn on_height_measured(&mut self, write: HeightWrite, now_ms: u64) {
        match &write {
            HeightWrite::Block(row) => self.memory_cache.save_block_layout(row.clone()),
            HeightWrite::Page(row) => self.memory_cache.save_page_layout(row.clone()),
        }
        self.pending.insert(write.key(), write);
        self.last_measure_at_ms = Some(now_ms);
        self.scheduled_persistence_jobs = self.scheduled_persistence_jobs.saturating_add(1);
    }

    pub fn flush_if_due(
        &mut self,
        now_ms: u64,
        sink: &mut impl HeightWriteSink,
    ) -> Result<Option<HeightWriteFlush>, HeightWriteError> {
        let Some(last_measure_at_ms) = self.last_measure_at_ms else {
            return Ok(None);
        };
        if now_ms.saturating_sub(last_measure_at_ms) < self.debounce_ms {
            return Ok(None);
        }
        self.flush(HeightWriteTrigger::DebounceElapsed, sink)
            .map(Some)
    }

    pub fn flush_on_close(
        &mut self,
        sink: &mut impl HeightWriteSink,
    ) -> Result<HeightWriteFlush, HeightWriteError> {
        self.flush(HeightWriteTrigger::CloseFlush, sink)
    }

    pub fn retry_dirty(
        &mut self,
        sink: &mut impl HeightWriteSink,
    ) -> Result<HeightWriteFlush, HeightWriteError> {
        if self.dirty_queue.is_empty() {
            return Ok(HeightWriteFlush {
                trigger: HeightWriteTrigger::RetryDirty,
                attempted: 0,
                written: 0,
                dirty_after: 0,
            });
        }
        let writes = std::mem::take(&mut self.dirty_queue);
        match sink.write_batch(&writes) {
            Ok(()) => Ok(HeightWriteFlush {
                trigger: HeightWriteTrigger::RetryDirty,
                attempted: writes.len(),
                written: writes.len(),
                dirty_after: 0,
            }),
            Err(error) => {
                self.dirty_queue = writes;
                Err(error)
            }
        }
    }

    fn flush(
        &mut self,
        trigger: HeightWriteTrigger,
        sink: &mut impl HeightWriteSink,
    ) -> Result<HeightWriteFlush, HeightWriteError> {
        if self.pending.is_empty() {
            return Ok(HeightWriteFlush {
                trigger,
                attempted: 0,
                written: 0,
                dirty_after: self.dirty_queue.len(),
            });
        }

        let writes = self
            .pending
            .drain()
            .map(|(_, write)| write)
            .collect::<Vec<_>>();
        let attempted = writes.len();
        match sink.write_batch(&writes) {
            Ok(()) => {
                self.last_measure_at_ms = None;
                Ok(HeightWriteFlush {
                    trigger,
                    attempted,
                    written: attempted,
                    dirty_after: self.dirty_queue.len(),
                })
            }
            Err(error) => {
                self.dirty_queue.extend(writes);
                self.last_measure_at_ms = None;
                Err(error)
            }
        }
    }
}

impl Default for HeightWriteDebouncer {
    fn default() -> Self {
        Self::new(DEFAULT_HEIGHT_WRITE_DEBOUNCE_MS)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeightWriteFlush {
    pub trigger: HeightWriteTrigger,
    pub attempted: usize,
    pub written: usize,
    pub dirty_after: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_cache::LayoutCacheKey;
    use cditor_core::layout::{HeightConfidence, HeightEstimate};

    #[test]
    fn typing_layout_height_updates_write_memory_first_not_sqlite_per_char() {
        let mut debouncer = HeightWriteDebouncer::default();
        let mut sink = InMemoryHeightWriteSink::default();
        let key = key(10, 800, 1);

        for index in 0..10 {
            debouncer.on_height_measured(
                HeightWrite::Block(BlockLayoutRow::new(
                    42,
                    key,
                    HeightEstimate {
                        height: 20.0 + index as f64,
                        confidence: HeightConfidence::Exact,
                        max_error_hint: 0.0,
                    },
                )),
                index * 10,
            );
            assert!(
                debouncer
                    .flush_if_due(index * 10, &mut sink)
                    .unwrap()
                    .is_none()
            );
        }

        let cached = debouncer.memory_cache().load_block_height(42, key);
        assert_eq!(cached.height, 29.0);
        assert_eq!(sink.writes.len(), 0);
        assert_eq!(debouncer.synchronous_ui_writes, 0);
        assert_eq!(debouncer.pending_len(), 1);
    }

    #[test]
    fn debounce_batches_height_storm_into_one_write_transaction() {
        let mut debouncer = HeightWriteDebouncer::default();
        let mut sink = InMemoryHeightWriteSink::default();
        for block_id in 1..=100 {
            debouncer.on_height_measured(
                HeightWrite::Block(BlockLayoutRow::new(
                    block_id,
                    key(10, 800, 1),
                    HeightEstimate {
                        height: 40.0,
                        confidence: HeightConfidence::Exact,
                        max_error_hint: 0.0,
                    },
                )),
                0,
            );
        }

        let flush = debouncer.flush_if_due(500, &mut sink).unwrap().unwrap();

        assert_eq!(flush.trigger, HeightWriteTrigger::DebounceElapsed);
        assert_eq!(flush.written, 100);
        assert_eq!(sink.writes.len(), 1);
        assert_eq!(sink.writes[0].len(), 100);
        assert_eq!(debouncer.pending_len(), 0);
    }

    #[test]
    fn close_flush_writes_pending_before_shutdown() {
        let mut debouncer = HeightWriteDebouncer::default();
        let mut sink = InMemoryHeightWriteSink::default();
        debouncer.on_height_measured(
            HeightWrite::Block(BlockLayoutRow::new(
                42,
                key(10, 800, 1),
                HeightEstimate {
                    height: 64.0,
                    confidence: HeightConfidence::Exact,
                    max_error_hint: 0.0,
                },
            )),
            100,
        );

        let flush = debouncer.flush_on_close(&mut sink).unwrap();

        assert_eq!(flush.trigger, HeightWriteTrigger::CloseFlush);
        assert_eq!(flush.written, 1);
        assert_eq!(sink.writes.len(), 1);
    }

    #[test]
    fn sqlite_write_failure_moves_batch_to_dirty_queue_and_can_retry() {
        let mut debouncer = HeightWriteDebouncer::default();
        let mut sink = InMemoryHeightWriteSink {
            fail_next: true,
            ..InMemoryHeightWriteSink::default()
        };
        debouncer.on_height_measured(
            HeightWrite::Block(BlockLayoutRow::new(
                42,
                key(10, 800, 1),
                HeightEstimate {
                    height: 64.0,
                    confidence: HeightConfidence::Exact,
                    max_error_hint: 0.0,
                },
            )),
            0,
        );

        let error = debouncer.flush_if_due(500, &mut sink).unwrap_err();
        assert_eq!(
            error,
            HeightWriteError::SinkFailed("simulated sqlite write failure")
        );
        assert_eq!(debouncer.dirty_len(), 1);

        let retry = debouncer.retry_dirty(&mut sink).unwrap();
        assert_eq!(retry.trigger, HeightWriteTrigger::RetryDirty);
        assert_eq!(retry.written, 1);
        assert_eq!(debouncer.dirty_len(), 0);
    }

    #[test]
    fn page_height_updates_are_debounced_with_block_heights() {
        let mut debouncer = HeightWriteDebouncer::default();
        let mut sink = InMemoryHeightWriteSink::default();
        debouncer.on_height_measured(HeightWrite::Page(page_row(1, 0, 1000.0)), 0);
        debouncer.on_height_measured(HeightWrite::Page(page_row(1, 1, 1100.0)), 20);
        debouncer.on_height_measured(
            HeightWrite::Block(BlockLayoutRow::new(
                42,
                key(10, 800, 1),
                HeightEstimate {
                    height: 64.0,
                    confidence: HeightConfidence::Exact,
                    max_error_hint: 0.0,
                },
            )),
            40,
        );

        let flush = debouncer.flush_if_due(540, &mut sink).unwrap().unwrap();

        assert_eq!(flush.written, 3);
        assert_eq!(sink.writes.len(), 1);
    }

    fn key(width_bucket: u16, exact_width_px: u32, content_version: u64) -> LayoutCacheKey {
        LayoutCacheKey {
            width_bucket,
            exact_width_px,
            content_version,
            attrs_version: 0,
            style_version: 0,
            font_version: 1,
            theme_version: 1,
            scale_factor_milli: 1000,
        }
    }

    fn page_row(document_id: DocumentId, page_index: usize, height: f64) -> PageLayoutRow {
        PageLayoutRow {
            document_id,
            visible_index_version: 1,
            structure_version: 1,
            layout_key_hash: "layout".to_string(),
            page_policy_version: 1,
            page_index,
            block_start_index: page_index * 100,
            block_count: 100,
            first_block_id: Some(page_index as BlockId * 100 + 1),
            last_block_id: Some(page_index as BlockId * 100 + 100),
            height,
            measured_ratio: 1.0,
            confidence: HeightConfidence::Exact,
            max_error_hint: 0.0,
            dirty: false,
            updated_at: 0,
        }
    }
}
