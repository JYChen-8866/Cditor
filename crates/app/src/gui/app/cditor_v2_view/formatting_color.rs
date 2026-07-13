use std::ops::Range;

use cditor_core::rich_text::{InlineColorTarget, InlineMark, InlineSpan};
use cditor_runtime::DocumentRuntime;

use crate::gui::overlay::{ActiveColor, ColorMenuAction, PaletteColor};

use super::CditorV2View;

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
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.readonly {
            return false;
        }
        let gutter_block_id = (!has_text_selection)
            .then_some(self.gutter_toolbar_block_id)
            .flatten();
        let result = self
            .ready_runtime()
            .ok_or_else(|| "runtime is not ready".to_owned())
            .and_then(|runtime| {
                if let Some(block_id) = gutter_block_id {
                    let text_len = runtime
                        .block_payload_record(block_id)
                        .map(|payload| payload.plain_text().len())
                        .ok_or_else(|| format!("missing payload for block {block_id}"))?;
                    return runtime.set_inline_color_for_range(
                        block_id,
                        0..text_len,
                        action.target,
                        action.value(),
                    );
                }
                runtime.set_inline_color_on_selection(action.target, action.value())
            });
        match result {
            Ok(changed) => {
                self.last_color_action = Some(action);
                self.color_menu_open = false;
                if changed {
                    self.mark_dirty(cx);
                }
                cx.notify();
                changed
            }
            Err(error) => {
                self.save_status = crate::gui::persistence::EditorSaveStatus::Failed(error);
                cx.notify();
                false
            }
        }
    }
}
