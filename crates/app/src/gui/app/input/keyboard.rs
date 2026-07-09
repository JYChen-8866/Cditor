use std::collections::HashMap;

use gpui::{ClipboardItem, Context, KeyDownEvent, Window};

use crate::gui::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::gui::block::table::{TableAxis, TableAxisSelection};
use crate::gui::clipboard_assets::image_asset_from_clipboard_item;
use crate::gui::image_preview::close_active_preview_if_escape_enabled;
use crate::gui::input::{GuiInputCommand, RichClipboardItem, command_for_key_down};
use crate::gui::text::RichTextPlatformLayout;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::InlineMark;
use cditor_runtime::DocumentRuntime;

impl CditorV2View {
    pub(in crate::gui::app) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.handle_table_cell_key_down(event, cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.apply_code_language_key_from_gui(event, cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.handle_slash_menu_key_down(event, cx) {
            cx.stop_propagation();
            return;
        }
        if event.keystroke.key.as_str() == "escape" && close_active_preview_if_escape_enabled(cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let command = command_for_key_down(event);
        if command.should_stop_propagation() {
            self.apply_input_command(command, cx);
            self.sync_slash_menu_from_runtime(cx);
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn handle_table_cell_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        if event.keystroke.is_ime_in_progress() {
            return false;
        }
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return false;
        }
        let CditorViewState::Ready(runtime) = &mut self.state else {
            return false;
        };
        if runtime.focused_table_cell_offset().is_none() {
            return false;
        }
        let (consumed, changed) = match event.keystroke.key.as_str() {
            "escape" => (runtime.blur_table_cell(), false),
            "backspace" => (true, runtime.delete_backward().unwrap_or(false)),
            "delete" => (true, runtime.delete_forward().unwrap_or(false)),
            "left" => (
                true,
                runtime.move_focused_table_cell_left().unwrap_or(false),
            ),
            "right" => (
                true,
                runtime.move_focused_table_cell_right().unwrap_or(false),
            ),
            "up" => (true, runtime.move_focused_table_cell_up().unwrap_or(false)),
            "down" => (
                true,
                runtime.move_focused_table_cell_down().unwrap_or(false),
            ),
            "tab" => (
                true,
                runtime
                    .move_focused_table_cell_tab(modifiers.shift)
                    .unwrap_or(false),
            ),
            "space" => (
                true,
                runtime
                    .replace_text_in_focused_range(None, " ")
                    .unwrap_or(false),
            ),
            "enter" => (true, runtime.handle_enter().is_ok()),
            _ => (false, false),
        };
        if consumed {
            self.slash_menu = None;
        }
        if changed {
            self.mark_dirty(cx);
        }
        consumed
    }

    fn handle_slash_menu_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        if self.slash_menu.is_none() || event.keystroke.is_ime_in_progress() {
            return false;
        }
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.cancel_slash_menu(cx),
            "up" => self.move_slash_menu_selection(-1, cx),
            "down" => self.move_slash_menu_selection(1, cx),
            "enter" | "tab" => {
                let _ = self.apply_selected_slash_menu_item(cx);
                true
            }
            _ => false,
        }
    }

