use std::ops::Range;

use gpui::{Bounds, Context, Pixels, Point, Size, Window, px};

use super::support::{
    ai_prompt_input_target_allows, code_language_input_target_allows,
    platform_input_geometry_allows, platform_input_target_allows, table_menu_input_target_allows,
};
use crate::editor_view::{CditorV2View, PlatformImeCandidateBounds};
use crate::features::table::menu::TABLE_MENU_SEARCH_FONT_SIZE_PX;
use crate::input::ime::utf8_to_utf16_offset;
use crate::input::ime::utf16_range_to_utf8_range;
use crate::input::trace::trace_input;
use crate::input::{SINGLE_LINE_INPUT_FONT_SIZE_PX, single_line_visible_range_x};
use crate::text::{
    TextHitPoint, platform_range_bounds_at, platform_text_position_for_local_point,
    record_unavailable_geometry,
};
use cditor_core::ids::SurfaceId;
use cditor_runtime::InputTarget;
use cditor_session::EditorSessionHandle;

impl CditorV2View {
    pub(crate) fn ime_character_index_for_text_surface(
        &self,
        session: &EditorSessionHandle,
        surface_id: SurfaceId,
        point: Point<Pixels>,
        text: &str,
    ) -> Option<usize> {
        let input_context = session.input_context().ok()?;
        let current = session.surface_version(surface_id).ok().flatten()?;
        let Some(cache) = self.current_text_surface_layout_cache(current) else {
            record_unavailable_geometry();
            return None;
        };
        if !platform_input_geometry_allows(
            self.input.target,
            self.input.session_identity,
            self.input.layout_identity,
            &input_context,
            cache,
        ) {
            record_unavailable_geometry();
            return None;
        }
        let bounds = self.input.element_bounds?;
        let utf8 = platform_text_position_for_local_point(
            cache,
            TextHitPoint {
                x: f64::from(point.x - bounds.left()),
                y: f64::from(point.y - bounds.top()),
            },
        )
        .offset
        .min(text.len());
        Some(utf8_to_utf16_offset(text, utf8))
    }

    pub(crate) fn ime_bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let registered_target = self.input.target;
        if let Some(bounds) = self
            .precise_ime_bounds_for_range(range_utf16, element_bounds, window)
            .filter(|bounds| ime_candidate_bounds_are_usable(*bounds))
        {
            if let Some(target) = registered_target {
                self.input.candidate_bounds = Some(PlatformImeCandidateBounds {
                    target,
                    bounds,
                    element_bounds,
                });
            }
            return Some(bounds);
        }

