use super::*;

/// Virtual document geometry, measurement convergence, and viewport state.
#[derive(Debug)]
pub(super) struct LayoutState {
    pub(super) height_index: BlockHeightIndex,
    pub(super) page_layout: PageLayoutIndex,
    pub(super) page_local_cache: HashMap<usize, PageLocalHeightIndex>,
    pub(super) scroll: VirtualScrollState,
    pub(super) table_horizontal_scroll_offsets: HashMap<BlockId, f32>,
    pub(super) payload_window_generation: u64,
    pub(super) payload_prefetch_residency_probe: Option<PayloadResidencyProbe>,
    pub(super) window_planner: WindowPlanner,
    pub(super) last_planned_scroll_top: f64,
    pub(super) window_plan_clock_ms: u64,
    pub(super) window_memory_pressure: WindowMemoryPressure,
    pub(super) projection: ProjectionState,
    pub(super) pending_measured_heights: HashMap<BlockId, PendingMeasuredHeight>,
    pub(super) dirty: bool,
    pub(super) scrollbar_drag: Option<ScrollbarDragSession>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PayloadResidencyProbe {
    pub(super) block_range: Range<usize>,
    pub(super) structure_version: u64,
    pub(super) visibility_version: u64,
    pub(super) residency_revision: u64,
    pub(super) resident: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ProjectionState {
    pub(super) window: ProjectionWindowState,
    pub(super) publication: ProjectionPublicationState,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ProjectionWindowState {
    pub(super) generation: u64,
    pub(super) desired: Option<ProjectionWindowTarget>,
    pub(super) preparing: Option<ProjectionWindowTarget>,
    pub(super) load_state: ProjectionWindowLoadState,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum ProjectionWindowDecision {
    Stable(ProjectionWindowTarget),
    ColdPlaceholder(ProjectionWindowTarget),
    FailedTarget {
        target: ProjectionWindowTarget,
        stable: Option<ProjectionWindowTarget>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ProjectionPublicationState {
    pub(super) next_frame_id: u64,
    pub(super) stable: Option<StableProjectionSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct StableProjectionSnapshot {
    pub(super) frame_id: u64,
    pub(super) target: ProjectionWindowTarget,
    pub(super) projection: EditorViewProjection,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ProjectionWindowTarget {
    pub(super) structure_version: u64,
    pub(super) page_range: Range<usize>,
    pub(super) block_range: Range<usize>,
    pub(super) visible_block_range: Range<usize>,
    pub(super) presented_scroll_top: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProjectionWindowLoadState {
    CurrentStable,
    PreparingNext,
    ColdPlaceholder,
    Failed,
}

impl Default for ProjectionState {
    fn default() -> Self {
        Self {
            window: ProjectionWindowState {
                generation: 0,
                desired: None,
                preparing: None,
                load_state: ProjectionWindowLoadState::ColdPlaceholder,
            },
            publication: ProjectionPublicationState {
                next_frame_id: 0,
                stable: None,
            },
        }
    }
}

impl ProjectionState {
    pub(super) fn generation(&self) -> u64 {
        self.window.generation
    }

    pub(super) fn reconcile(
        &mut self,
        desired: ProjectionWindowTarget,
        desired_ready: bool,
        stable_valid: bool,
        desired_failed: bool,
    ) -> ProjectionWindowDecision {
        let invalidated_stable = self.publication.stable.is_some() && !stable_valid;
        if !stable_valid {
            self.publication.stable = None;
        }

        let desired_changed = self
            .window
            .desired
            .as_ref()
            .is_none_or(|current| !current.same_window_as(&desired));
        if desired_changed || invalidated_stable {
            self.window.generation = self.window.generation.saturating_add(1);
        }
        self.window.desired = Some(desired.clone());

        if desired_ready {
            self.window.preparing = None;
            self.window.load_state = ProjectionWindowLoadState::CurrentStable;
            return ProjectionWindowDecision::Stable(desired);
        }

        self.window.preparing = Some(desired.clone());
        let stable = self
            .publication
            .stable
            .as_ref()
            .map(|snapshot| snapshot.target.clone());
        if desired_failed {
            self.window.preparing = None;
            self.window.load_state = ProjectionWindowLoadState::Failed;
            return ProjectionWindowDecision::FailedTarget {
                target: desired,
                stable,
            };
        }

        if let Some(stable) = stable {
            self.window.load_state = ProjectionWindowLoadState::PreparingNext;
            ProjectionWindowDecision::Stable(stable)
        } else {
            self.window.load_state = ProjectionWindowLoadState::ColdPlaceholder;
            ProjectionWindowDecision::ColdPlaceholder(desired)
        }
    }
}

impl ProjectionWindowTarget {
    fn same_window_as(&self, other: &Self) -> bool {
        self.structure_version == other.structure_version
            && self.page_range == other.page_range
            && self.block_range == other.block_range
            && self.visible_block_range == other.visible_block_range
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PendingMeasuredHeight {
    pub(super) content_version: u64,
    pub(super) height: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_preserves_height_page_and_scroll_convergence() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            (1..=10)
                .map(|block_id| {
                    BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, "")
                })
                .collect(),
            720.0,
        );
        runtime.sync_viewport_height(320.0).unwrap();
        let version = runtime.block_payload_record(1).unwrap().content_version;

        assert!(runtime.queue_measured_height(1, version, 96.0).unwrap());
        assert!(runtime.flush_pending_height_corrections().unwrap());

        assert_eq!(
            runtime.layout.height_index.total_height(),
            runtime.layout.page_layout.total_height()
        );
        assert_eq!(
            runtime.layout.scroll.model_total_height,
            runtime.scroll_extent_height(runtime.layout.page_layout.total_height())
        );
        assert!(runtime.layout.dirty);
        assert!(runtime.layout.pending_measured_heights.is_empty());

        let mut accumulator = ScrollAccumulator::default();
        accumulator.push_input(
            cditor_viewport::scroll::ScrollInput {
                delta_y: 48.0,
                mode: cditor_viewport::scroll::ScrollDeltaMode::Pixel,
                phase: cditor_viewport::scroll::ScrollPhase::Changed,
                device: cditor_viewport::scroll::ScrollDevice::Trackpad,
                timestamp: Instant::now(),
            },
            runtime.viewport_height(),
        );
        assert!(
            runtime
                .apply_scroll_accumulator_frame(&mut accumulator)
                .unwrap()
        );
        assert_eq!(runtime.global_scroll_top(), 48.0);
    }

    fn projection_target(start: usize, scroll_top: f64) -> ProjectionWindowTarget {
        ProjectionWindowTarget {
            structure_version: 1,
            page_range: start / 10..start / 10 + 1,
            block_range: start..start + 10,
            visible_block_range: start + 2..start + 6,
            presented_scroll_top: scroll_top,
        }
    }

    #[test]
    fn projection_state_keeps_stable_publication_while_next_target_prepares() {
        let mut state = ProjectionState::default();
        let stable = projection_target(0, 0.0);
        assert_eq!(
            state.reconcile(stable.clone(), true, false, false),
            ProjectionWindowDecision::Stable(stable.clone())
        );
        state.publication.stable = Some(StableProjectionSnapshot {
            frame_id: 0,
            target: stable.clone(),
            projection: DocumentRuntime::demo().projection_for_window(),
        });

        let desired = projection_target(100, 3_200.0);
        assert_eq!(
            state.reconcile(desired.clone(), false, true, false),
            ProjectionWindowDecision::Stable(stable)
        );
        assert_eq!(state.window.preparing, Some(desired));
        assert_eq!(
            state.window.load_state,
            ProjectionWindowLoadState::PreparingNext
        );
    }
}
