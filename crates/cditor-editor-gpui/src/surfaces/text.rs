use std::ops::Range;

use gpui::{Context, Pixels, Point, Window};

use cditor_core::edit::TextAffinity;
use cditor_core::ids::{BlockId, SurfaceId};
#[cfg(test)]
use cditor_runtime::DocumentRuntime;
use cditor_runtime::TextSurfaceSnapshot;
use cditor_session::SurfaceVersionSnapshot;

use crate::editor_view::{CditorV2View, CditorViewState};
use crate::input::trace::trace_input;
use crate::interaction::geometry::FallbackViewportOrigin;
use crate::text::{
    ParleySelectionKind, ParleyTextPosition, RichTextElement, RichTextLayoutInput,
    RichTextPlatformLayout, platform_text_position_for_point, record_synchronous_geometry_fallback,
    record_unavailable_geometry, text_geometry_telemetry,
};

pub(crate) fn selection_kind_for_click_count(click_count: usize) -> Option<ParleySelectionKind> {
    match click_count {
        0 | 1 => None,
        2 => Some(ParleySelectionKind::Word),
        _ => Some(ParleySelectionKind::Line),
    }
}

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
    pub(crate) fn current_text_layout_cache(
        &self,
        current: SurfaceVersionSnapshot,
        block_id: BlockId,
    ) -> Option<&RichTextPlatformLayout> {
        let cache = self.cache.text_layouts.get(&block_id)?;
        layout_cache_is_current(cache, current).then_some(cache)
    }

    pub(crate) fn current_text_surface_layout_cache(
        &self,
        current: SurfaceVersionSnapshot,
    ) -> Option<&RichTextPlatformLayout> {
        match current.surface_id {
            SurfaceId::Block(block_id) => self.current_text_layout_cache(current, block_id),
            SurfaceId::TableCell {
                block_id,
                row,
                column,
            } => self.current_table_cell_layout_cache(current, block_id, row, column),
            SurfaceId::ImageCaption { .. } => super::caption::current_layout(self, current),
            SurfaceId::CollectionTitle { .. } => {
                super::collection_title::current_layout(self, current)
            }
            SurfaceId::Ephemeral { .. } => None,
        }
    }

    pub(crate) fn text_position_for_surface_at_position(
        &self,
        surface_id: SurfaceId,
        position: Point<Pixels>,
    ) -> Option<ParleyTextPosition> {
        let session = self.ready_session()?;
        let current = session.surface_version(surface_id).ok().flatten()?;
        let cache = self.current_text_surface_layout_cache(current)?;
        Some(platform_text_position_for_point(cache, position))
    }

    pub(crate) fn text_position_for_block_at_position(
        &self,
        block_id: BlockId,
        position: Point<Pixels>,
    ) -> Option<ParleyTextPosition> {
        let session = self.ready_session()?;
        let current = session
            .surface_version(SurfaceId::Block(block_id))
            .ok()
            .flatten()?;
        if let Some(cache) = self.current_text_layout_cache(current, block_id) {
            return Some(platform_text_position_for_point(cache, position));
        }
        record_synchronous_geometry_fallback();
        let fallback =
            self.fallback_text_position_for_block_at_position(session, block_id, position);
        if fallback.is_none() {
            record_unavailable_geometry();
        }
        trace_input(
            "geometry.sync_layout_fallback",
            format_args!(
                "block={block_id} available={} telemetry={:?}",
                fallback.is_some(),
                text_geometry_telemetry()
            ),
        );
        fallback
    }

    fn fallback_text_position_for_block_at_position(
        &self,
        session: &cditor_session::EditorSessionHandle,
        block_id: BlockId,
        position: Point<Pixels>,
    ) -> Option<ParleyTextPosition> {
        let rect = self
            .interaction
            .projected_block_rects
            .iter()
            .find(|rect| rect.block_id == block_id)?;
        let viewport_origin = self.infer_document_viewport_origin()?;
        let payload = session.loaded_payload_record(block_id).ok().flatten()?;
        let spans = match &payload.payload {
            cditor_core::rich_text::BlockPayload::RichText { spans } => spans.clone(),
            cditor_core::rich_text::BlockPayload::Code { text, .. } => {
                vec![cditor_core::rich_text::InlineSpan::plain(text)]
            }
            cditor_core::rich_text::BlockPayload::Html { html, .. } => {
                vec![cditor_core::rich_text::InlineSpan::plain(html)]
            }
            _ => return Some(ParleyTextPosition::downstream(0)),
        };
        let text = cditor_core::rich_text::plain_text_from_spans(&spans);
        if text.is_empty() {
            return Some(ParleyTextPosition::downstream(0));
        }
        let hit_point = fallback_text_hit_point(
            position,
            viewport_origin,
            rect.document_top,
            rect.text_origin_x_in_block_px,
            rect.text_origin_y_in_block_px,
            session.layout_viewport().ok()?.global_scroll_top,
        );
        let input = RichTextLayoutInput {
            block_id,
            surface_id: crate::text::TextLayoutSurfaceId::Block(block_id),
            content_version: payload.content_version,
            layout_version: session
                .surface_version(SurfaceId::Block(block_id))
                .ok()
                .flatten()?
                .layout_version,
            kind: payload.kind,
            text_align: cditor_core::rich_text::TextAlign::Start,
            spans,
            width_px: rect.text_width_px,
            theme_version: 1,
            font_version: 1,
        };
        Some(
            RichTextElement::new(input, crate::theme::GuiTheme::light())
                .hit_test_position(hit_point),
        )
    }

    pub(crate) fn infer_document_viewport_origin(&self) -> Option<FallbackViewportOrigin> {
        let session = self.ready_session()?;
        let focused = session
            .document_snapshot()
            .ok()
            .and_then(|snapshot| snapshot.focused_block_id)
            .and_then(|block_id| {
                viewport_origin_for_block(
                    session,
                    &self.interaction.projected_block_rects,
                    &self.cache.text_layouts,
                    block_id,
                )
            });
        focused.or_else(|| {
            self.interaction
                .projected_block_rects
                .iter()
                .find_map(|rect| {
                    viewport_origin_for_block(
                        session,
                        &self.interaction.projected_block_rects,
                        &self.cache.text_layouts,
                        rect.block_id,
                    )
                })
        })
    }
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
        let focused = state.focused && !self.status.readonly;
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
        if self.status.readonly {
            return;
        }
        let hit = self
            .text_position_for_surface_at_position(surface_id, position)
            .map(|position| position.offset);
        let click_selection = if let Some(kind) = selection_kind_for_click_count(click_count) {
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

        window.focus(&self.focus.editor, cx);
        self.interaction.table_interaction_mode = Default::default();
        self.overlay.table_menu_ui = Default::default();
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
                    self.status.save_status =
                        crate::persistence::EditorSaveStatus::Failed(error.to_string());
                    cx.notify();
                }
            }
        }
    }
}