        let cached = self
            .input
            .candidate_bounds
            .filter(|candidate| registered_target.is_none_or(|target| target == candidate.target))
            .and_then(|candidate| translated_candidate_bounds(candidate, element_bounds));
        let fallback = cached.unwrap_or_else(|| ime_candidate_fallback_bounds(element_bounds));
        trace_input(
            "bounds_for_range.stable_fallback",
            format_args!(
                "registered={registered_target:?} cached={} element_bounds={element_bounds:?} fallback={fallback:?}",
                cached.is_some()
            ),
        );
        Some(fallback)
    }

    fn precise_ime_bounds_for_range(
        &self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
    ) -> Option<Bounds<Pixels>> {
        if let Some(selection) = self.interaction.table_interaction_mode.axis_selection() {
            if !table_menu_input_target_allows(self.input.target, selection.block_id) {
                return None;
            }
            let range = utf16_range_to_utf8_range(&self.overlay.table_menu_ui.query, &range_utf16);
            let x_range = single_line_visible_range_x(
                &self.overlay.table_menu_ui.query,
                range,
                self.overlay.table_menu_ui.caret_offset,
                px(TABLE_MENU_SEARCH_FONT_SIZE_PX),
                element_bounds,
                window,
            );
            return Some(Bounds {
                origin: Point {
                    x: element_bounds.origin.x + px(x_range.start),
                    y: element_bounds.origin.y,
                },
                size: Size {
                    width: px((x_range.end - x_range.start).max(1.0)),
                    height: element_bounds.size.height,
                },
            });
        }
        if self.focus.ai_prompt.is_focused(window) {
            let registered_target = self.input.target;
            let prompt = self.overlay.ai_prompt.as_ref()?;
            if !ai_prompt_input_target_allows(registered_target, prompt.block_id) {
                return None;
            }
            let range = utf16_range_to_utf8_range(&prompt.draft, &range_utf16);
            let x_range = single_line_visible_range_x(
                &prompt.draft,
                range,
                prompt.caret_offset,
                px(SINGLE_LINE_INPUT_FONT_SIZE_PX),
                element_bounds,
                window,
            );
            return Some(Bounds {
                origin: gpui::point(
                    element_bounds.left() + px(x_range.start),
                    element_bounds.top(),
                ),
                size: Size {
                    width: px((x_range.end - x_range.start).max(1.0)),
                    height: element_bounds.size.height,
                },
            });
        }
        if self.focus.link_edit.is_focused(window) {
            let registered_target = self.input.target;
            let edit = self.overlay.link_edit.as_ref()?;
            if !super::support::link_edit_input_target_allows(registered_target, edit.block_id) {
                return None;
            }
            let draft = edit.active_draft();
            let range = utf16_range_to_utf8_range(draft, &range_utf16);
            let x_range = single_line_visible_range_x(
                draft,
                range,
                edit.caret_offset,
                px(SINGLE_LINE_INPUT_FONT_SIZE_PX),
                element_bounds,
                window,
            );
            return Some(Bounds {
                origin: Point {
                    x: element_bounds.origin.x + px(x_range.start),
                    y: element_bounds.origin.y,
                },
                size: Size {
                    width: px((x_range.end - x_range.start).max(1.0)),
                    height: element_bounds.size.height.max(px(22.0)),
                },
            });
        }
        if self.focus.code_language.is_focused(window) {
            let registered_target = self.input.target;
            let edit = self.overlay.code_language_edit.as_ref()?;
            if !code_language_input_target_allows(registered_target, edit.block_id) {
                trace_input(
                    "bounds_for_range.code_language_rejected_target",
                    format_args!("registered={:?} block={}", registered_target, edit.block_id),
                );
                return None;
            }
            let range = utf16_range_to_utf8_range(&edit.draft, &range_utf16);
            let x_range = single_line_visible_range_x(
                &edit.draft,
                range,
                edit.caret_offset,
                px(SINGLE_LINE_INPUT_FONT_SIZE_PX),
                element_bounds,
                window,
            );
            return Some(Bounds {
                origin: Point {
                    x: element_bounds.origin.x + px(x_range.start),
                    y: element_bounds.origin.y,
                },
                size: Size {
                    width: px((x_range.end - x_range.start).max(1.0)),
                    height: element_bounds.size.height.max(px(22.0)),
                },
            });
        }
        let session = self.ready_session()?;
        let input_context = session.input_context().ok()?;
        if !platform_input_target_allows(
            self.input.target,
            self.input.session_identity,
            &input_context,
        ) {
            trace_input(
                "bounds_for_range.rejected_target",
                format_args!(
                    "registered={:?} runtime={:?}",
                    self.input.target, input_context.target
                ),
            );
            return None;
        }
        let focused = input_context.focused_text.as_ref()?;
        let (block_id, text) = (focused.block_id, &focused.text);
        let range = utf16_range_to_utf8_range(text, &range_utf16);
        let target = input_context.target?;
        let surface_id = target.surface_id()?;
        let current = session.surface_version(surface_id).ok().flatten()?;
        trace_input(
            "bounds_for_range.query",
            format_args!(
                "target={target:?} range_utf16={range_utf16:?} range_utf8={range:?} element_bounds={element_bounds:?} registered_session={:?} runtime_session={:?} registered_layout={:?} surface_version={current:?}",
                self.input.session_identity, input_context.identity, self.input.layout_identity,
            ),
        );
        let result = match target {
            InputTarget::TableCell {
                block_id: target_block_id,
                row,
                col,
            } if target_block_id == block_id => {
                let cached = self
                    .resolved_table_cell_text_geometry(current, block_id, row, col)
                    .filter(|geometry| {
                        platform_input_geometry_allows(
                            self.input.target,
                            self.input.session_identity,
                            self.input.layout_identity,
                            &input_context,
                            geometry.layout(),
                        )
                    })
                    .map(|geometry| geometry.bounds_for_range(range.clone()));
                let result = cached.or_else(|| {
                    trace_input(
                        "bounds_for_range.table_cell.sync_fallback",
                        format_args!("block={block_id} row={row} col={col}"),
                    );
                    self.synchronous_text_range_bounds_for_table_cell(
                        session, current, block_id, row, col, range,
                    )
                });
                if result.is_none() {
                    record_unavailable_geometry();
                }
                trace_input(
                    "bounds_for_range.table_cell.result",
                    format_args!("block={block_id} row={row} col={col} bounds={result:?}"),
                );
                result
            }
            InputTarget::BlockText {
                block_id: target_block_id,
            } if target_block_id == block_id => {
                let cached = self
                    .projected_text_geometry_for_block(current, block_id)
                    .filter(|geometry| {
                        platform_input_geometry_allows(
                            self.input.target,
                            self.input.session_identity,
                            self.input.layout_identity,
                            &input_context,
                            geometry.layout(),
                        )
                    })
                    .map(|geometry| geometry.bounds_for_range(range.clone()));
                let result = cached.or_else(|| {
                    trace_input(
                        "bounds_for_range.block_text.sync_fallback",
                        format_args!("block={block_id}"),
                    );
                    self.synchronous_text_range_bounds_for_block(session, block_id, current, range)
                });
                if result.is_none() {
                    record_unavailable_geometry();
                }
                trace_input(
                    "bounds_for_range.block_text.result",
                    format_args!("block={block_id} bounds={result:?}"),
                );
                result
            }
            InputTarget::ImageCaption {
                block_id: target_block_id,
            }
            | InputTarget::CollectionTitle {
                block_id: target_block_id,
            } if target_block_id == block_id => {
                let Some(cache) = self.current_text_surface_layout_cache(current) else {
                    record_unavailable_geometry();
                    return None;
                };
                if !platform_input_geometry_allows(
                    self.input.target,
                    self.input.session_identity,
                    self.input.layout_identity,
                    &input_context,
                    cache,
                ) {
                    record_unavailable_geometry();
                    return None;
                }
                Some(platform_range_bounds_at(
                    cache,
                    range,
                    element_bounds.origin,
                ))
            }
            _ => None,
        };
        trace_input(
            "bounds_for_range.result",
            format_args!("target={target:?} bounds={result:?}"),
        );
        result
    }
}

