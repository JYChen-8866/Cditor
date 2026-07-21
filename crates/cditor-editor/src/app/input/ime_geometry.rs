use std::ops::Range;

use gpui::{Bounds, Context, Pixels, Point, Size, Window, px};

use super::ime_support::{
    ai_prompt_input_target_allows, code_language_input_target_allows,
    platform_input_geometry_allows, platform_input_target_allows, table_menu_input_target_allows,
};
use crate::app::cditor_v2_view::CditorV2View;
use crate::app::input_trace::trace_input;
use crate::block::table::menu::TABLE_MENU_SEARCH_FONT_SIZE_PX;
use crate::input::ime::utf8_to_utf16_offset;
use crate::input::ime::utf16_range_to_utf8_range;
use crate::input::{SINGLE_LINE_INPUT_FONT_SIZE_PX, single_line_visible_range_x};
use crate::text::{platform_index_for_point, platform_range_bounds, record_unavailable_geometry};
use cditor_core::ids::SurfaceId;
use cditor_runtime::{DocumentRuntime, InputTarget};

impl CditorV2View {
    pub(in crate::app) fn ime_character_index_for_text_surface(
        &self,
        runtime: &DocumentRuntime,
        surface_id: SurfaceId,
        point: Point<Pixels>,
        text: &str,
    ) -> Option<usize> {
        let Some(cache) = self.current_text_surface_layout_cache(runtime, surface_id) else {
            record_unavailable_geometry();
            return None;
        };
        if !platform_input_geometry_allows(
            self.platform_input_target,
            self.platform_input_session_identity,
            self.platform_input_layout_identity,
            runtime,
            cache,
        ) {
            record_unavailable_geometry();
            return None;
        }
        let utf8 = platform_index_for_point(cache, point).min(text.len());
        Some(utf8_to_utf16_offset(text, utf8))
    }

    pub(in crate::app) fn ime_bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if let Some(selection) = self.table_interaction_mode.axis_selection() {
            if !table_menu_input_target_allows(self.platform_input_target, selection.block_id) {
                return None;
            }
            let range = utf16_range_to_utf8_range(&self.table_menu_ui.query, &range_utf16);
            let x_range = single_line_visible_range_x(
                &self.table_menu_ui.query,
                range,
                self.table_menu_ui.caret_offset,
                px(TABLE_MENU_SEARCH_FONT_SIZE_PX),
                element_bounds,
                _window,
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
        if self.ai_prompt_focus.is_focused(_window) {
            let registered_target = self.platform_input_target;
            let prompt = self.ai_prompt.as_ref()?;
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
                _window,
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
        if self.code_language_focus.is_focused(_window) {
            let registered_target = self.platform_input_target;
            let edit = self.code_language_edit.as_ref()?;
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
                _window,
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
        let runtime = self.ready_runtime_ref()?;
        if !platform_input_target_allows(
            self.platform_input_target,
            self.platform_input_session_identity,
            runtime,
        ) {
            trace_input(
                "bounds_for_range.rejected_target",
                format_args!(
                    "registered={:?} runtime={:?}",
                    self.platform_input_target,
                    runtime.input_session_target()
                ),
            );
            return None;
        }
        let (block_id, text) = runtime.focused_text_for_platform_input()?;
        let range = utf16_range_to_utf8_range(&text, &range_utf16);
        match runtime.input_session_target()? {
            InputTarget::TableCell {
                block_id: target_block_id,
                row,
                col,
            } if target_block_id == block_id => {
                let Some(cache) = self.current_table_cell_layout_cache(runtime, block_id, row, col)
                else {
                    record_unavailable_geometry();
                    return None;
                };
                if !platform_input_geometry_allows(
                    self.platform_input_target,
                    self.platform_input_session_identity,
                    self.platform_input_layout_identity,
                    runtime,
                    cache,
                ) {
                    record_unavailable_geometry();
                    return None;
                }
                Some(platform_range_bounds(cache, range))
            }
            InputTarget::BlockText {
                block_id: target_block_id,
            } if target_block_id == block_id => {
                let Some(cache) = self.current_text_layout_cache(runtime, block_id) else {
                    record_unavailable_geometry();
                    return None;
                };
                if !platform_input_geometry_allows(
                    self.platform_input_target,
                    self.platform_input_session_identity,
                    self.platform_input_layout_identity,
                    runtime,
                    cache,
                ) {
                    record_unavailable_geometry();
                    return None;
                }
                Some(platform_range_bounds(cache, range))
            }
            target @ (InputTarget::ImageCaption {
                block_id: target_block_id,
            }
            | InputTarget::CollectionTitle {
                block_id: target_block_id,
            }) if target_block_id == block_id => {
                let surface_id = target.surface_id()?;
                let Some(cache) = self.current_text_surface_layout_cache(runtime, surface_id)
                else {
                    record_unavailable_geometry();
                    return None;
                };
                if !platform_input_geometry_allows(
                    self.platform_input_target,
                    self.platform_input_session_identity,
                    self.platform_input_layout_identity,
                    runtime,
                    cache,
                ) {
                    record_unavailable_geometry();
                    return None;
                }
                Some(platform_range_bounds(cache, range))
            }
            _ => None,
        }
    }
}