fn viewport_origin_for_block(
    session: &cditor_session::EditorSessionHandle,
    rects: &[crate::interaction::geometry::ProjectedBlockRect],
    layouts: &std::collections::HashMap<BlockId, RichTextPlatformLayout>,
    block_id: BlockId,
) -> Option<FallbackViewportOrigin> {
    let cache = layouts.get(&block_id)?;
    let rect = rects.iter().find(|rect| rect.block_id == block_id)?;
    if session
        .surface_version(SurfaceId::Block(block_id))
        .ok()
        .flatten()?
        .content_version
        != cache.content_version
    {
        return None;
    }
    Some(FallbackViewportOrigin {
        x: f32::from(cache.bounds.left()) as f64 - rect.text_origin_x_in_block_px,
        y: f32::from(cache.bounds.top()) as f64 - rect.document_top
            + session.layout_viewport().ok()?.global_scroll_top
            - rect.text_origin_y_in_block_px,
    })
}

pub(crate) fn layout_cache_is_current(
    cache: &RichTextPlatformLayout,
    current: SurfaceVersionSnapshot,
) -> bool {
    cache.surface_id == current.surface_id
        && cache.content_version == current.content_version
        && cache.layout_version == current.layout_version
}

pub(crate) fn fallback_text_hit_point(
    position: Point<Pixels>,
    viewport_origin: FallbackViewportOrigin,
    document_top: f64,
    text_origin_x_in_block_px: f64,
    text_origin_y_in_block_px: f64,
    global_scroll_top: f64,
) -> crate::text::TextHitPoint {
    let text_origin_x = viewport_origin.x + text_origin_x_in_block_px;
    let text_origin_y =
        viewport_origin.y + document_top - global_scroll_top + text_origin_y_in_block_px;
    crate::text::TextHitPoint {
        x: f32::from(position.x) as f64 - text_origin_x,
        y: f32::from(position.y) as f64 - text_origin_y,
    }
}