fn translated_candidate_bounds(
    candidate: PlatformImeCandidateBounds,
    element_bounds: Bounds<Pixels>,
) -> Option<Bounds<Pixels>> {
    if !ime_candidate_bounds_are_usable(candidate.bounds)
        || !bounds_origin_is_finite(candidate.element_bounds)
        || !bounds_origin_is_finite(element_bounds)
    {
        return None;
    }
    Some(Bounds {
        origin: Point {
            x: candidate.bounds.origin.x
                + (element_bounds.origin.x - candidate.element_bounds.origin.x),
            y: candidate.bounds.origin.y
                + (element_bounds.origin.y - candidate.element_bounds.origin.y),
        },
        size: candidate.bounds.size,
    })
}

fn ime_candidate_fallback_bounds(element_bounds: Bounds<Pixels>) -> Bounds<Pixels> {
    let left = finite_pixels_or(element_bounds.origin.x, px(1.0));
    let top = finite_pixels_or(element_bounds.origin.y, px(1.0));
    let element_height = f32::from(element_bounds.size.height);
    let height = if element_height.is_finite() && element_height > 0.0 {
        element_height.min(24.0)
    } else {
        1.0
    };
    Bounds {
        origin: Point { x: left, y: top },
        size: Size {
            width: px(1.0),
            height: px(height),
        },
    }
}

fn ime_candidate_bounds_are_usable(bounds: Bounds<Pixels>) -> bool {
    bounds_origin_is_finite(bounds)
        && f32::from(bounds.size.width).is_finite()
        && f32::from(bounds.size.height).is_finite()
        && bounds.size.width > px(0.0)
        && bounds.size.height > px(0.0)
}

fn bounds_origin_is_finite(bounds: Bounds<Pixels>) -> bool {
    f32::from(bounds.origin.x).is_finite() && f32::from(bounds.origin.y).is_finite()
}

fn finite_pixels_or(value: Pixels, fallback: Pixels) -> Pixels {
    f32::from(value)
        .is_finite()
        .then_some(value)
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_view::GuiPlatformInputTarget;
    use gpui::{point, px, size};

    fn candidate(
        bounds: Bounds<Pixels>,
        element_bounds: Bounds<Pixels>,
    ) -> PlatformImeCandidateBounds {
        PlatformImeCandidateBounds {
            target: GuiPlatformInputTarget::BlockText { block_id: 1 },
            bounds,
            element_bounds,
        }
    }

    #[test]
    fn fallback_bounds_are_positive_even_when_element_geometry_is_empty() {
        let bounds = ime_candidate_fallback_bounds(Bounds::default());

        assert!(ime_candidate_bounds_are_usable(bounds));
        assert_eq!(bounds.size.width, px(1.0));
        assert_eq!(bounds.size.height, px(1.0));
    }

    #[test]
    fn cached_candidate_bounds_follow_element_translation() {
        let cached_element = Bounds::new(point(px(10.0), px(20.0)), size(px(300.0), px(24.0)));
        let cached = candidate(
            Bounds::new(point(px(30.0), px(40.0)), size(px(1.0), px(20.0))),
            cached_element,
        );
        let current_element = Bounds::new(point(px(16.0), px(27.0)), size(px(300.0), px(24.0)));

        let translated = translated_candidate_bounds(cached, current_element).unwrap();

        assert_eq!(translated.origin, point(px(36.0), px(47.0)));
        assert_eq!(translated.size, size(px(1.0), px(20.0)));
    }

    #[test]
    fn invalid_cached_geometry_is_discarded_instead_of_reaching_platform_ime() {
        let element = Bounds::new(point(px(10.0), px(20.0)), size(px(300.0), px(24.0)));
        let cached = candidate(
            Bounds::new(point(px(f32::NAN), px(40.0)), size(px(1.0), px(20.0))),
            element,
        );

        assert!(translated_candidate_bounds(cached, element).is_none());
    }
}
