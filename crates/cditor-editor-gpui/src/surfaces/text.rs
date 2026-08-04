use std::ops::Range;

use gpui::{Bounds, Context, Pixels, Point, Window, point, px};

use cditor_core::edit::TextAffinity;
use cditor_core::ids::{BlockId, SurfaceId};
#[cfg(test)]
use cditor_runtime::DocumentRuntime;
use cditor_runtime::TextSurfaceSnapshot;
use cditor_session::SurfaceVersionSnapshot;

use crate::editor_view::{CditorV2View, CditorViewState};
use crate::input::trace::trace_input;
use crate::interaction::geometry::{DocumentViewportOrigin, ProjectedTextPlacement};
use crate::text::{
    RichTextElement, RichTextLayoutInput, RichTextPlatformLayout, RichTextTypography, TextHitPoint,
    TextLayoutPosition, TextLayoutSelection, TextLayoutSelectionKind, platform_range_bounds_at,
    platform_text_position_for_local_point, record_synchronous_geometry_fallback,
    record_unavailable_geometry, text_geometry_telemetry,
};

pub(crate) fn selection_kind_for_click_count(
    click_count: usize,
) -> Option<TextLayoutSelectionKind> {
    match click_count {
        0 | 1 => None,
        2 => Some(TextLayoutSelectionKind::Word),
        _ => Some(TextLayoutSelectionKind::Line),
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

/// A version-checked text snapshot paired with its current window placement.
/// Parley owns local geometry; this placement is the only transform between
/// that geometry and document window coordinates.
pub(crate) struct ResolvedProjectedTextGeometry<'a> {
    layout: &'a RichTextPlatformLayout,
    placement: ProjectedTextPlacement,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TextSurfaceInteractionGeometry {
    placement: ProjectedTextPlacement,
    typography: RichTextTypography,
}

impl TextSurfaceInteractionGeometry {
    pub(crate) fn from_bounds(
        bounds: Bounds<Pixels>,
        wrap_width_px: f64,
        text_align: cditor_core::rich_text::TextAlign,
        typography: RichTextTypography,
    ) -> Self {
        Self {
            placement: ProjectedTextPlacement {
                window_origin_x_px: f64::from(bounds.left()),
                window_origin_y_px: f64::from(bounds.top()),
                wrap_width_px,
                text_align,
            },
            typography,
        }
    }
}

impl ResolvedProjectedTextGeometry<'_> {
    pub(crate) fn new(
        layout: &RichTextPlatformLayout,
        placement: ProjectedTextPlacement,
    ) -> Option<ResolvedProjectedTextGeometry<'_>> {
        layout
            .matches_text_constraints(placement.wrap_width_px, placement.text_align)
            .then_some(ResolvedProjectedTextGeometry { layout, placement })
    }

    pub(crate) const fn layout(&self) -> &RichTextPlatformLayout {
        self.layout
    }

    pub(crate) fn position_for_window_point(&self, position: Point<Pixels>) -> TextLayoutPosition {
        platform_text_position_for_local_point(
            self.layout,
            projected_text_hit_point(self.placement, position),
        )
    }

    pub(crate) fn selection_at_window_point(
        &self,
        position: Point<Pixels>,
        kind: TextLayoutSelectionKind,
    ) -> TextLayoutSelection {
        let point = projected_text_hit_point(self.placement, position);
        self.layout
            .snapshot
            .selection_at_point(point.x as f32, point.y as f32, kind)
    }

    pub(crate) fn bounds_for_range(&self, range: Range<usize>) -> Bounds<Pixels> {
        platform_range_bounds_at(self.layout, range, self.window_origin())
    }

    fn window_origin(&self) -> Point<Pixels> {
        point(
            px(self.placement.window_origin_x_px as f32),
            px(self.placement.window_origin_y_px as f32),
        )
    }
}

impl CditorV2View {
    pub(crate) fn current_text_layout_cache(
        &self,
        current: SurfaceVersionSnapshot,
        block_id: BlockId,
    ) -> Option<&RichTextPlatformLayout> {
        let cache = self.cache.text_layouts.get(&block_id)?;
        let rect = self
            .interaction
            .projected_block_rects
            .iter()
            .find(|r| r.block_id == block_id);
        let width = rect.map(|r| r.text_width_px);
        let align = rect.and_then(|r| r.text_align);
        layout_cache_is_current(cache, current, width, align).then_some(cache)
    }

    pub(crate) fn projected_text_geometry_for_block(
        &self,
        current: SurfaceVersionSnapshot,
        block_id: BlockId,
    ) -> Option<ResolvedProjectedTextGeometry<'_>> {
        let placement = self.projected_text_placement_for_block(block_id)?;
        let layout = self.current_text_layout_cache(current, block_id)?;
        ResolvedProjectedTextGeometry::new(layout, placement)
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

