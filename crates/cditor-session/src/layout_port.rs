use std::ops::Range;

use cditor_core::ids::BlockId;
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode};
use cditor_runtime::DocumentRuntime;
use cditor_viewport::scroll::{
    ScrollAccumulator, ScrollInput, ScrollbarDragEnd, ScrollbarDragUpdate, ScrollbarPolicy,
    ScrollbarVisualState,
};

use crate::EditorSessionHandle;

const DEFAULT_SCROLLBAR_MIN_THUMB_HEIGHT: f64 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutScrollSnapshot {
    pub changed: bool,
    pub global_scroll_top: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutViewportSnapshot {
    pub global_scroll_top: f64,
    pub viewport_height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockLayoutContextSnapshot {
    pub content_version: Option<u64>,
    pub effective_height: f64,
    pub estimated_height: f64,
}

pub fn project_layout_viewport(runtime: &DocumentRuntime) -> LayoutViewportSnapshot {
    LayoutViewportSnapshot {
        global_scroll_top: runtime.global_scroll_top(),
        viewport_height: runtime.viewport_height(),
    }
}

pub fn project_block_layout_context(
    runtime: &DocumentRuntime,
    block_id: BlockId,
) -> Option<BlockLayoutContextSnapshot> {
    let layout = runtime.block_layout_meta(block_id)?;
    Some(BlockLayoutContextSnapshot {
        content_version: runtime.block_content_version(block_id),
        effective_height: layout.effective_height(),
        estimated_height: layout.estimated_height,
    })
}

pub fn project_measured_block_height(
    runtime: &mut DocumentRuntime,
    block_id: BlockId,
    content_version: u64,
    measured_height: f64,
) -> Result<bool, ProtocolError> {
    runtime
        .queue_measured_height(block_id, content_version, measured_height)
        .map_err(|message| layout_error(runtime, message))
}

pub fn project_apply_measured_block_height(
    runtime: &mut DocumentRuntime,
    block_id: BlockId,
    content_version: u64,
    measured_height: f64,
) -> Result<bool, ProtocolError> {
    runtime
        .apply_measured_height(block_id, content_version, measured_height)
        .map_err(|message| layout_error(runtime, message))
}

pub fn project_scroll_input_frame(
    runtime: &mut DocumentRuntime,
    accumulator: &mut ScrollAccumulator,
    input: ScrollInput,
) -> Result<LayoutScrollSnapshot, ProtocolError> {
    project_queue_scroll_input(runtime, accumulator, input);
    project_flush_scroll_input_frame(runtime, accumulator)
}

pub fn project_queue_scroll_input(
    runtime: &DocumentRuntime,
    accumulator: &mut ScrollAccumulator,
    input: ScrollInput,
) {
    accumulator.push_input(input, runtime.viewport_height());
}

pub fn project_flush_scroll_input_frame(
    runtime: &mut DocumentRuntime,
    accumulator: &mut ScrollAccumulator,
) -> Result<LayoutScrollSnapshot, ProtocolError> {
    let changed = runtime
        .apply_scroll_accumulator_frame(accumulator)
        .map_err(|message| layout_error(runtime, message))?;
    Ok(LayoutScrollSnapshot {
        changed,
        global_scroll_top: runtime.global_scroll_top(),
    })
}

pub fn project_scroll_by_delta(
    runtime: &mut DocumentRuntime,
    delta_y: f64,
) -> Result<LayoutScrollSnapshot, ProtocolError> {
    let before = runtime.global_scroll_top();
    runtime
        .scroll_by_delta(delta_y)
        .map_err(|message| layout_error(runtime, message))?;
    let global_scroll_top = runtime.global_scroll_top();
    Ok(LayoutScrollSnapshot {
        changed: (global_scroll_top - before).abs() > f64::EPSILON,
        global_scroll_top,
    })
}

pub fn project_scroll_focused_block_into_view(
    runtime: &mut DocumentRuntime,
) -> Result<LayoutScrollSnapshot, ProtocolError> {
    let changed = runtime
        .scroll_focused_block_into_view()
        .map_err(|message| layout_error(runtime, message))?;
    Ok(LayoutScrollSnapshot {
        changed,
        global_scroll_top: runtime.global_scroll_top(),
    })
}

pub fn project_scroll_to_block(
    runtime: &mut DocumentRuntime,
    block_id: BlockId,
    alignment: Option<f64>,
) -> Result<LayoutScrollSnapshot, ProtocolError> {
    let changed = runtime
        .scroll_to_block_with_alignment(block_id, alignment)
        .map_err(|message| layout_error(runtime, message))?;
    Ok(LayoutScrollSnapshot {
        changed,
        global_scroll_top: runtime.global_scroll_top(),
    })
}

pub fn project_begin_scrollbar_drag(runtime: &mut DocumentRuntime) -> ScrollbarVisualState {
    runtime.begin_scrollbar_drag(scrollbar_policy(runtime))
}

pub fn project_drag_scrollbar(
    runtime: &mut DocumentRuntime,
    thumb_top: f64,
) -> Result<Option<ScrollbarDragUpdate>, ProtocolError> {
    runtime
        .drag_scrollbar_to_thumb_top(scrollbar_policy(runtime), thumb_top)
        .map_err(|message| layout_error(runtime, message))
}

pub fn project_drag_scrollbar_to_ratio(
    runtime: &mut DocumentRuntime,
    ratio: f64,
) -> Result<Option<ScrollbarDragUpdate>, ProtocolError> {
    runtime
        .drag_scrollbar_to_ratio(scrollbar_policy(runtime), ratio)
        .map_err(|message| layout_error(runtime, message))
}

pub fn project_finish_scrollbar_drag(
    runtime: &mut DocumentRuntime,
) -> Result<Option<ScrollbarDragEnd>, ProtocolError> {
    runtime
        .finish_scrollbar_drag()
        .map_err(|message| layout_error(runtime, message))
}

fn scrollbar_policy(runtime: &DocumentRuntime) -> ScrollbarPolicy {
    ScrollbarPolicy {
        track_height: runtime.viewport_height().max(1.0),
        min_thumb_height: DEFAULT_SCROLLBAR_MIN_THUMB_HEIGHT,
        local_list_state_scrollbar_enabled: false,
    }
}

fn layout_error(runtime: &DocumentRuntime, message: String) -> ProtocolError {
    ProtocolError::new(ProtocolErrorCode::ApplyFailed, message).with_document(runtime.document_id())
}

impl EditorSessionHandle {
    pub fn layout_viewport(&self) -> Result<LayoutViewportSnapshot, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_layout_viewport(&session.runtime))
    }

    pub fn block_layout_context(
        &self,
        block_id: BlockId,
    ) -> Result<Option<BlockLayoutContextSnapshot>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| {
            ProtocolError::new(
                ProtocolErrorCode::Busy,
                "editor session is already processing a synchronous request",
            )
            .retryable()
        })?;
        Ok(project_block_layout_context(&session.runtime, block_id))
    }

    pub fn queue_measured_block_height(
        &self,
        block_id: BlockId,
        content_version: u64,
        measured_height: f64,
    ) -> Result<bool, ProtocolError> {
        project_measured_block_height(
            &mut self.try_session_mut()?.runtime,
            block_id,
            content_version,
            measured_height,
        )
    }

    /// Applies a user-committed geometry change before its visual preview is removed.
    pub fn apply_measured_block_height(
        &self,
        block_id: BlockId,
        content_version: u64,
        measured_height: f64,
    ) -> Result<bool, ProtocolError> {
        project_apply_measured_block_height(
            &mut self.try_session_mut()?.runtime,
            block_id,
            content_version,
            measured_height,
        )
    }

    pub fn apply_scroll_input_frame(
        &self,
        accumulator: &mut ScrollAccumulator,
        input: ScrollInput,
    ) -> Result<LayoutScrollSnapshot, ProtocolError> {
        project_scroll_input_frame(&mut self.try_session_mut()?.runtime, accumulator, input)
    }

    pub fn queue_scroll_input(
        &self,
        accumulator: &mut ScrollAccumulator,
        input: ScrollInput,
    ) -> Result<(), ProtocolError> {
        project_queue_scroll_input(&self.try_session_mut()?.runtime, accumulator, input);
        Ok(())
    }

    pub fn flush_scroll_input_frame(
        &self,
        accumulator: &mut ScrollAccumulator,
    ) -> Result<LayoutScrollSnapshot, ProtocolError> {
        project_flush_scroll_input_frame(&mut self.try_session_mut()?.runtime, accumulator)
    }

    pub fn request_scroll_delta(
        &self,
        delta_y: f64,
    ) -> Result<LayoutScrollSnapshot, ProtocolError> {
        project_scroll_by_delta(&mut self.try_session_mut()?.runtime, delta_y)
    }

    pub fn ensure_focused_block_visible(&self) -> Result<LayoutScrollSnapshot, ProtocolError> {
        project_scroll_focused_block_into_view(&mut self.try_session_mut()?.runtime)
    }

    pub fn scroll_to_block(
        &self,
        block_id: BlockId,
        alignment: Option<f64>,
    ) -> Result<LayoutScrollSnapshot, ProtocolError> {
        project_scroll_to_block(&mut self.try_session_mut()?.runtime, block_id, alignment)
    }

    pub fn start_scrollbar_drag(&self) -> Result<ScrollbarVisualState, ProtocolError> {
        Ok(project_begin_scrollbar_drag(
            &mut self.try_session_mut()?.runtime,
        ))
    }

    pub fn drag_scrollbar(
        &self,
        thumb_top: f64,
    ) -> Result<Option<ScrollbarDragUpdate>, ProtocolError> {
        project_drag_scrollbar(&mut self.try_session_mut()?.runtime, thumb_top)
    }

    pub fn drag_scrollbar_to_ratio(
        &self,
        ratio: f64,
    ) -> Result<Option<ScrollbarDragUpdate>, ProtocolError> {
        project_drag_scrollbar_to_ratio(&mut self.try_session_mut()?.runtime, ratio)
    }

    pub fn current_foreground_payload_range(&self) -> Result<Range<usize>, ProtocolError> {
        Ok(self
            .try_session_mut()?
            .runtime
            .current_foreground_payload_range())
    }

    pub fn end_scrollbar_drag(&self) -> Result<Option<ScrollbarDragEnd>, ProtocolError> {
        project_finish_scrollbar_drag(&mut self.try_session_mut()?.runtime)
    }
}

