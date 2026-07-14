use super::*;
use crate::{PayloadCachePolicy, PayloadCacheTrimReport};

impl DocumentRuntime {
    /// Acknowledges the exact payload versions included in a successful save.
    /// A newer edit made while the save was in flight remains dirty and pinned.
    pub fn mark_payload_versions_persisted(&mut self, versions: &[(BlockId, u64)]) {
        self.payload_window.mark_persisted_versions(versions);
    }

    /// Trims heavyweight payload entities without evicting the active viewport,
    /// interaction state, dirty content, or UI-owned asynchronous work.
    pub fn trim_payload_cache(
        &mut self,
        policy: PayloadCachePolicy,
        extra_pins: impl IntoIterator<Item = BlockId>,
    ) -> PayloadCacheTrimReport {
        self.payload_window.refresh_estimated_bytes();
        let before_entries = self.payload_window.payloads.len();
        let before_estimated_bytes = self.payload_window.total_estimated_bytes();

        let active_ids = self
            .payload_window
            .block_range
            .clone()
            .filter_map(|visible_index| self.visible_index.id_at_visible_index(visible_index))
            .collect::<Vec<_>>();
        let mut protected = active_ids.iter().copied().collect::<HashSet<_>>();
        for block_id in active_ids {
            self.payload_window.touch(block_id);
        }

        if let Some(editing) = self.editing.as_ref() {
            protected.extend(editing.pinned_blocks().iter().copied());
        }
        if let Some(first) = self
            .selected_block_ids
            .iter()
            .min_by_key(|block_id| self.index.index_of(**block_id).unwrap_or(usize::MAX))
        {
            protected.insert(*first);
        }
        if let Some(last) = self
            .selected_block_ids
            .iter()
            .max_by_key(|block_id| self.index.index_of(**block_id).unwrap_or(0))
        {
            protected.insert(*last);
        }
        if let Some(selection) = self.document_selection {
            protected.insert(selection.anchor.block_id);
            protected.insert(selection.focus.block_id);
        }
        if let Some(focused) = self.focused_table_cell {
            protected.insert(focused.block_id);
        }
        protected.extend(self.ai_payload_pin_ids());
        protected.extend(self.payload_window.loading.iter().copied());
        protected.extend(extra_pins);

        let dirty = self
            .payload_window
            .payloads
            .keys()
            .copied()
            .filter(|block_id| self.payload_window.is_dirty(*block_id))
            .collect::<HashSet<_>>();
        let evicted = self.payload_window.evict_to_limits(
            policy.max_entries,
            policy.max_estimated_bytes,
            |block_id, _| !protected.contains(&block_id) && !dirty.contains(&block_id),
        );
        let mut evicted_block_ids = Vec::with_capacity(evicted.len());
        for evicted in evicted {
            let block_id = evicted.block_id;
            self.text_models.remove(&block_id);
            self.table_runtimes.remove(&block_id);
            self.table_horizontal_scroll_offsets.remove(&block_id);
            self.pending_measured_heights.remove(&block_id);
            evicted_block_ids.push(block_id);
        }

        let after_entries = self.payload_window.payloads.len();
        let after_estimated_bytes = self.payload_window.total_estimated_bytes();
        PayloadCacheTrimReport {
            before_entries,
            after_entries,
            before_estimated_bytes,
            after_estimated_bytes,
            evicted_entries: evicted_block_ids.len(),
            evicted_block_ids,
            over_capacity: after_entries > policy.max_entries
                || after_estimated_bytes > policy.max_estimated_bytes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_with_paragraph_blocks(count: usize) -> DocumentRuntime {
        let payloads = (1..=count as BlockId)
            .map(|block_id| BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, ""))
            .collect();
        DocumentRuntime::from_payloads(1, payloads, 720.0)
    }

    fn narrow_active_window(runtime: &mut DocumentRuntime, range: Range<usize>) {
        runtime.payload_window.block_range = range;
    }

    fn entry_policy(max_entries: usize) -> PayloadCachePolicy {
        PayloadCachePolicy {
            max_entries,
            max_estimated_bytes: usize::MAX,
        }
    }

    #[test]
    fn clean_lru_payloads_outside_the_active_window_are_evicted() {
        let mut runtime = runtime_with_paragraph_blocks(10);
        narrow_active_window(&mut runtime, 8..10);

        let report = runtime.trim_payload_cache(entry_policy(3), []);

        assert_eq!(report.after_entries, 3);
        assert_eq!(report.evicted_entries, 7);
        assert!(runtime.payload_window.get(9).is_some());
        assert!(runtime.payload_window.get(10).is_some());
    }

    #[test]
    fn revisiting_a_resident_window_refreshes_its_lru_position() {
        let mut runtime = runtime_with_paragraph_blocks(4);
        assert!(runtime.activate_payload_window_if_resident(0..1));
        assert!(runtime.activate_payload_window_if_resident(3..4));

        runtime.trim_payload_cache(entry_policy(2), []);

        assert!(runtime.payload_window.get(1).is_some());
        assert!(runtime.payload_window.get(4).is_some());
        assert!(runtime.payload_window.get(2).is_none());
        assert!(runtime.payload_window.get(3).is_none());
    }

    #[test]
    fn editing_and_active_window_are_pinned() {
        let mut runtime = runtime_with_paragraph_blocks(10);
        narrow_active_window(&mut runtime, 8..10);
        runtime.focus_block_at_offset(1, 0).unwrap();

        let report = runtime.trim_payload_cache(entry_policy(3), []);

        assert!(!report.over_capacity);
        for block_id in [1, 9, 10] {
            assert!(runtime.payload_window.get(block_id).is_some(), "{block_id}");
        }
    }

    #[test]
    fn document_selection_endpoints_are_pinned() {
        let mut runtime = runtime_with_paragraph_blocks(10);
        narrow_active_window(&mut runtime, 8..10);
        runtime.set_document_text_selection(2, 0, 3, 0).unwrap();

        let report = runtime.trim_payload_cache(entry_policy(4), []);

        assert!(!report.over_capacity);
        for block_id in [2, 3, 9, 10] {
            assert!(runtime.payload_window.get(block_id).is_some(), "{block_id}");
        }
    }

    #[test]
    fn dirty_payload_cannot_be_evicted_until_its_exact_version_is_saved() {
        let mut runtime = runtime_with_paragraph_blocks(4);
        narrow_active_window(&mut runtime, 3..4);
        let mut edited = runtime.payload_window.get(1).unwrap().clone();
        edited.content_version = 2;
        runtime.payload_window.insert(edited);

        let dirty_report = runtime.trim_payload_cache(entry_policy(1), []);
        assert!(dirty_report.over_capacity);
        assert!(runtime.payload_window.get(1).is_some());
        assert!(runtime.payload_window.get(4).is_some());

        runtime.mark_payload_versions_persisted(&[(1, 1)]);
        assert!(runtime.payload_window.is_dirty(1));
        runtime.mark_payload_versions_persisted(&[(1, 2)]);
        let saved_report = runtime.trim_payload_cache(entry_policy(1), []);
        assert!(!saved_report.over_capacity);
        assert!(runtime.payload_window.get(1).is_none());
        assert!(runtime.payload_window.get(4).is_some());
    }

    #[test]
    fn eviction_releases_runtime_text_entities() {
        let mut runtime = runtime_with_paragraph_blocks(3);
        narrow_active_window(&mut runtime, 2..3);
        assert!(runtime.text_models.contains_key(&1));

        runtime.trim_payload_cache(entry_policy(1), []);

        assert!(runtime.payload_window.get(1).is_none());
        assert!(!runtime.text_models.contains_key(&1));
    }

    #[test]
    fn evicted_table_runtime_rehydrates_from_a_reloaded_payload() {
        let table_payload = BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: default_table_payload("cell".to_owned()),
        };
        let paragraph = BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "after");
        let mut runtime =
            DocumentRuntime::from_payloads(1, vec![table_payload.clone(), paragraph], 720.0);
        narrow_active_window(&mut runtime, 1..2);
        assert!(runtime.table_runtimes.contains_key(&1));