    #[cfg(test)]
    pub(crate) fn text_position_for_surface_at_position(
        &self,
        surface_id: SurfaceId,
        position: Point<Pixels>,
    ) -> Option<TextLayoutPosition> {
        if let SurfaceId::Block(block_id) = surface_id {
            return self.text_position_for_block_at_position(block_id, position);
        }
        if let SurfaceId::TableCell {
            block_id,
            row,
            column,
        } = surface_id
        {
            return self.text_position_for_table_cell_at_position(block_id, row, column, position);
        }
        None
    }

    fn text_position_for_auxiliary_surface_at_position(
        &self,
        surface_id: SurfaceId,
        position: Point<Pixels>,
        geometry: TextSurfaceInteractionGeometry,
    ) -> Option<TextLayoutPosition> {
        if !matches!(
            surface_id,
            SurfaceId::ImageCaption { .. } | SurfaceId::CollectionTitle { .. }
        ) {
            return None;
        }
        let session = self.ready_session()?;
        let current = session.surface_version(surface_id).ok().flatten()?;
        let hit_point = projected_text_hit_point(geometry.placement, position);
        if let Some(cache) = self.current_text_surface_layout_cache(current)
            && cache.matches_text_constraints(
                geometry.placement.wrap_width_px,
                geometry.placement.text_align,
            )
        {
            return Some(platform_text_position_for_local_point(cache, hit_point));
        }

        record_synchronous_geometry_fallback();
        let element =
            cold_text_element_for_auxiliary_surface(session, surface_id, current, geometry)?;
        if element.input.spans.is_empty() {
            return Some(TextLayoutPosition::downstream(0));
        }
        Some(element.hit_test_position(hit_point))
    }

    fn text_selection_for_auxiliary_surface_at_position(
        &self,
        surface_id: SurfaceId,
        position: Point<Pixels>,
        kind: TextLayoutSelectionKind,
        geometry: TextSurfaceInteractionGeometry,
    ) -> Option<TextLayoutSelection> {
        let session = self.ready_session()?;
        let current = session.surface_version(surface_id).ok().flatten()?;
        let hit_point = projected_text_hit_point(geometry.placement, position);
        if let Some(cache) = self.current_text_surface_layout_cache(current)
            && cache.matches_text_constraints(
                geometry.placement.wrap_width_px,
                geometry.placement.text_align,
            )
        {
            return Some(cache.snapshot.selection_at_point(
                hit_point.x as f32,
                hit_point.y as f32,
                kind,
            ));
        }
        let element =
            cold_text_element_for_auxiliary_surface(session, surface_id, current, geometry)?;
        Some(element.selection_at_point(hit_point, kind))
    }

    pub(crate) fn text_position_for_block_at_position(
        &self,
        block_id: BlockId,
        position: Point<Pixels>,
    ) -> Option<TextLayoutPosition> {
        let session = self.ready_session()?;
        let current = session
            .surface_version(SurfaceId::Block(block_id))
            .ok()
            .flatten()?;
        let placement = self.projected_text_placement_for_block(block_id)?;
        let hit_point = projected_text_hit_point(placement, position);
        if let Some(geometry) = self.projected_text_geometry_for_block(current, block_id) {
            return Some(geometry.position_for_window_point(position));
        }
        record_synchronous_geometry_fallback();
        let fallback = self.fallback_text_position_for_block_at_position(
            session, block_id, current, placement, hit_point,
        );
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
        current: SurfaceVersionSnapshot,
        placement: ProjectedTextPlacement,
        hit_point: TextHitPoint,
    ) -> Option<TextLayoutPosition> {
        let element = cold_text_element_for_block(session, block_id, current, placement)?;
        if element.input.spans.is_empty() {
            return Some(TextLayoutPosition::downstream(0));
        }
        Some(element.hit_test_position(hit_point))
    }

    pub(crate) fn document_viewport_origin(&self) -> Option<DocumentViewportOrigin> {
        let bounds = self.interaction.editor_viewport_handle.bounds();
        let width = f32::from(bounds.size.width);
        let height = f32::from(bounds.size.height);
        if width > 0.5 && height > 0.5 {
            let viewport =
                crate::menu_metrics::EditorViewport::from_measurement(bounds, bounds.size);
            let document = self.document_layout_metrics(viewport.width);
            return Some(DocumentViewportOrigin::from_layout(viewport, document));
        }
        self.interaction.document_viewport_origin
    }

    pub(crate) fn sync_document_viewport_origin(
        &mut self,
        viewport: crate::menu_metrics::EditorViewport,
        document: crate::document::DocumentLayoutMetrics,
    ) {
        self.interaction.document_viewport_origin =
            Some(DocumentViewportOrigin::from_layout(viewport, document));
    }