#[cfg(test)]
mod tests {
    use web_time::Instant;

    use cditor_viewport::scroll::{
        ScrollDeltaMode, ScrollDevice, ScrollPhase, WheelPipelineConfig,
    };

    use super::*;
    use crate::EditorSession;

    #[test]
    fn wheel_input_is_normalized_and_committed_in_one_session_request() {
        let handle = EditorSession::new(DocumentRuntime::large_mixed_demo(), false).into_handle();
        let mut accumulator = ScrollAccumulator::new(WheelPipelineConfig::default());
        let outcome = handle
            .apply_scroll_input_frame(
                &mut accumulator,
                ScrollInput {
                    delta_y: 120.0,
                    mode: ScrollDeltaMode::Pixel,
                    phase: ScrollPhase::Changed,
                    device: ScrollDevice::Trackpad,
                    timestamp: Instant::now(),
                },
            )
            .unwrap();

        assert!(outcome.changed);
        assert!(outcome.global_scroll_top > 0.0);
        assert_eq!(accumulator.received_inputs, 1);
        assert_eq!(accumulator.committed_frames, 1);
    }

    #[test]
    fn queued_wheel_inputs_are_coalesced_into_one_frame_commit() {
        let handle = EditorSession::new(DocumentRuntime::large_mixed_demo(), false).into_handle();
        let mut accumulator = ScrollAccumulator::new(WheelPipelineConfig::default());
        let before = handle.layout_viewport().unwrap().global_scroll_top;
        let started_at = Instant::now();

        for event in 0..4 {
            handle
                .queue_scroll_input(
                    &mut accumulator,
                    ScrollInput {
                        delta_y: 30.0,
                        mode: ScrollDeltaMode::Pixel,
                        phase: ScrollPhase::Changed,
                        device: ScrollDevice::Trackpad,
                        timestamp: started_at + std::time::Duration::from_millis(event),
                    },
                )
                .unwrap();
        }

        assert_eq!(handle.layout_viewport().unwrap().global_scroll_top, before);
        assert_eq!(accumulator.received_inputs, 4);
        assert_eq!(accumulator.committed_frames, 0);
        assert_eq!(accumulator.pending_delta_y, 120.0);

        let outcome = handle.flush_scroll_input_frame(&mut accumulator).unwrap();
        assert!(outcome.changed);
        assert!(outcome.global_scroll_top > before);
        assert_eq!(accumulator.pending_delta_y, 0.0);
        assert_eq!(accumulator.committed_frames, 1);
    }