#[cfg(test)]
mod tests {
    use cditor_core::rich_text::{BlockPayload, BlockPayloadRecord, RichBlockKind};
    use cditor_runtime::TableCellPosition;
    use gpui::{AppContext, Bounds, Size, TestAppContext, point, px};

    use super::*;

    #[test]
    fn fallback_text_hit_point_accounts_for_scroll_and_text_origin() {
        let hit = fallback_text_hit_point(
            point(px(180.0), px(260.0)),
            FallbackViewportOrigin { x: 100.0, y: 40.0 },
            500.0,
            32.0,
            12.0,
            320.0,
        );

        assert_eq!(hit.x, 48.0);
        assert_eq!(hit.y, 28.0);
    }

    #[test]
    fn layout_cache_rejects_stale_surface_content_and_layout_identity() {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord {
                block_id: 1,
                content_version: 1,
                kind: RichBlockKind::Table,
                payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                    rows: vec![cditor_core::rich_text::TableRowPayload {
                        cells: vec![cditor_core::rich_text::TableCellPayload::plain("cell")],
                        height: Default::default(),
                    }],
                    columns: Vec::new(),
                    header_rows: 0,
                    header_cols: 0,
                    header_style: Default::default(),
                }),
            }],
            720.0,
        );
        crate::test_support::focus_table_cell_at_offset(&mut runtime, 1, 0, 0, 4);
        crate::test_support::replace_realtime_text(&mut runtime, None, "\nmore");
        let current_version = runtime.block_content_version(1).unwrap();
        let stale_cache = crate::text::test_platform_layout(
            1,
            current_version.saturating_sub(1),
            "cell",
            Bounds {
                origin: point(px(10.0), px(20.0)),
                size: Size {
                    width: px(120.0),
                    height: px(36.0),
                },
            },
            Some(TableCellPosition { row: 0, col: 0 }),
        );
        let surface_id = SurfaceId::TableCell {
            block_id: 1,
            row: 0,
            column: 0,
        };
        let current = cditor_session::project_surface_version(&runtime, surface_id).unwrap();
        assert!(!layout_cache_is_current(&stale_cache, current));
        let mut current_cache = crate::text::test_platform_layout(
            1,
            current_version,
            "cell\nmore",
            Bounds {
                origin: point(px(10.0), px(20.0)),
                size: Size {
                    width: px(120.0),
                    height: px(88.0),
                },
            },
            Some(TableCellPosition { row: 0, col: 0 }),
        );
        current_cache.layout_version = current.layout_version;

        assert!(layout_cache_is_current(&current_cache, current));
        assert!(!layout_cache_is_current(
            &current_cache,
            SurfaceVersionSnapshot {
                layout_version: current.layout_version.saturating_add(1),
                ..current
            }
        ));
        assert!(!layout_cache_is_current(
            &current_cache,
            SurfaceVersionSnapshot {
                surface_id: SurfaceId::Block(1),
                ..current
            }
        ));
    }

    #[test]
    fn click_count_maps_single_to_caret_double_to_word_and_triple_to_line() {
        assert_eq!(selection_kind_for_click_count(1), None);
        assert_eq!(
            selection_kind_for_click_count(2),
            Some(ParleySelectionKind::Word)
        );
        assert_eq!(
            selection_kind_for_click_count(3),
            Some(ParleySelectionKind::Line)
        );
        assert_eq!(
            selection_kind_for_click_count(5),
            Some(ParleySelectionKind::Line)
        );
    }

    #[gpui::test]
    fn render_state_projects_caption_snapshot_and_focus_session(cx: &mut TestAppContext) {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord {
                block_id: 10,
                content_version: 3,
                kind: RichBlockKind::Image,
                payload: BlockPayload::Image(cditor_core::rich_text::ImagePayload {
                    caption: "caption".into(),
                    ..Default::default()
                }),
            }],
            720.0,
        );
        let surface_id = super::super::caption::surface_id(10);
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