    pub(crate) fn projected_text_placement_for_block(
        &self,
        block_id: BlockId,
    ) -> Option<ProjectedTextPlacement> {
        let rect = self
            .interaction
            .projected_block_rects
            .iter()
            .find(|r| r.block_id == block_id)?;
        let viewport_origin = self.document_viewport_origin()?;
        let internal_scroll = if rect.has_internal_text_scroll {
            self.interaction
                .code_scroll_handles
                .get(&block_id)
                .map(|h| f64::from(h.offset().y))
                .unwrap_or(0.0)
        } else {
            0.0
        };
        Some(ProjectedTextPlacement::for_block(
            viewport_origin,
            *rect,
            self.interaction.presented_scroll_top,
            internal_scroll,
        ))
    }

    pub(crate) fn text_range_bounds_for_block(
        &self,
        block_id: BlockId,
        range: Range<usize>,
    ) -> Option<Bounds<Pixels>> {
        let session = self.ready_session()?;
        let current = session
            .surface_version(SurfaceId::Block(block_id))
            .ok()
            .flatten()?;
        if let Some(geometry) = self.projected_text_geometry_for_block(current, block_id) {
            return Some(geometry.bounds_for_range(range));
        }
        self.synchronous_text_range_bounds_for_block(session, block_id, current, range)
    }

    pub(crate) fn synchronous_text_range_bounds_for_block(
        &self,
        session: &cditor_session::EditorSessionHandle,
        block_id: BlockId,
        current: SurfaceVersionSnapshot,
        range: Range<usize>,
    ) -> Option<Bounds<Pixels>> {
        record_synchronous_geometry_fallback();
        let placement = self.projected_text_placement_for_block(block_id)?;
        let element = cold_text_element_for_block(session, block_id, current, placement)?;
        let local_rects = if range.is_empty() {
            vec![element.local_caret_rect_for_offset(range.start)]
        } else {
            element.local_rects_for_range(range)
        };
        projected_bounds_for_local_rects(placement, local_rects)
    }

    pub(crate) fn text_caret_bounds_for_block(
        &self,
        block_id: BlockId,
        offset: usize,
    ) -> Option<Bounds<Pixels>> {
        self.text_range_bounds_for_block(block_id, offset..offset)
    }

    pub(crate) fn text_range_contains_block_position(
        &self,
        block_id: BlockId,
        range: Range<usize>,
        position: Point<Pixels>,
    ) -> bool {
        if range.is_empty() {
            return false;
        }
        let Some(session) = self.ready_session() else {
            return false;
        };
        let Some(current) = session
            .surface_version(SurfaceId::Block(block_id))
            .ok()
            .flatten()
        else {
            return false;
        };
        let Some(placement) = self.projected_text_placement_for_block(block_id) else {
            return false;
        };
        let hit_point = projected_text_hit_point(placement, position);
        let rects = if let Some(geometry) =
            self.projected_text_geometry_for_block(current, block_id)
        {
            geometry.layout().snapshot.range_rects(range)
        } else {
            record_synchronous_geometry_fallback();
            let Some(element) = cold_text_element_for_block(session, block_id, current, placement)
            else {
                return false;
            };
            element.local_rects_for_range(range)
        };
        rects.into_iter().any(|rect| {
            hit_point.x >= f64::from(rect.x)
                && hit_point.x <= f64::from(rect.x + rect.width)
                && hit_point.y >= f64::from(rect.y)
                && hit_point.y <= f64::from(rect.y + rect.height)
        })
    }