    pub(in crate::gui::app) fn apply_input_command(
        &mut self,
        command: GuiInputCommand,
        cx: &mut Context<Self>,
    ) {
        if matches!(command, GuiInputCommand::ToggleDebugOverlay) {
            self.show_debug = !self.show_debug;
            return;
        }
        if self.readonly {
            return;
        }
        let should_scroll_focus = !matches!(
            command,
            GuiInputCommand::Ignore
                | GuiInputCommand::ToggleDebugOverlay
                | GuiInputCommand::CopySelection
        );
        let mut next_internal_clipboard = self.internal_clipboard.clone();
        let selected_table_axis = self.selected_table_axis;
        {
            let CditorViewState::Ready(runtime) = &mut self.state else {
                return;
            };
            match command {
                GuiInputCommand::Ignore | GuiInputCommand::ToggleDebugOverlay => {}
                GuiInputCommand::SelectAllFocusedText => {
                    runtime.select_focused_text_all();
                }
                GuiInputCommand::CopySelection => {
                    if let Some((block_id, range)) =
                        selected_table_axis_range(runtime, selected_table_axis)
                        && let Some(table) = runtime.table_clipboard_for_range(block_id, range)
                    {
                        let system_text = table.markdown.clone();
                        next_internal_clipboard = Some(RichClipboardItem::from_table(table));
                        cx.write_to_clipboard(ClipboardItem::new_string(system_text));
                    } else if let Some(text) = runtime.selected_focused_text() {
                        next_internal_clipboard = runtime
                            .selected_focused_rich_text()
                            .map(RichClipboardItem::from_rich)
                            .or_else(|| Some(RichClipboardItem::plain_text(text.clone())));
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                }
                GuiInputCommand::CutSelection => {
                    if let Some((block_id, range)) =
                        selected_table_axis_range(runtime, selected_table_axis)
                        && let Some(table) = runtime.table_clipboard_for_range(block_id, range)
                    {
                        let system_text = table.markdown.clone();
                        next_internal_clipboard = Some(RichClipboardItem::from_table(table));
                        cx.write_to_clipboard(ClipboardItem::new_string(system_text));
                    } else if let Some(text) = runtime.selected_focused_text() {
                        next_internal_clipboard = runtime
                            .selected_focused_rich_text()
                            .map(RichClipboardItem::from_rich)
                            .or_else(|| Some(RichClipboardItem::plain_text(text.clone())));
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        let changed = if runtime.has_cross_block_text_selection() {
                            runtime.delete_document_selection().unwrap_or(false)
                        } else {
                            runtime
                                .replace_text_in_focused_range(None, "")
                                .unwrap_or(false)
                        };
                        if changed {
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
                            paste_text_from_clipboard(
                                runtime,
                                &text,
                                next_internal_clipboard.as_ref(),
                            )
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
                    if runtime.delete_backward().is_ok() {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::DeleteForward => {
                    if runtime.delete_forward().is_ok() {
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
        self.internal_clipboard = next_internal_clipboard;
        if should_scroll_focus && let CditorViewState::Ready(runtime) = &mut self.state {
            let _ = runtime.scroll_focused_block_into_view();
        }
    }
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
    internal_clipboard: Option<&RichClipboardItem>,
) -> bool {
    let rich_text = internal_clipboard
        .filter(|item| item.matches_system_text(text))
        .and_then(|item| item.rich_text.as_ref());
    let table = internal_clipboard
        .filter(|item| item.matches_system_text(text))
        .and_then(|item| item.table.as_ref());
    if let Some(table) = table {
        match runtime.paste_table_clipboard_at_focused_cell(table) {
            Ok(true) => return true,
            Ok(false) | Err(_) => {}
        }
    }
    match runtime.paste_delimited_table_text_at_focused_cell(text) {
        Ok(true) => return true,
        Ok(false) | Err(_) => {}
    }
    if let Some(rich_text) = rich_text {
        match runtime.replace_focused_range_with_rich_text_spans(&rich_text.spans) {
            Ok(true) => return true,
            Ok(false) | Err(_) => {}
        }
    }
    match runtime.insert_markdown_paste(text) {
        Ok(true) => true,
        Ok(false) | Err(_) => runtime
            .replace_text_in_focused_range(None, text)
            .unwrap_or(false),
    }
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
mod tests {
    use super::*;
    use cditor_core::rich_text::{
        BlockPayload, BlockPayloadRecord, InlineMark, InlineSpan, RichBlockKind, TableCellPayload,
        TablePayload, TableRowPayload,
    };
    use cditor_runtime::RichTextSelectionSnapshot;

    fn paragraph_runtime(text: &str) -> DocumentRuntime {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(
                1,
                RichBlockKind::Paragraph,
                text,
            )],
            720.0,
        );
        runtime.focus_block_at_offset(1, text.len()).unwrap();
        runtime
    }

    fn table_runtime(block_id: BlockId, rows: &[&[&str]]) -> DocumentRuntime {
        let mut table = TablePayload {
            rows: rows
                .iter()
                .map(|row| TableRowPayload {
                    cells: row
                        .iter()
                        .map(|cell| TableCellPayload::plain(*cell))
                        .collect(),
                    height: Default::default(),
                })
                .collect(),
            columns: Vec::new(),
            header_rows: 0,
            header_cols: 0,
            header_style: Default::default(),
        };
        table.normalize();
        DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord {
                block_id,
                content_version: 1,
                kind: RichBlockKind::Table,
                payload: BlockPayload::Table(table),
            }],
            720.0,
        )
    }

    #[test]
    fn paste_text_from_clipboard_uses_internal_rich_snapshot_only_when_plain_text_matches() {
        let mut runtime = paragraph_runtime("hello ");
        let internal = RichClipboardItem::from_rich(RichTextSelectionSnapshot {
            text: "bold".to_owned(),
            spans: vec![InlineSpan {
                text: "bold".to_owned(),
                marks: vec![InlineMark::Bold],
            }],
        });

        assert!(paste_text_from_clipboard(
            &mut runtime,
            "bold",
            Some(&internal)
        ));

        let payload = runtime.payload_window.get(1).unwrap();
        match &payload.payload {
            BlockPayload::RichText { spans } => {
                assert_eq!(payload.plain_text(), "hello bold");
                assert!(
                    spans
                        .iter()
                        .any(|span| span.text == "bold" && span.marks.contains(&InlineMark::Bold))
                );
            }
            _ => panic!("expected rich text payload"),
        }
    }

    #[test]
    fn paste_text_from_clipboard_treats_mismatched_internal_snapshot_as_external_plain_text() {
        let mut runtime = paragraph_runtime("hello ");
        let internal = RichClipboardItem::from_rich(RichTextSelectionSnapshot {
            text: "bold".to_owned(),
            spans: vec![InlineSpan {
                text: "bold".to_owned(),
                marks: vec![InlineMark::Bold],
            }],
        });

        assert!(paste_text_from_clipboard(
            &mut runtime,
            "plain",
            Some(&internal)
        ));

        let payload = runtime.payload_window.get(1).unwrap();
        match &payload.payload {
            BlockPayload::RichText { spans } => {
                assert_eq!(payload.plain_text(), "hello plain");
                assert!(
                    spans
                        .iter()
                        .all(|span| !span.marks.contains(&InlineMark::Bold))
                );
            }
            _ => panic!("expected rich text payload"),
        }
    }

    #[test]
    fn paste_text_from_clipboard_uses_internal_table_snapshot_when_system_text_matches() {
        let source = table_runtime(1, &[&["a", "b"], &["c", "d"]]);
        let snapshot = source
            .table_clipboard_for_whole_table(1)
            .expect("table clipboard");
        let internal = RichClipboardItem::from_table(snapshot.clone());
        let mut target = table_runtime(2, &[&["x"]]);
        target.focus_table_cell_at_offset(2, 0, 0, 0).unwrap();

        assert!(paste_text_from_clipboard(
            &mut target,
            &snapshot.markdown,
            Some(&internal)
        ));

        let payload = target.payload_window.get(2).unwrap();
        let BlockPayload::Table(table) = &payload.payload else {
            panic!("expected table payload");
        };
        assert_eq!(table.row_count(), 2);
        assert_eq!(table.column_count(), 2);
        assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a"));
        assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some("d"));
    }

    #[test]
    fn paste_text_from_clipboard_treats_external_tsv_as_table_range_when_cell_is_focused() {
        let mut target = table_runtime(2, &[&["x"]]);
        target.focus_table_cell_at_offset(2, 0, 0, 0).unwrap();

        assert!(paste_text_from_clipboard(&mut target, "a\tb\nc\td", None));

        let payload = target.payload_window.get(2).unwrap();
        let BlockPayload::Table(table) = &payload.payload else {
            panic!("expected table payload");
        };
        assert_eq!(table.row_count(), 2);
        assert_eq!(table.column_count(), 2);
        assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a"));
        assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some("d"));
    }
}
