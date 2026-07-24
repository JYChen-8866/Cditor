use std::ops::Range;

use cditor_core::edit::TextAffinity;
use cditor_core::ids::SurfaceId;
use cditor_runtime::TextSurfaceSnapshot;
use gpui::{Context, Pixels, Point, Window};

use crate::app::cditor_v2_view::{CditorV2View, CditorViewState};

#[derive(Debug, Clone)]
pub(crate) struct TextSurfaceRenderState {
    pub snapshot: TextSurfaceSnapshot,
    pub focused: bool,
    pub caret_offset: Option<usize>,
    pub caret_affinity: TextAffinity,
    pub selection_range: Option<Range<usize>>,
    pub marked_range: Option<Range<usize>>,
}

impl CditorV2View {
    pub(crate) fn text_surface_render_state(
        &self,
        surface_id: SurfaceId,
    ) -> Option<TextSurfaceRenderState> {
        let CditorViewState::Ready(session) = &self.state else {
            return None;
        };
        let state = session.text_surface_state(surface_id).ok().flatten()?;
        let focused = state.focused && !self.readonly;
        let selection_range = state.selection_range.filter(|range| !range.is_empty());
        Some(TextSurfaceRenderState {
            snapshot: state.snapshot,
            focused,
            caret_offset: focused.then_some(state.caret_offset).flatten(),
            caret_affinity: state.caret_affinity,
            selection_range,
            marked_range: state.marked_range,
        })
    }

    pub(crate) fn focus_text_surface_from_gui_at_position(
        &mut self,
        surface_id: SurfaceId,
        position: Point<Pixels>,
        click_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.readonly {
            return;
        }
        let hit = self
            .text_position_for_surface_at_position(surface_id, position)
            .map(|position| position.offset);
        let click_selection =
            if let Some(kind) = crate::app::text_hit::selection_kind_for_click_count(click_count) {
                self.ready_session()
                    .and_then(|session| session.surface_version(surface_id).ok().flatten())
                    .and_then(|current| self.current_text_surface_layout_cache(current))
                    .map(|cache| {
                        let local_x = f32::from(position.x - cache.bounds.left());
                        let local_y = f32::from(position.y - cache.bounds.top());
                        cache.snapshot.selection_at_point(local_x, local_y, kind)
                    })
            } else {
                None
            };
        let fallback = self
            .ready_session()
            .and_then(|session| session.text_surface_state(surface_id).ok().flatten())
            .map(|state| state.snapshot.len())
            .unwrap_or_default();

        window.focus(&self.focus, cx);
        self.table_interaction_mode = Default::default();
        self.table_menu_ui = Default::default();
        self.clear_gutter_action();
        if let Some(session) = self.ready_session() {
            let command = if let Some(selection) = click_selection {
                cditor_editor_protocol::command::CditorCommand::SetTextSurfaceSelection {
                    surface_id,
                    anchor_offset: selection.anchor.offset,
                    focus_offset: selection.focus.offset,
                    focus_affinity: selection.focus.affinity,
                }
            } else {
                let offset = hit.unwrap_or(fallback);
                cditor_editor_protocol::command::CditorCommand::SetTextSurfaceSelection {
                    surface_id,
                    anchor_offset: offset,
                    focus_offset: offset,
                    focus_affinity: TextAffinity::Downstream,
                }
            };
            let focus_result =
                session.dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
                    command,
                    cditor_editor_protocol::command::CommandSource::Toolbar,
                ));
            match focus_result {
                Ok(_) => cx.notify(),
                Err(error) => {
                    self.save_status =
                        crate::persistence::EditorSaveStatus::Failed(error.to_string());
                    cx.notify();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::ids::SurfaceId;
    use cditor_core::rich_text::{BlockPayload, BlockPayloadRecord, ImagePayload, RichBlockKind};
    use cditor_runtime::DocumentRuntime;
    use gpui::{AppContext, TestAppContext};

    use super::*;

    #[gpui::test]
    fn render_state_projects_caption_snapshot_and_focus_session(cx: &mut TestAppContext) {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord {
                block_id: 10,
                content_version: 3,
                kind: RichBlockKind::Image,
                payload: BlockPayload::Image(ImagePayload {
                    caption: "caption".into(),
                    ..Default::default()
                }),
            }],
            720.0,
        );
        let surface_id = SurfaceId::ImageCaption { block_id: 10 };
        crate::test_support::focus_text_surface_at_offset(&mut runtime, surface_id, 2);
        let view = cx.new(|cx| CditorV2View::from_runtime(runtime, false, cx));

        view.update(cx, |view, _cx| {
            let state = view.text_surface_render_state(surface_id).unwrap();
            assert!(state.focused);
            assert_eq!(state.caret_offset, Some(2));
            assert_eq!(state.snapshot.plain_text(), "caption");
            assert_eq!(state.snapshot.identity.content_version, 3);
        });
    }
}
