use std::ops::Range;

use gpui::{Bounds, Context, Pixels, Point, Size, Window, px};

use super::ime::{
    ai_prompt_input_target_allows, code_language_input_target_allows, platform_input_target_allows,
};
use crate::gui::app::cditor_v2_view::CditorV2View;
use crate::gui::app::input_trace::trace_input;
use crate::gui::input::ime::utf16_range_to_utf8_range;
use crate::gui::input::{SINGLE_LINE_INPUT_FONT_SIZE_PX, single_line_visible_range_x};
use crate::gui::text::platform_range_bounds;
use cditor_runtime::InputTarget;

impl CditorV2View {
    pub(in crate::gui::app) fn ime_bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
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
        if !platform_input_target_allows(self.platform_input_target, runtime) {
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
                let cache = self.current_table_cell_layout_cache(runtime, block_id, row, col)?;
                platform_range_bounds(cache, range).or(Some(Bounds {
                    origin: element_bounds.origin,
                    size: Size {
                        width: px(1.0),
                        height: px(24.0),
                    },
                }))
            }
            InputTarget::BlockText {
                block_id: target_block_id,
            } if target_block_id == block_id => {
                let cache = self.current_text_layout_cache(runtime, block_id)?;
                platform_range_bounds(cache, range).or(Some(Bounds {
                    origin: element_bounds.origin,
                    size: Size {
                        width: px(1.0),
                        height: px(24.0),
                    },
                }))
            }
            _ => None,
        }
    }
}
