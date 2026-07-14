use std::ops::Range;
use std::time::Duration;

use cditor_core::rich_text::{InlineColorTarget, InlineMark, InlineSpan};
use cditor_runtime::DocumentRuntime;

use crate::gui::diagnostics::block_color::trace as trace_block_color;
use crate::gui::overlay::{ActiveColor, ColorMenuAction, PaletteColor};

use super::super::CditorV2View;

pub(super) fn selected_spans_color(
    spans: &[InlineSpan],
    range: Range<usize>,
    target: InlineColorTarget,
) -> ActiveColor {
    let mut offset = 0usize;
    let mut selected_value: Option<Option<&str>> = None;
    for span in spans {
        let span_range = offset..offset + span.text.len();
        offset = span_range.end;
        if span_range.start >= range.end || span_range.end <= range.start {
            continue;
        }
        let values = span
            .marks
            .iter()
            .filter(|mark| target.matches(mark))
            .filter_map(|mark| match mark {
                InlineMark::Color(value) | InlineMark::Background(value) => Some(value.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let value = match values.as_slice() {
            [] => None,
            [value] => Some(*value),
            values if values.windows(2).all(|pair| pair[0] == pair[1]) => Some(values[0]),
            _ => return ActiveColor::Mixed,
        };
        match selected_value {
            None => selected_value = Some(value),
            Some(current) if current == value => {}
            Some(_) => return ActiveColor::Mixed,
        }
    }
    match selected_value.flatten() {
        None => ActiveColor::Default,
        Some(value) => PaletteColor::from_value(target, value)
            .map(ActiveColor::Palette)
            .unwrap_or(ActiveColor::Mixed),
    }
}

impl CditorV2View {
    pub(crate) fn set_color_menu_hovered(&mut self, hovered: bool, cx: &mut gpui::Context<Self>) {
        self.color_menu_hover_generation = self.color_menu_hover_generation.wrapping_add(1);
        if hovered {
            self.open_color_menu_from_gui(cx);
            return;
        }

        let generation = self.color_menu_hover_generation;
        let delay = cx.background_executor().timer(Duration::from_millis(140));
        cx.spawn(async move |view, cx| {
            delay.await;
            let _ = view.update(cx, |view, cx| {
                if view.color_menu_hover_generation == generation && view.color_menu_open {
                    view.color_menu_open = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn open_color_menu_from_gui(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let has_target = self.gutter_toolbar_block_id.is_some()
            || self
                .ready_runtime_ref()
                .is_some_and(DocumentRuntime::has_document_text_selection);
        if self.color_menu_open || !has_target {
            return false;
        }
        self.color_menu_open = true;
        self.block_transform_menu_open = false;
        self.color_menu_scroll_handle
            .set_offset(gpui::point(gpui::px(0.0), gpui::px(0.0)));
        cx.notify();
        true
    }

    pub(crate) fn apply_color_from_toolbar(
        &mut self,
        action: ColorMenuAction,
        has_text_selection: bool,
        target_block_id: Option<cditor_core::ids::BlockId>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.readonly {
            return false;
        }
        let gutter_block_id = color_action_block_target(has_text_selection, target_block_id);
        trace_block_color(
            "apply.begin",
            format_args!(
                "live_toolbar_block={:?} captured_block={target_block_id:?} resolved_gutter_block={gutter_block_id:?} has_text_selection={has_text_selection} target={:?} value={:?}",
                self.gutter_toolbar_block_id,
                action.target,
                action.value(),
            ),
        );
        let result = self
            .ready_runtime()
            .ok_or_else(|| "runtime is not ready".to_owned())
            .and_then(|runtime| {
                if let Some(block_id) = gutter_block_id {
                    let before = runtime.block_attrs(block_id);
                    let focused = runtime.focused_block_id();
                    let result = runtime.set_block_color(block_id, action.target, action.value());
                    let after = runtime.block_attrs(block_id);
                    trace_block_color(
                        "apply.runtime",
                        format_args!(
                            "block_id={block_id} focused={focused:?} before={before:?} after={after:?} result={result:?}",
                        ),
                    );
                    return result;
                }
                let result = runtime.set_inline_color_on_selection(action.target, action.value());
                trace_block_color(
                    "apply.inline",
                    format_args!("focused={:?} result={result:?}", runtime.focused_block_id()),
                );
                result
            });
        match result {
            Ok(changed) => {
                trace_block_color(
                    "apply.finish",
                    format_args!("changed={changed} mark_dirty={changed}"),
                );
                self.last_color_action = Some(action);
                self.color_menu_open = false;
                self.color_menu_hover_generation = self.color_menu_hover_generation.wrapping_add(1);
                if changed {
                    self.mark_dirty(cx);
                }
                cx.notify();
                changed
            }
            Err(error) => {
                trace_block_color("apply.error", &error);
                self.save_status = crate::gui::persistence::EditorSaveStatus::Failed(error);
                cx.notify();
                false
            }
        }
    }
}

fn color_action_block_target(
    has_text_selection: bool,
    captured_block_id: Option<cditor_core::ids::BlockId>,
) -> Option<cditor_core::ids::BlockId> {
    (!has_text_selection).then_some(captured_block_id).flatten()
}

#[cfg(test)]
mod target_tests {
    use super::*;

    #[test]
    fn gutter_color_uses_the_block_captured_by_the_open_menu() {
        assert_eq!(color_action_block_target(false, Some(42)), Some(42));
    }

    #[test]
    fn text_selection_color_never_falls_back_to_a_captured_block() {
        assert_eq!(color_action_block_target(true, Some(42)), None);
    }
}