    #[test]
    fn viewport_snapshot_owns_coordinate_conversion_inputs() {
        let handle = EditorSession::new(DocumentRuntime::large_mixed_demo(), false).into_handle();
        handle.request_scroll_delta(120.0).unwrap();

        let snapshot = handle.layout_viewport().unwrap();
        assert!(snapshot.global_scroll_top > 0.0);
        assert!(snapshot.viewport_height > 0.0);
    }

    #[test]
    fn measured_height_and_programmatic_scroll_stay_behind_layout_port() {
        let runtime = DocumentRuntime::large_mixed_demo();
        let block_id = runtime.visible_block_ids()[0];
        let last_block = *runtime.visible_block_ids().last().unwrap();
        let content_version = runtime
            .block_payload_record(block_id)
            .unwrap()
            .content_version;
        let handle = EditorSession::new(runtime, false).into_handle();

        assert!(
            handle
                .queue_measured_block_height(block_id, content_version, 240.0)
                .unwrap()
        );
        assert!(
            handle
                .apply_measured_block_height(block_id, content_version, 240.0)
                .unwrap()
        );
        assert!(
            !handle
                .apply_measured_block_height(block_id, content_version, 240.0)
                .unwrap()
        );
        assert!(
            handle
                .scroll_to_block(last_block, Some(0.0))
                .unwrap()
                .changed
        );
    }

