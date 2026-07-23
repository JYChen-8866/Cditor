use std::ops::Range;

use cditor_core::{ids::SurfaceId, rich_text::BlockPayloadRecord};
use cditor_editor_protocol::{ProtocolError, ProtocolErrorCode, projection::ProjectionRequest};
use cditor_runtime::{EditorViewProjection, content::payload_window::PayloadWindowLoadRequest};
use cditor_viewport::scroll::{
    scrollbar::{ScrollbarPolicy, ScrollbarVisualState},
    wheel::HeightCorrectionPriority,
};

use crate::EditorSessionHandle;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderFrameRequest {
    pub viewport_height: f64,
    pub include_diagnostics: bool,
    pub height_correction_priority: HeightCorrectionPriority,
    pub min_scrollbar_thumb_height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderFrameSnapshot {
    pub projection: EditorViewProjection,
    pub automatic_text_layout_pins: Vec<SurfaceId>,
    pub scrollbar_visual: ScrollbarVisualState,
    pub warnings: RenderFrameWarnings,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderFrameWarnings {
    pub viewport_sync_error: Option<String>,
    pub height_correction_error: Option<String>,
}

impl EditorSessionHandle {
    pub fn render_frame(
        &self,
        request: RenderFrameRequest,
    ) -> Result<RenderFrameSnapshot, ProtocolError> {
        let mut session = self.try_session_mut()?;
        Ok(project_render_frame(&mut session.runtime, request))
    }
}

pub fn project_render_frame(
    runtime: &mut cditor_runtime::DocumentRuntime,
    request: RenderFrameRequest,
) -> RenderFrameSnapshot {
    let viewport_sync_error = runtime.sync_viewport_height(request.viewport_height).err();
    let height_correction_error = runtime
        .flush_pending_height_corrections_with_priority(request.height_correction_priority)
        .err();
    let automatic_text_layout_pins = runtime
        .input_session_target()
        .and_then(|target| target.surface_id())
        .into_iter()
        .collect();
    let revision = runtime.revision();
    let projection = runtime.projection(ProjectionRequest {
        viewport_revision: revision,
        include_diagnostics: request.include_diagnostics,
    });
    let scrollbar_visual = runtime.scrollbar_visual_state(ScrollbarPolicy {
        track_height: request.viewport_height.max(1.0),
        min_thumb_height: request.min_scrollbar_thumb_height,
        local_list_state_scrollbar_enabled: false,
    });
    RenderFrameSnapshot {
        projection,
        automatic_text_layout_pins,
        scrollbar_visual,
        warnings: RenderFrameWarnings {
            viewport_sync_error,
            height_correction_error,
        },
    }
}

impl EditorSessionHandle {
    pub fn set_table_horizontal_scroll_offset(
        &self,
        block_id: cditor_core::ids::BlockId,
        offset_x: f32,
    ) -> Result<bool, ProtocolError> {
        self.try_session_mut()?
            .runtime
            .set_table_horizontal_scroll_offset_px(block_id, offset_x)
            .map_err(render_error)
    }

    pub fn activate_resident_payload_window(
        &self,
        block_range: Range<usize>,
    ) -> Result<bool, ProtocolError> {
        Ok(self
            .try_session_mut()?
            .runtime
            .activate_payload_window_if_resident(block_range))
    }

    pub fn plan_payload_window_load(
        &self,
        block_range: Range<usize>,
    ) -> Result<Option<PayloadWindowLoadRequest>, ProtocolError> {
        Ok(self
            .try_session_mut()?
            .runtime
            .plan_payload_window_load_if_needed(block_range))
    }

    pub fn loaded_payload_record(
        &self,
        block_id: cditor_core::ids::BlockId,
    ) -> Result<Option<BlockPayloadRecord>, ProtocolError> {
        let session = self.inner.try_borrow().map_err(|_| session_busy())?;
        Ok(session.runtime.block_payload_record(block_id))
    }
}

fn render_error(message: String) -> ProtocolError {
    ProtocolError::new(ProtocolErrorCode::ApplyFailed, message)
}

fn session_busy() -> ProtocolError {
    ProtocolError::new(
        ProtocolErrorCode::Busy,
        "editor session is already processing a synchronous request",
    )
    .retryable()
}

#[cfg(test)]
mod tests {
    use cditor_runtime::DocumentRuntime;

    use super::*;
    use crate::EditorSession;

    #[test]
    fn render_frame_uses_one_revision_for_projection_and_session_snapshot() {
        let handle = EditorSession::new(DocumentRuntime::demo(), false).into_handle();
        let frame = handle
            .render_frame(RenderFrameRequest {
                viewport_height: 720.0,
                include_diagnostics: false,
                height_correction_priority: HeightCorrectionPriority::Normal,
                min_scrollbar_thumb_height: 24.0,
            })
            .unwrap();

        assert_eq!(
            frame.projection.viewport_revision,
            handle.snapshot().unwrap().revision
        );
        assert_eq!(frame.scrollbar_visual.track_height, 720.0);
        assert_eq!(frame.projection.scroll.viewport_height, 720.0);
        assert_eq!(frame.warnings, RenderFrameWarnings::default());
    }
}
