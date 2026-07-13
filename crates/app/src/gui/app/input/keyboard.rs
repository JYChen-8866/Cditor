use std::collections::HashMap;

use gpui::{ClipboardItem, Context};

use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::app::input_trace::trace_input;
use crate::gui::block::table::{TableAxis, TableAxisSelection};
use crate::gui::clipboard_assets::image_asset_from_clipboard_item;
use crate::gui::input::GuiInputCommand;
use crate::gui::platform::normalize_external_line_endings;
use crate::gui::text::RichTextPlatformLayout;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::{
    CditorClipboardEnvelope, ClipboardSelection, InlineMark, looks_like_markdown_paste,
};
use cditor_runtime::DocumentRuntime;

fn trace_clipboard_markdown(event: &str, details: impl std::fmt::Display) {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let enabled = *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_MARKDOWN")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
    });
    if enabled {
        eprintln!("[cditor][markdown][clipboard.{event}] {details}");
    }
}

fn clipboard_trace_preview(text: &str) -> String {
    text.chars()
        .take(160)
        .collect::<String>()
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

impl CditorV2View {
    pub(in crate::gui::app) fn apply_input_command(
        &mut self,
        command: GuiInputCommand,
        cx: &mut Context<Self>,
    ) {
        if matches!(command, GuiInputCommand::ToggleDebugOverlay) {
            self.show_debug = !self.show_debug;
            return;
        }
        if self.readonly && !matches!(command, GuiInputCommand::CopySelection) {
            return;
        }
        let should_scroll_focus = !matches!(
            command,
            GuiInputCommand::Ignore
                | GuiInputCommand::ToggleDebugOverlay
                | GuiInputCommand::CopySelection
        );
        let selected_table_axis = self.projected_table_axis_selection();
        {
            let CditorViewState::Ready(runtime) = &mut self.state else {
                return;
            };
            match command {
                GuiInputCommand::Ignore | GuiInputCommand::ToggleDebugOverlay => {}
                GuiInputCommand::SelectAllFocusedText => {
                    runtime.select_all_command();
                }
                GuiInputCommand::CopySelection => {
                    if let Some((block_id, range)) =
                        selected_table_axis_range(runtime, selected_table_axis)
                        && let Some(table) = runtime.table_clipboard_for_range(block_id, range)
                    {
                        let (system_text, envelope) =
                            crate::gui::input::clipboard::envelope_for_selection(
                                Some(runtime.document_id),
                                ClipboardSelection::Table { table: table.table },
                            );
                        cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(
                            system_text,
                            &envelope,
                        ));
                    } else if let Some(selection) = runtime.clipboard_selection_snapshot() {
                        let (system_text, envelope) =
                            crate::gui::input::clipboard::envelope_for_selection(
                                Some(runtime.document_id),
                                selection,
                            );
                        cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(
                            system_text,
                            &envelope,
                        ));
                    } else if let Some(text) = runtime.selected_focused_text() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                }
                GuiInputCommand::CutSelection => {
                    if let Some((block_id, range)) =
                        selected_table_axis_range(runtime, selected_table_axis)
                        && let Some(table) = runtime.table_clipboard_for_range(block_id, range)
                    {
                        let (system_text, envelope) =
                            crate::gui::input::clipboard::envelope_for_selection(
                                Some(runtime.document_id),
                                ClipboardSelection::Table { table: table.table },
                            );
                        cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(
                            system_text,
                            &envelope,
                        ));
                        if runtime.clear_table_range(block_id, range).unwrap_or(false) {
                            self.mark_dirty(cx);
                        }
                    } else if let Some(selection) = runtime.clipboard_selection_snapshot() {
                        let selected_blocks = runtime.has_selected_blocks();
                        let (system_text, envelope) =
                            crate::gui::input::clipboard::envelope_for_selection(
                                Some(runtime.document_id),
                                selection,
                            );
                        cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(
                            system_text,
                            &envelope,
                        ));
                        let changed = if selected_blocks {
                            runtime.delete_selected_block_selection().unwrap_or(false)
                        } else if runtime.has_cross_block_text_selection() {
                            runtime.delete_document_selection().unwrap_or(false)
                        } else {
                            runtime
                                .replace_text_in_focused_range(None, "")
                                .unwrap_or(false)
                        };
                        if changed {
                            self.mark_dirty(cx);
                        }
                    } else if let Some(text) = runtime.selected_focused_text() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        if runtime
                            .replace_text_in_focused_range(None, "")
                            .unwrap_or(false)
                        {
                            self.mark_dirty(cx);
                        }
                    }
                }
                GuiInputCommand::PasteClipboard => {
                    if let Some(item) = cx.read_from_clipboard() {
                        let changed = if let Some(asset) = image_asset_from_clipboard_item(&item) {
                            runtime
                                .insert_image_asset_after_focused(asset.payload)
                                .is_ok()
                        } else if let Some(text) = item.text() {
                            let metadata_selection = item.metadata().and_then(|json| {
                                CditorClipboardEnvelope::decode_metadata(json, &text)
                                    .ok()
                                    .map(|envelope| envelope.selection)
                            });
                            paste_text_from_clipboard(runtime, &text, metadata_selection.as_ref())
                        } else {
                            false
                        };
                        if changed {
                            self.mark_dirty(cx);
                        }
                    }
                }
                GuiInputCommand::UndoFocusedBlock => {
                    if runtime.undo_focused_block().is_ok() {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::RedoFocusedBlock => {
                    if runtime.redo_focused_block().is_ok() {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::InsertParagraphAfterFocused => {
                    if runtime.insert_paragraph_after_focused().is_ok() {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::InsertSoftLineBreak => {
                    if runtime.insert_soft_line_break().is_ok() {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::HandleEnter => {
                    if runtime.handle_enter().is_ok() {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::IndentBlock => {
                    if runtime.indent_focused_block().unwrap_or(false) {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::OutdentBlock => {
                    if runtime.outdent_focused_block().unwrap_or(false) {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::InsertSpaceOrMarkdownShortcut => {
                    if runtime.insert_space_or_markdown_shortcut().is_ok() {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::DeleteBackward => {
                    let result = runtime.delete_backward();
                    trace_input("delete_backward.result", format_args!("{result:?}"));
                    if matches!(result, Ok(true)) {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::DeleteForward => {
                    let result = runtime.delete_forward();
                    trace_input("delete_forward.result", format_args!("{result:?}"));
                    if matches!(result, Ok(true)) {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::MoveCaretLeft { extend_selection } => {
                    let _ = runtime.move_caret_left(extend_selection);
                }
                GuiInputCommand::MoveCaretRight { extend_selection } => {
                    let _ = runtime.move_caret_right(extend_selection);
                }
                GuiInputCommand::MoveCaretUp { extend_selection } => {
                    let moved_in_block = move_caret_vertically_in_focused_block(
                        &self.text_layouts,
                        runtime,
                        -1,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved_in_block {
                        let _ = runtime.move_caret_up(extend_selection);
                    }
                }
                GuiInputCommand::MoveCaretDown { extend_selection } => {
                    let moved_in_block = move_caret_vertically_in_focused_block(
                        &self.text_layouts,
                        runtime,
                        1,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved_in_block {
                        let _ = runtime.move_caret_down(extend_selection);
                    }
                }
                GuiInputCommand::MoveCaretToLineStart { extend_selection } => {
                    let _ = runtime.move_focused_caret_to_line_boundary(false, extend_selection);
                }
                GuiInputCommand::MoveCaretToLineEnd { extend_selection } => {
                    let _ = runtime.move_focused_caret_to_line_boundary(true, extend_selection);
                }
                GuiInputCommand::ToggleBold => {
                    if runtime
                        .toggle_inline_mark_on_selection(InlineMark::Bold)
                        .is_ok()
                    {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::ToggleItalic => {
                    if runtime
                        .toggle_inline_mark_on_selection(InlineMark::Italic)
                        .is_ok()
                    {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::ToggleUnderline => {
                    if runtime
                        .toggle_inline_mark_on_selection(InlineMark::Underline)
                        .is_ok()
                    {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::ToggleInlineCode => {
                    if runtime
                        .toggle_inline_mark_on_selection(InlineMark::Code)
                        .is_ok()
                    {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::InsertChar(ch) => {
                    ensure_runtime_focus_for_insert_char(runtime);
                    if runtime.insert_char(ch).is_ok() {
                        self.mark_dirty(cx);
                    }
                }
            }
        }
        if should_scroll_focus && let CditorViewState::Ready(runtime) = &mut self.state {
            let _ = runtime.scroll_focused_block_into_view();
        }
    }
}

pub(super) fn mermaid_preview_blocks_command(command: GuiInputCommand) -> bool {
    matches!(
        command,
        GuiInputCommand::SelectAllFocusedText
            | GuiInputCommand::CutSelection
            | GuiInputCommand::PasteClipboard
            | GuiInputCommand::InsertSoftLineBreak
            | GuiInputCommand::InsertSpaceOrMarkdownShortcut
            | GuiInputCommand::DeleteBackward
            | GuiInputCommand::DeleteForward
            | GuiInputCommand::MoveCaretToLineStart { .. }
            | GuiInputCommand::MoveCaretToLineEnd { .. }
            | GuiInputCommand::ToggleBold
            | GuiInputCommand::ToggleItalic
            | GuiInputCommand::ToggleUnderline
            | GuiInputCommand::ToggleInlineCode
            | GuiInputCommand::InsertChar(_)
    )
}

fn selected_table_axis_range(
    runtime: &DocumentRuntime,
    selection: Option<TableAxisSelection>,
) -> Option<(BlockId, cditor_core::rich_text::TableRange)> {
    let selection = selection?;
    let range = match selection.axis {
        TableAxis::Row => runtime.table_row_selection_range(selection.block_id, selection.index),
        TableAxis::Column => {
            runtime.table_column_selection_range(selection.block_id, selection.index)
        }
    }?;
    Some((selection.block_id, range))
}

pub(in crate::gui::app) fn ensure_runtime_focus_for_insert_char(runtime: &mut DocumentRuntime) {
    if runtime.focused_block_id().is_none()
        && let Some(block_id) = runtime.first_visible_block_id()
    {
        runtime.focus_block(block_id);
    }
}

fn paste_text_from_clipboard(
    runtime: &mut DocumentRuntime,
    text: &str,
    metadata_selection: Option<&ClipboardSelection>,
) -> bool {
    let text = normalize_external_line_endings(text);
    let text = text.as_ref();
    let markdown_detected = looks_like_markdown_paste(text);
    trace_clipboard_markdown(
        "received",
        format_args!(
            "bytes={} metadata={} detected={} focus={:?} preview=\"{}\"",
            text.len(),
            metadata_selection.is_some(),
            markdown_detected,
            runtime.focused_block_id(),
            clipboard_trace_preview(text)
        ),
    );
    if let Some(selection @ ClipboardSelection::Table { .. }) = metadata_selection {
        match runtime.paste_clipboard_selection(selection) {
            Ok(true) => {
                trace_clipboard_markdown("result", "route=table_metadata changed=true");
                return true;
            }
            Ok(false) => {}
            Err(error) => trace_clipboard_markdown("metadata_error", format_args!("error={error}")),
        }
    }
    match runtime.paste_delimited_table_text_at_focused_cell(text) {
        Ok(true) => {
            trace_clipboard_markdown("result", "route=table changed=true");
            return true;
        }
        Ok(false) => {}
        Err(error) => trace_clipboard_markdown("table_error", format_args!("error={error}")),
    }
    if markdown_detected {
        match runtime.insert_markdown_paste(text) {
            Ok(true) => {
                trace_clipboard_markdown("result", "route=markdown changed=true");
                return true;
            }
            result => trace_clipboard_markdown(
                "markdown_error",
                format_args!("markdown_result={result:?}"),
            ),
        }
    }
    if let Some(selection) = metadata_selection {
        match runtime.paste_clipboard_selection(selection) {
            Ok(true) => {
                trace_clipboard_markdown("result", "route=rich_metadata changed=true");
                return true;
            }
            Ok(false) => {}
            Err(error) => trace_clipboard_markdown("metadata_error", format_args!("error={error}")),
        }
    }
    trace_clipboard_markdown("fallback", "route=plain_text");
    let changed = runtime
        .replace_text_in_focused_range(None, text)
        .unwrap_or(false);
    trace_clipboard_markdown("result", format_args!("route=plain_text changed={changed}"));
    changed
}

fn move_caret_vertically_in_focused_block(
    text_layouts: &HashMap<BlockId, RichTextPlatformLayout>,
    runtime: &mut DocumentRuntime,
    direction: i32,
    extend_selection: bool,
) -> Result<bool, String> {
    let Some(block_id) = runtime.focused_block_id() else {
        return Ok(false);
    };
    let Some(cache) = text_layouts.get(&block_id) else {
        return Ok(false);
    };
    let Some(current_content_version) = runtime.block_content_version(block_id) else {
        return Ok(false);
    };
    if cache.content_version != current_content_version {
        return Ok(false);
    }
    let Some(caret) = runtime.caret_offset_for_block(block_id) else {
        return Ok(false);
    };
    let Some(bounds) = crate::gui::text::platform_range_bounds(cache, caret..caret) else {
        return Ok(false);
    };
    let line_height = f32::from(cache.line_height).max(1.0);
    let target = gpui::point(
        bounds.left() + bounds.size.width / 2.0,
        bounds.top() + gpui::px(line_height * direction as f32),
    );
    let next = crate::gui::text::platform_index_for_point(cache, target);
    if next == caret {
        return Ok(false);
    }
    runtime.move_focused_caret_to_offset(block_id, next, extend_selection)?;
    Ok(true)
}

#[cfg(test)]
#[path = "keyboard_tests.rs"]
mod tests;