    pub(crate) fn text_selection_for_block_at_position(
        &self,
        block_id: BlockId,
        position: Point<Pixels>,
        kind: TextLayoutSelectionKind,
    ) -> Option<TextLayoutSelection> {
        let session = self.ready_session()?;
        let current = session
            .surface_version(SurfaceId::Block(block_id))
            .ok()
            .flatten()?;
        let placement = self.projected_text_placement_for_block(block_id)?;
        let hit_point = projected_text_hit_point(placement, position);
        if let Some(geometry) = self.projected_text_geometry_for_block(current, block_id) {
            return Some(geometry.selection_at_window_point(position, kind));
        }
        let payload = session.loaded_payload_record(block_id).ok().flatten()?;
        let input = block_text_layout_input(&payload, current, placement)?;
        Some(
            RichTextElement::new(input, crate::theme::GuiTheme::light())
                .selection_at_point(hit_point, kind),
        )
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
        geometry: TextSurfaceInteractionGeometry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.status.readonly {
            return;
        }
        let hit = match surface_id {
            SurfaceId::Block(block_id) => {
                self.text_position_for_block_at_position(block_id, position)
            }
            _ => {
                self.text_position_for_auxiliary_surface_at_position(surface_id, position, geometry)
            }
        }
        .map(|position| position.offset);
        let click_selection = if let Some(kind) = selection_kind_for_click_count(click_count) {
            match surface_id {
                SurfaceId::Block(block_id) => {
                    self.text_selection_for_block_at_position(block_id, position, kind)
                }
                _ => self.text_selection_for_auxiliary_surface_at_position(
                    surface_id, position, kind, geometry,
                ),
            }
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

pub(crate) fn projected_text_hit_point(
    placement: ProjectedTextPlacement,
    position: Point<Pixels>,
) -> TextHitPoint {
    let (x, y) = placement.local_point(f64::from(position.x), f64::from(position.y));
    TextHitPoint { x, y }
}

fn block_text_layout_input(
    payload: &cditor_core::rich_text::BlockPayloadRecord,
    current: SurfaceVersionSnapshot,
    placement: ProjectedTextPlacement,
) -> Option<RichTextLayoutInput> {
    if payload.content_version != current.content_version {
        return None;
    }
    let spans = match &payload.payload {
        cditor_core::rich_text::BlockPayload::RichText { spans } => spans.clone(),
        cditor_core::rich_text::BlockPayload::Code { text, .. } => {
            vec![cditor_core::rich_text::InlineSpan::plain(text)]
        }
        cditor_core::rich_text::BlockPayload::Html { html, .. } => {
            vec![cditor_core::rich_text::InlineSpan::plain(html)]
        }
        _ => return None,
    };
    Some(RichTextLayoutInput {
        block_id: payload.block_id,
        surface_id: crate::text::TextLayoutSurfaceId::Block(payload.block_id),
        content_version: payload.content_version,
        layout_version: current.layout_version,
        kind: payload.kind.clone(),
        text_align: placement.text_align,
        spans: spans.into(),
        width_px: placement.wrap_width_px,
        theme_version: 1,
        font_version: 1,
    })
}

fn cold_text_element_for_block(
    session: &cditor_session::EditorSessionHandle,
    block_id: BlockId,
    current: SurfaceVersionSnapshot,
    placement: ProjectedTextPlacement,
) -> Option<RichTextElement> {
    let payload = session.loaded_payload_record(block_id).ok().flatten()?;
    let input = block_text_layout_input(&payload, current, placement)?;
    Some(RichTextElement::new(input, crate::theme::GuiTheme::light()))
}

fn cold_text_element_for_auxiliary_surface(
    session: &cditor_session::EditorSessionHandle,
    surface_id: SurfaceId,
    current: SurfaceVersionSnapshot,
    geometry: TextSurfaceInteractionGeometry,
) -> Option<RichTextElement> {
    let state = session.text_surface_state(surface_id).ok().flatten()?;
    if state.snapshot.identity.content_version != current.content_version {
        return None;
    }
    let input = RichTextLayoutInput::from_text_surface_snapshot(
        state.snapshot,
        current.layout_version,
        geometry.placement.text_align,
        geometry.placement.wrap_width_px,
        1,
        1,
    );
    Some(
        RichTextElement::new(input, crate::theme::GuiTheme::light())
            .with_typography(geometry.typography),
    )
}

pub(super) fn projected_bounds_for_local_rects(
    placement: ProjectedTextPlacement,
    rects: Vec<crate::text::TextLayoutRect>,
) -> Option<Bounds<Pixels>> {
    let mut rects = rects.into_iter().map(|rect| {
        Bounds::new(
            point(
                px((placement.window_origin_x_px + f64::from(rect.x)) as f32),
                px((placement.window_origin_y_px + f64::from(rect.y)) as f32),
            ),
            gpui::size(px(rect.width.max(1.0)), px(rect.height.max(0.0))),
        )
    });
    let first = rects.next()?;
    Some(rects.fold(first, |union, rect| {
        Bounds::from_corners(
            point(union.left().min(rect.left()), union.top().min(rect.top())),
            point(
                union.right().max(rect.right()),
                union.bottom().max(rect.bottom()),
            ),
        )
    }))
}

pub(crate) fn layout_cache_is_current(
    cache: &RichTextPlatformLayout,
    current: SurfaceVersionSnapshot,
    wrap_width_px: Option<f64>,
    text_align: Option<cditor_core::rich_text::TextAlign>,
) -> bool {
    if cache.surface_id != current.surface_id
        || cache.content_version != current.content_version
        || cache.layout_version != current.layout_version
    {
        return false;
    }
    if let Some(w) = wrap_width_px {
        cache.matches_text_constraints(w, text_align.unwrap_or(cache.text_align))
    } else {
        true
    }
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