        runtime.trim_payload_cache(entry_policy(1), []);
        assert!(!runtime.table_runtimes.contains_key(&1));

        runtime.payload_window.insert_loaded(table_payload);
        runtime.hydrate_payload_runtime_state(1);
        assert!(runtime.table_runtimes.contains_key(&1));
    }

    #[test]
    fn byte_budget_evicts_large_clean_payloads() {
        let payloads = (1..=4)
            .map(|block_id| {
                BlockPayloadRecord::rich_text(
                    block_id,
                    RichBlockKind::Paragraph,
                    "x".repeat(8 * 1024),
                )
            })
            .collect();
        let mut runtime = DocumentRuntime::from_payloads(1, payloads, 720.0);
        narrow_active_window(&mut runtime, 3..4);
        let active_bytes = runtime
            .payload_window
            .get(4)
            .map(crate::content::payload_cache::estimated_payload_record_bytes)
            .unwrap();

        let report = runtime.trim_payload_cache(
            PayloadCachePolicy {
                max_entries: usize::MAX,
                max_estimated_bytes: active_bytes + 256,
            },
            [],
        );

        assert!(!report.over_capacity);
        assert_eq!(report.after_entries, 1);
        assert!(runtime.payload_window.get(4).is_some());
    }

    #[test]
    fn repeated_large_window_residency_stays_within_the_entry_budget() {
        let mut runtime = runtime_with_paragraph_blocks(10_000);
        narrow_active_window(&mut runtime, 9_936..10_000);

        let report = runtime.trim_payload_cache(entry_policy(128), []);

        assert!(!report.over_capacity);
        assert_eq!(report.after_entries, 128);
        assert_eq!(runtime.text_models.len(), 128);
    }
}