    #[test]
    fn block_layout_context_reports_effective_height_after_measured_update() {
        let runtime = DocumentRuntime::large_mixed_demo();
        let block_id = runtime.visible_block_ids()[0];
        let content_version = runtime
            .block_payload_record(block_id)
            .unwrap()
            .content_version;
        let handle = EditorSession::new(runtime, false).into_handle();

        let before = handle.block_layout_context(block_id).unwrap().unwrap();
        assert_eq!(before.content_version, Some(content_version));
        assert_eq!(before.effective_height, before.estimated_height);

        assert!(
            handle
                .apply_measured_block_height(block_id, content_version, 240.0)
                .unwrap()
        );

        let after = handle.block_layout_context(block_id).unwrap().unwrap();
        assert_eq!(after.effective_height, 240.0);
        assert_eq!(after.estimated_height, before.estimated_height);
        assert!(handle.block_layout_context(999_999).unwrap().is_none());
    }

    #[test]
    fn invalid_measurement_returns_document_scoped_protocol_error() {
        let runtime = DocumentRuntime::empty();
        let document_id = runtime.document_id();
        let handle = EditorSession::new(runtime, false).into_handle();

        let error = handle
            .queue_measured_block_height(1, 1, f64::NAN)
            .unwrap_err();
        assert_eq!(error.code, ProtocolErrorCode::ApplyFailed);
        assert_eq!(error.document_id, Some(document_id));
    }

    #[test]
    fn scrollbar_drag_lifecycle_is_owned_by_session() {
        let handle = EditorSession::new(DocumentRuntime::large_mixed_demo(), false).into_handle();
        let visual = handle.start_scrollbar_drag().unwrap();
        assert!(visual.enabled);

        let update = handle.drag_scrollbar(visual.track_height / 2.0).unwrap();
        assert!(update.is_some());
        assert!(handle.end_scrollbar_drag().unwrap().is_some());
        assert!(handle.end_scrollbar_drag().unwrap().is_none());
    }
}
