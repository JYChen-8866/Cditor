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

pub fn project_scroll_input_frame(
    runtime: &mut DocumentRuntime,
    accumulator: &mut ScrollAccumulator,
    input: ScrollInput,
) -> Result<LayoutScrollSnapshot, ProtocolError> {
    accumulator.push_input(input, runtime.viewport_height());
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

    pub fn apply_scroll_input_frame(
        &self,
        accumulator: &mut ScrollAccumulator,
        input: ScrollInput,
    ) -> Result<LayoutScrollSnapshot, ProtocolError> {
        project_scroll_input_frame(&mut self.try_session_mut()?.runtime, accumulator, input)
    }

    pub fn scroll_by_delta(&self, delta_y: f64) -> Result<LayoutScrollSnapshot, ProtocolError> {
        project_scroll_by_delta(&mut self.try_session_mut()?.runtime, delta_y)
    }

    pub fn scroll_focused_block_into_view(&self) -> Result<LayoutScrollSnapshot, ProtocolError> {
        project_scroll_focused_block_into_view(&mut self.try_session_mut()?.runtime)
    }

    pub fn scroll_to_block(
        &self,
        block_id: BlockId,
        alignment: Option<f64>,
    ) -> Result<LayoutScrollSnapshot, ProtocolError> {
        project_scroll_to_block(&mut self.try_session_mut()?.runtime, block_id, alignment)
    }

    pub fn begin_scrollbar_drag(&self) -> Result<ScrollbarVisualState, ProtocolError> {
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

    pub fn finish_scrollbar_drag(&self) -> Result<Option<ScrollbarDragEnd>, ProtocolError> {
        project_finish_scrollbar_drag(&mut self.try_session_mut()?.runtime)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

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
                .scroll_to_block(last_block, Some(0.0))
                .unwrap()
                .changed
        );
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
        let visual = handle.begin_scrollbar_drag().unwrap();
        assert!(visual.enabled);

        let update = handle.drag_scrollbar(visual.track_height / 2.0).unwrap();
        assert!(update.is_some());
        assert!(handle.finish_scrollbar_drag().unwrap().is_some());
        assert!(handle.finish_scrollbar_drag().unwrap().is_none());
    }
}
