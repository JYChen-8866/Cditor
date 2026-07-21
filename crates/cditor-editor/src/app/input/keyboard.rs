use std::collections::HashMap;

use gpui::{ClipboardItem, Context};

use crate::app::cditor_v2_view::{CditorV2View, CditorViewState};
use crate::app::input_trace::trace_input;
use crate::block::table::{TableAxis, TableAxisSelection};
use crate::clipboard_assets::image_asset_from_clipboard_item;
use crate::input::GuiInputCommand;
use crate::platform::normalize_external_line_endings;
use crate::text::{
    ParleyMoveCommand, ParleySelection, ParleyTextPosition, RichTextPlatformLayout,
    TextGeometryOperation, record_snapshot_geometry, record_unavailable_geometry,
};
use cditor_core::edit::TextAffinity;
use cditor_core::ids::{BlockId, SurfaceId};
use cditor_core::rich_text::InlineMark;
use cditor_import_export::clipboard::{CditorClipboardEnvelope, ClipboardSelection};
use cditor_import_export::markdown::looks_like_markdown_paste;
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
    pub(in crate::app) fn execute_gui_input_command_handler(
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
        if matches!(
            command,
            GuiInputCommand::UndoFocusedBlock | GuiInputCommand::RedoFocusedBlock
        ) {
            let redo = matches!(command, GuiInputCommand::RedoFocusedBlock);
            let origin = if redo {
                cditor_core::edit::ChangeOrigin::Redo
            } else {
                cditor_core::edit::ChangeOrigin::Undo
            };
            let _ = self.execute_history_action(origin, redo, cx);
            return;
        }
        if matches!(
            command,
            GuiInputCommand::CopySelection
                | GuiInputCommand::CutSelection
                | GuiInputCommand::DeleteBackward
                | GuiInputCommand::DeleteForward
        ) && let Some(request) = self
            .ready_runtime_ref()
            .and_then(DocumentRuntime::selection_materialization_request)
            && self.schedule_selection_materialization(command, request, cx)
        {
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
                            crate::input::clipboard::envelope_for_selection(
                                Some(runtime.document_id),
                                ClipboardSelection::Table { table: table.table },
                            );
                        cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(
                            system_text,
                            &envelope,
                        ));
                    } else if let Some(selection) = runtime.clipboard_selection_snapshot() {
                        let (system_text, envelope) =
                            crate::input::clipboard::envelope_for_selection(
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
                            crate::input::clipboard::envelope_for_selection(
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
                            crate::input::clipboard::envelope_for_selection(
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
                    if matches!(runtime.undo_focused_block(), Ok(true)) {
                        self.mark_dirty_with_origin(cditor_core::edit::ChangeOrigin::Undo, cx);
                    }
                }
                GuiInputCommand::RedoFocusedBlock => {
                    if matches!(runtime.redo_focused_block(), Ok(true)) {
                        self.mark_dirty_with_origin(cditor_core::edit::ChangeOrigin::Redo, cx);
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
                    let moved = move_caret_with_parley(
                        &self.text_layouts,
                        &self.text_surface_layouts,
                        &mut self.preferred_text_navigation_x,
                        runtime,
                        ParleyMoveCommand::PreviousVisual,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved {
                        let _ = runtime.move_caret_left(extend_selection);
                    }
                }
                GuiInputCommand::MoveCaretRight { extend_selection } => {
                    let moved = move_caret_with_parley(
                        &self.text_layouts,
                        &self.text_surface_layouts,
                        &mut self.preferred_text_navigation_x,
                        runtime,
                        ParleyMoveCommand::NextVisual,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved {
                        let _ = runtime.move_caret_right(extend_selection);
                    }
                }
                GuiInputCommand::MoveCaretToPreviousWord { extend_selection } => {
                    let moved = move_caret_with_parley(
                        &self.text_layouts,
                        &self.text_surface_layouts,
                        &mut self.preferred_text_navigation_x,
                        runtime,
                        ParleyMoveCommand::PreviousVisualWord,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved {
                        let _ = runtime.move_focused_caret_by_word(false, extend_selection);
                    }
                }
                GuiInputCommand::MoveCaretToNextWord { extend_selection } => {
                    let moved = move_caret_with_parley(
                        &self.text_layouts,
                        &self.text_surface_layouts,
                        &mut self.preferred_text_navigation_x,
                        runtime,
                        ParleyMoveCommand::NextVisualWord,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved {
                        let _ = runtime.move_focused_caret_by_word(true, extend_selection);
                    }
                }
                GuiInputCommand::MoveCaretToDocumentStart { extend_selection } => {
                    let _ = runtime.move_caret_to_document_boundary(false, extend_selection);
                }
                GuiInputCommand::MoveCaretToDocumentEnd { extend_selection } => {
                    let _ = runtime.move_caret_to_document_boundary(true, extend_selection);
                }
                GuiInputCommand::MoveCaretUp { extend_selection } => {
                    let moved_in_block = move_caret_with_parley(
                        &self.text_layouts,
                        &self.text_surface_layouts,
                        &mut self.preferred_text_navigation_x,
                        runtime,
                        ParleyMoveCommand::PreviousLine,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved_in_block {
                        let _ = runtime.move_caret_up(extend_selection);
                    }
                }
                GuiInputCommand::MoveCaretDown { extend_selection } => {
                    let moved_in_block = move_caret_with_parley(
                        &self.text_layouts,
                        &self.text_surface_layouts,
                        &mut self.preferred_text_navigation_x,
                        runtime,
                        ParleyMoveCommand::NextLine,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved_in_block {
                        let _ = runtime.move_caret_down(extend_selection);
                    }
                }
                GuiInputCommand::MoveCaretToLineStart { extend_selection } => {
                    let moved = move_caret_with_parley(
                        &self.text_layouts,
                        &self.text_surface_layouts,
                        &mut self.preferred_text_navigation_x,
                        runtime,
                        ParleyMoveCommand::LineStart,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved {
                        let _ =
                            runtime.move_focused_caret_to_line_boundary(false, extend_selection);
                    }
                }
                GuiInputCommand::MoveCaretToLineEnd { extend_selection } => {
                    let moved = move_caret_with_parley(
                        &self.text_layouts,
                        &self.text_surface_layouts,
                        &mut self.preferred_text_navigation_x,
                        runtime,
                        ParleyMoveCommand::LineEnd,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved {
                        let _ = runtime.move_focused_caret_to_line_boundary(true, extend_selection);
                    }
                }
                GuiInputCommand::ToggleBold => {
                    if matches!(
                        runtime.toggle_inline_mark_on_selection(InlineMark::Bold),
                        Ok(true)
                    ) {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::ToggleItalic => {
                    if matches!(
                        runtime.toggle_inline_mark_on_selection(InlineMark::Italic),
                        Ok(true)
                    ) {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::ToggleUnderline => {
                    if matches!(
                        runtime.toggle_inline_mark_on_selection(InlineMark::Underline),
                        Ok(true)
                    ) {
                        self.mark_dirty(cx);
                    }
                }
                GuiInputCommand::ToggleInlineCode => {
                    if matches!(
                        runtime.toggle_inline_mark_on_selection(InlineMark::Code),
                        Ok(true)
                    ) {
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
            | GuiInputCommand::DeleteBackward
            | GuiInputCommand::DeleteForward
            | GuiInputCommand::MoveCaretToLineStart { .. }
            | GuiInputCommand::MoveCaretToLineEnd { .. }
            | GuiInputCommand::MoveCaretToPreviousWord { .. }
            | GuiInputCommand::MoveCaretToNextWord { .. }
            | GuiInputCommand::MoveCaretToDocumentStart { .. }
            | GuiInputCommand::MoveCaretToDocumentEnd { .. }
            | GuiInputCommand::ToggleBold
            | GuiInputCommand::ToggleItalic
            | GuiInputCommand::ToggleUnderline
            | GuiInputCommand::ToggleInlineCode
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
    let changed = runtime.replace_text_from_paste(None, text).unwrap_or(false);
    trace_clipboard_markdown("result", format_args!("route=plain_text changed={changed}"));
    changed
}

fn move_caret_with_parley(
    text_layouts: &HashMap<BlockId, RichTextPlatformLayout>,
    text_surface_layouts: &HashMap<SurfaceId, RichTextPlatformLayout>,
    preferred_x: &mut Option<(SurfaceId, f32)>,
    runtime: &mut DocumentRuntime,
    command: ParleyMoveCommand,
    extend_selection: bool,
) -> Result<bool, String> {
    let Some(surface_id) = runtime.focused_text_surface_id() else {
        return Ok(false);
    };
    let cache = match surface_id {
        SurfaceId::Block(block_id) => text_layouts.get(&block_id),
        SurfaceId::ImageCaption { .. } | SurfaceId::CollectionTitle { .. } => {
            text_surface_layouts.get(&surface_id)
        }
        SurfaceId::TableCell { .. } | SurfaceId::Ephemeral { .. } => None,
    };
    let Some(cache) = cache else {
        record_unavailable_geometry();
        return Ok(false);
    };
    let Some(current) = runtime.text_surface_snapshot(surface_id) else {
        return Ok(false);
    };
    if cache.content_version != current.identity.content_version {
        record_unavailable_geometry();
        return Ok(false);
    }
    let Some(caret_offset) = runtime.text_surface_caret_offset(surface_id) else {
        return Ok(false);
    };
    let caret_affinity = match surface_id {
        SurfaceId::Block(block_id) => runtime
            .caret_position_for_block(block_id)
            .map(|position| position.affinity)
            .unwrap_or(TextAffinity::Downstream),
        _ => TextAffinity::Downstream,
    };
    let layout = &cache.snapshot;
    record_snapshot_geometry(TextGeometryOperation::Navigation);
    let selected = runtime
        .input_session_selected_range()
        .unwrap_or(caret_offset..caret_offset);
    let anchor_offset = if runtime.input_session_selection_reversed() {
        selected.end
    } else {
        selected.start
    };
    let selection = ParleySelection {
        anchor: ParleyTextPosition {
            offset: anchor_offset,
            affinity: TextAffinity::Downstream,
        },
        focus: ParleyTextPosition {
            offset: caret_offset,
            affinity: caret_affinity,
        },
    };
    let current_preferred_x = preferred_x
        .as_ref()
        .filter(|(current_surface, _)| *current_surface == surface_id)
        .map(|(_, x)| *x);
    let (moved, next_preferred_x) = layout.move_selection_with_preferred_x(
        selection,
        command,
        extend_selection,
        current_preferred_x,
    );
    *preferred_x = next_preferred_x.map(|x| (surface_id, x));
    if moved.focus.offset == caret_offset && moved.focus.affinity == caret_affinity {
        return Ok(false);
    }
    runtime.move_focused_text_surface_to_offset(
        surface_id,
        moved.focus.offset,
        moved.focus.affinity,
        extend_selection,
    )?;
    Ok(true)
}

#[cfg(test)]
#[path = "keyboard_tests.rs"]
mod tests;
