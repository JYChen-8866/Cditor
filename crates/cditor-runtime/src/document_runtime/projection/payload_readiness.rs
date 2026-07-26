use super::*;

impl DocumentRuntime {
    pub(super) fn payload_window_covers(&self, block_range: &Range<usize>) -> bool {
        if block_range.is_empty() {
            return true;
        }
        if self.document.payload_window.block_range.start > block_range.start
            || block_range.end > self.document.payload_window.block_range.end
        {
            return false;
        }
        block_range.clone().all(|visible_index| {
            self.document
                .visible_index
                .id_at_visible_index(visible_index)
                .is_some_and(|block_id| {
                    self.document
                        .payload_window
                        .payloads
                        .contains_key(&block_id)
                })
        })
    }

    pub(super) fn payload_window_has_any(&self, block_range: &Range<usize>) -> bool {
        block_range.clone().any(|visible_index| {
            self.document
                .visible_index
                .id_at_visible_index(visible_index)
                .is_some_and(|block_id| {
                    self.document
                        .payload_window
                        .payloads
                        .contains_key(&block_id)
                })
        })
    }

    pub(super) fn payloads_resident_for(&self, block_range: &Range<usize>) -> bool {
        block_range.clone().all(|visible_index| {
            self.document
                .visible_index
                .id_at_visible_index(visible_index)
                .is_some_and(|block_id| {
                    self.document
                        .payload_window
                        .payloads
                        .contains_key(&block_id)
                })
        })
    }

    pub(super) fn payloads_resident_for_prefetch_cached(
        &mut self,
        block_range: &Range<usize>,
    ) -> bool {
        let structure_version = self.document.visible_index.source_structure_version;
        let visibility_version = self.document.visible_index.visibility_version;
        let residency_revision = self.document.payload_window.residency_revision();
        if let Some(probe) = self.layout.payload_prefetch_residency_probe.as_ref()
            && probe.structure_version == structure_version
            && probe.visibility_version == visibility_version
            && probe.residency_revision == residency_revision
            && (probe.block_range == *block_range
                || (probe.resident
                    && probe.block_range.start <= block_range.start
                    && block_range.end <= probe.block_range.end))
        {
            return probe.resident;
        }

        let resident = self.payloads_resident_for(block_range);
        self.layout.payload_prefetch_residency_probe =
            Some(super::super::layout_state::PayloadResidencyProbe {
                block_range: block_range.clone(),
                structure_version,
                visibility_version,
                residency_revision,
                resident,
            });
        resident
    }

    pub(super) fn payload_terminal_failure_for(&self, block_range: &Range<usize>) -> bool {
        block_range.clone().any(|visible_index| {
            self.document
                .visible_index
                .id_at_visible_index(visible_index)
                .is_some_and(|block_id| {
                    self.document.payload_window.failed.contains_key(&block_id)
                        && !self.document.payload_window.can_retry(block_id)
                })
        })
    }

    pub(super) fn payload_failure_view_for(
        &self,
        block_range: &Range<usize>,
    ) -> Option<crate::projection::view::PayloadWindowFailureView> {
        block_range.clone().find_map(|visible_index| {
            let block_id = self
                .document
                .visible_index
                .id_at_visible_index(visible_index)?;
            let message = self.document.payload_window.failed.get(&block_id)?.clone();
            let attempts = self
                .document
                .payload_window
                .failure_attempts
                .get(&block_id)
                .copied()
                .unwrap_or(0);
            Some(crate::projection::view::PayloadWindowFailureView {
                message,
                attempts,
                max_attempts: crate::content::payload_window::MAX_PAYLOAD_WINDOW_LOAD_ATTEMPTS,
                automatic_retry_pending: self.document.payload_window.can_retry(block_id),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefetch_residency_probe_invalidates_when_a_payload_becomes_resident() {
        let records = (1..=2)
            .map(|block_id| {
                BlockIndexRecord::new(
                    block_id,
                    None,
                    0,
                    kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                    0,
                )
            })
            .collect();
        let mut runtime = DocumentRuntime::from_index_records_with_window(
            1,
            records,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                "one",
            )],
            1,
            720.0,
            0..1,
        );

        assert!(!runtime.payloads_resident_for_prefetch_cached(&(0..2)));
        let first_revision = runtime
            .layout
            .payload_prefetch_residency_probe
            .as_ref()
            .unwrap()
            .residency_revision;

        runtime
            .document
            .payload_window
            .insert_loaded(BlockPayloadRecord::rich_text(
                2,
                RichBlockKind::Paragraph,
                "two",
            ));

        assert!(runtime.payloads_resident_for_prefetch_cached(&(0..2)));
        assert!(
            runtime
                .layout
                .payload_prefetch_residency_probe
                .as_ref()
                .unwrap()
                .residency_revision
                > first_revision
        );
    }

    #[test]
    fn prefetch_residency_probe_invalidates_when_folding_remaps_the_same_range() {
        let records = vec![
            BlockIndexRecord::new(
                1,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Toggle),
                0,
            ),
            BlockIndexRecord::new(
                2,
                Some(1),
                1,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            ),
            BlockIndexRecord::new(
                3,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            ),
        ];
        let mut runtime = DocumentRuntime::from_index_records_with_window(
            1,
            records,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Toggle, "toggle"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "child"),
            ],
            1,
            720.0,
            0..2,
        );

        assert!(runtime.payloads_resident_for_prefetch_cached(&(0..2)));
        let cached_visibility_version = runtime
            .layout
            .payload_prefetch_residency_probe
            .as_ref()
            .unwrap()
            .visibility_version;

        assert!(runtime.toggle_block_fold(1).unwrap());
        assert_eq!(runtime.document.visible_index.visible_block_ids, vec![1, 3]);
        assert!(!runtime.payloads_resident_for_prefetch_cached(&(0..2)));
        assert!(
            runtime
                .layout
                .payload_prefetch_residency_probe
                .as_ref()
                .unwrap()
                .visibility_version
                > cached_visibility_version
        );
    }

    #[test]
    fn prefetch_residency_probe_invalidates_when_structure_move_remaps_the_same_range() {
        let records = (1..=3)
            .map(|block_id| {
                BlockIndexRecord::new(
                    block_id,
                    None,
                    0,
                    kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                    0,
                )
            })
            .collect();
        let mut runtime = DocumentRuntime::from_index_records_with_window(
            1,
            records,
            vec![
                BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "one"),
                BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "two"),
            ],
            1,
            720.0,
            0..2,
        );

        assert!(runtime.payloads_resident_for_prefetch_cached(&(0..2)));
        let cached_structure_version = runtime
            .layout
            .payload_prefetch_residency_probe
            .as_ref()
            .unwrap()
            .structure_version;

        assert!(runtime.move_block_subtree_before(3, Some(1)).unwrap());
        assert_eq!(
            runtime.document.visible_index.visible_block_ids,
            vec![3, 1, 2]
        );
        assert!(!runtime.payloads_resident_for_prefetch_cached(&(0..2)));
        assert!(
            runtime
                .layout
                .payload_prefetch_residency_probe
                .as_ref()
                .unwrap()
                .structure_version
                > cached_structure_version
        );
    }
}
