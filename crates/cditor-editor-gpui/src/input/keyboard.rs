use std::collections::HashMap;

use gpui::{ClipboardItem, Context};

use crate::clipboard_assets::image_asset_from_clipboard_item;
use crate::editor_view::{CditorV2View, CditorViewState};
use crate::features::table::{TableAxis, TableAxisSelection};
use crate::input::GuiInputCommand;
use crate::text::{
    ParleyMoveCommand, ParleySelection, ParleyTextPosition, RichTextPlatformLayout,
    TextGeometryOperation, record_snapshot_geometry, record_unavailable_geometry,
};
use cditor_core::edit::TextAffinity;
use cditor_core::ids::{BlockId, SurfaceId};
use cditor_editor_protocol::command::CaretDirection;
#[cfg(test)]
use cditor_import_export::clipboard::ClipboardSelection;
#[cfg(test)]
use cditor_runtime::DocumentRuntime;
use cditor_session::EditorSessionHandle;

impl CditorV2View {
    pub(crate) fn execute_gui_input_command_handler(
        &mut self,
        command: GuiInputCommand,
        cx: &mut Context<Self>,
    ) {
        if self.status.readonly && !matches!(command, GuiInputCommand::CopySelection) {
            return;
        }
        if matches!(
            command,
            GuiInputCommand::ToggleBold
                | GuiInputCommand::ToggleItalic
                | GuiInputCommand::ToggleUnderline
                | GuiInputCommand::ToggleInlineCode
        ) {
            if let Some(editor_command) = command.cditor_command() {
                let _ = self.dispatch_command(
                    editor_command,
                    cditor_editor_protocol::command::CommandSource::Keyboard,
                    cx,
                );
            }
            return;
        }
        if matches!(
            command,
            GuiInputCommand::UndoFocusedBlock | GuiInputCommand::RedoFocusedBlock
        ) {
            let editor_command = if matches!(command, GuiInputCommand::RedoFocusedBlock) {
                cditor_editor_protocol::command::CditorCommand::Redo
            } else {
                cditor_editor_protocol::command::CditorCommand::Undo
            };
            let _ = self.dispatch_command(
                editor_command,
                cditor_editor_protocol::command::CommandSource::Keyboard,
                cx,
            );
            return;
        }
        if matches!(
            command,
            GuiInputCommand::CopySelection
                | GuiInputCommand::CutSelection
                | GuiInputCommand::DeleteBackward
                | GuiInputCommand::DeleteForward
        ) && let Some(request) = self
            .ready_session()
            .and_then(|session| session.selection_materialization_request().ok().flatten())
            && self.schedule_selection_materialization(command, request, cx)
        {
            return;
        }
        let should_scroll_focus = !matches!(
            command,
            GuiInputCommand::Ignore | GuiInputCommand::CopySelection
        );
        let selected_table_axis = self.projected_table_axis_selection();
        let mut deferred_command = None;
        {
            let CditorViewState::Ready(session) = &self.state else {
                return;
            };
            let runtime = session;
            match command {
                GuiInputCommand::Ignore => {}
                GuiInputCommand::SelectAllFocusedText => unreachable!(
                    "select-all returns through Runtime dispatch before the GUI handler"
                ),
                GuiInputCommand::CopySelection => {
                    if let Some((block_id, range)) =
                        selected_table_axis_range(runtime, selected_table_axis)
                        && let Ok(Some(selection)) =
                            runtime.table_clipboard_selection(block_id, range)
                    {
                        let document_id =
                            runtime.snapshot().ok().map(|snapshot| snapshot.document_id);
                        let (system_text, envelope) =
                            crate::input::clipboard::envelope_for_selection(document_id, selection);
                        cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(
                            system_text,
                            &envelope,
                        ));
                    } else if let Ok(snapshot) = runtime.clipboard_snapshot()
                        && let Some(selection) = snapshot.selection
                    {
                        let (system_text, envelope) =
                            crate::input::clipboard::envelope_for_selection(
                                Some(snapshot.document_id),
                                selection,
                            );
                        cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(
                            system_text,
                            &envelope,
                        ));
                    } else if let Ok(Some(text)) = runtime.selected_text() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                }
                GuiInputCommand::CutSelection => {
                    if let Some((block_id, range)) =
                        selected_table_axis_range(runtime, selected_table_axis)
                        && let Ok(Some(selection)) =
                            runtime.table_clipboard_selection(block_id, range)
                    {
                        let document_id =
                            runtime.snapshot().ok().map(|snapshot| snapshot.document_id);
                        let (system_text, envelope) =
                            crate::input::clipboard::envelope_for_selection(document_id, selection);
                        cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(
                            system_text,
                            &envelope,
                        ));
                        deferred_command = Some(
                            cditor_editor_protocol::command::EditorCommand::TableClearRange {
                                block_id,
                                range,
                            },
                        );
                    } else if let Ok(snapshot) = runtime.clipboard_snapshot()
                        && let Some(selection) = snapshot.selection
                    {
                        let (system_text, envelope) =
                            crate::input::clipboard::envelope_for_selection(
                                Some(snapshot.document_id),
                                selection,
                            );
                        cx.write_to_clipboard(ClipboardItem::new_string_with_json_metadata(
                            system_text,
                            &envelope,
                        ));
                        deferred_command =
                            Some(cditor_editor_protocol::command::EditorCommand::DeleteSelection);
                    } else if let Ok(Some(text)) = runtime.selected_text() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        deferred_command =
                            Some(cditor_editor_protocol::command::EditorCommand::DeleteSelection);
                    }
                }
                GuiInputCommand::PasteClipboard => {
                    if let Some(item) = cx.read_from_clipboard() {
                        deferred_command =
                            if let Some(asset) = image_asset_from_clipboard_item(&item) {
                                Some(
                                cditor_editor_protocol::command::EditorCommand::InsertImageAsset {
                                    payload: asset.payload,
                                },
                            )
                            } else {
                                item.text().map(|text| {
                                cditor_editor_protocol::command::EditorCommand::ApplyClipboardData {
                                text,
                                metadata_json: item.metadata().cloned(),
                                }
                            })
                            };
                    }
                }
                GuiInputCommand::UndoFocusedBlock | GuiInputCommand::RedoFocusedBlock => {
                    unreachable!("history returns through Runtime dispatch before the GUI handler")
                }
                GuiInputCommand::InsertParagraphAfterFocused
                | GuiInputCommand::InsertSoftLineBreak
                | GuiInputCommand::HandleEnter
                | GuiInputCommand::IndentBlock
                | GuiInputCommand::OutdentBlock
                | GuiInputCommand::DeleteBackward
                | GuiInputCommand::DeleteForward => {
                    unreachable!("keyboard document mutations return through Runtime dispatch")
                }
                GuiInputCommand::MoveCaretLeft { extend_selection } => {
                    let moved = move_caret_with_parley(
                        &self.cache.text_layouts,
                        &self.cache.text_surface_layouts,
                        &mut self.input.preferred_navigation_x,
                        runtime,
                        ParleyMoveCommand::PreviousVisual,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved {
                        let _ = dispatch_caret_navigation(
                            runtime,
                            CaretDirection::PreviousVisual,
                            extend_selection,
                        );
                    }
                }
                GuiInputCommand::MoveCaretRight { extend_selection } => {
                    let moved = move_caret_with_parley(
                        &self.cache.text_layouts,
                        &self.cache.text_surface_layouts,
                        &mut self.input.preferred_navigation_x,
                        runtime,
                        ParleyMoveCommand::NextVisual,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved {
                        let _ = dispatch_caret_navigation(
                            runtime,
                            CaretDirection::NextVisual,
                            extend_selection,
                        );
                    }
                }
                GuiInputCommand::MoveCaretToPreviousWord { extend_selection } => {
                    let moved = move_caret_with_parley(
                        &self.cache.text_layouts,
                        &self.cache.text_surface_layouts,
                        &mut self.input.preferred_navigation_x,
                        runtime,
                        ParleyMoveCommand::PreviousVisualWord,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved {
                        let _ = dispatch_caret_navigation(
                            runtime,
                            CaretDirection::PreviousWord,
                            extend_selection,
                        );
                    }
                }
                GuiInputCommand::MoveCaretToNextWord { extend_selection } => {
                    let moved = move_caret_with_parley(
                        &self.cache.text_layouts,
                        &self.cache.text_surface_layouts,
                        &mut self.input.preferred_navigation_x,
                        runtime,
                        ParleyMoveCommand::NextVisualWord,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved {
                        let _ = dispatch_caret_navigation(
                            runtime,
                            CaretDirection::NextWord,
                            extend_selection,
                        );
                    }
                }
                GuiInputCommand::MoveCaretToDocumentStart { extend_selection } => {
                    let _ = dispatch_caret_navigation(
                        runtime,
                        CaretDirection::DocumentStart,
                        extend_selection,
                    );
                }
                GuiInputCommand::MoveCaretToDocumentEnd { extend_selection } => {
                    let _ = dispatch_caret_navigation(
                        runtime,
                        CaretDirection::DocumentEnd,
                        extend_selection,
                    );
                }
                GuiInputCommand::MoveCaretUp { extend_selection } => {
                    let moved_in_block = move_caret_with_parley(
                        &self.cache.text_layouts,
                        &self.cache.text_surface_layouts,
                        &mut self.input.preferred_navigation_x,
                        runtime,
                        ParleyMoveCommand::PreviousLine,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved_in_block {
                        let _ = dispatch_caret_navigation(
                            runtime,
                            CaretDirection::PreviousLine,
                            extend_selection,
                        );
                    }
                }
                GuiInputCommand::MoveCaretDown { extend_selection } => {
                    let moved_in_block = move_caret_with_parley(
                        &self.cache.text_layouts,
                        &self.cache.text_surface_layouts,
                        &mut self.input.preferred_navigation_x,
                        runtime,
                        ParleyMoveCommand::NextLine,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved_in_block {
                        let _ = dispatch_caret_navigation(
                            runtime,
                            CaretDirection::NextLine,
                            extend_selection,
                        );
                    }
                }
                GuiInputCommand::MoveCaretToLineStart { extend_selection } => {
                    let moved = move_caret_with_parley(
                        &self.cache.text_layouts,
                        &self.cache.text_surface_layouts,
                        &mut self.input.preferred_navigation_x,
                        runtime,
                        ParleyMoveCommand::LineStart,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved {
                        let _ = dispatch_caret_navigation(
                            runtime,
                            CaretDirection::LineStart,
                            extend_selection,
                        );
                    }
                }
                GuiInputCommand::MoveCaretToLineEnd { extend_selection } => {
                    let moved = move_caret_with_parley(
                        &self.cache.text_layouts,
                        &self.cache.text_surface_layouts,
                        &mut self.input.preferred_navigation_x,
                        runtime,
                        ParleyMoveCommand::LineEnd,
                        extend_selection,
                    )
                    .unwrap_or(false);
                    if !moved {
                        let _ = dispatch_caret_navigation(
                            runtime,
                            CaretDirection::LineEnd,
                            extend_selection,
                        );
                    }
                }
                GuiInputCommand::ToggleBold
                | GuiInputCommand::ToggleItalic
                | GuiInputCommand::ToggleUnderline
                | GuiInputCommand::ToggleInlineCode => unreachable!(
                    "format commands return through Runtime dispatch before handler mutation"
                ),
            }
        }
        if let Some(command) = deferred_command {
            let _ = self.dispatch_command(
                command,
                cditor_editor_protocol::command::CommandSource::Keyboard,
                cx,
            );
        }
        if should_scroll_focus && let CditorViewState::Ready(session) = &self.state {
            let _ = session.ensure_focused_block_visible();
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
    session: &EditorSessionHandle,
    selection: Option<TableAxisSelection>,
) -> Option<(BlockId, cditor_core::rich_text::TableRange)> {
    let selection = selection?;
    let range = match selection.axis {
        TableAxis::Row => session.table_range(
            selection.block_id,
            cditor_session::TableRangeRequest::Row(selection.index),
        ),
        TableAxis::Column => session.table_range(
            selection.block_id,
            cditor_session::TableRangeRequest::Column(selection.index),
        ),
    }
    .ok()
    .flatten()?;
    Some((selection.block_id, range))
}

fn move_caret_with_parley(
    text_layouts: &HashMap<BlockId, RichTextPlatformLayout>,
    text_surface_layouts: &HashMap<SurfaceId, RichTextPlatformLayout>,
    preferred_x: &mut Option<(SurfaceId, f32)>,
    runtime: &EditorSessionHandle,
    command: ParleyMoveCommand,
    extend_selection: bool,
) -> Result<bool, String> {
    let input_context = runtime.input_context().map_err(|error| error.to_string())?;
    let Some(surface_id) = input_context.target.and_then(|target| target.surface_id()) else {
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
    let Some(current) = runtime
        .text_surface_state(surface_id)
        .map_err(|error| error.to_string())?
    else {
        return Ok(false);
    };
    if cache.content_version != current.snapshot.identity.content_version {
        record_unavailable_geometry();
        return Ok(false);
    }
    let Some(caret_offset) = current.caret_offset else {
        return Ok(false);
    };
    let caret_affinity = current.caret_affinity;
    let layout = &cache.snapshot;
    record_snapshot_geometry(TextGeometryOperation::Navigation);
    let selected = input_context
        .selected_range
        .unwrap_or(caret_offset..caret_offset);
    let anchor_offset = if input_context.selection_reversed {
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
    if matches!(
        surface_id,
        SurfaceId::ImageCaption { .. } | SurfaceId::CollectionTitle { .. }
    ) {
        let anchor_offset = if extend_selection {
            anchor_offset
        } else {
            moved.focus.offset
        };
        runtime
            .dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
                cditor_editor_protocol::command::CditorCommand::SetTextSurfaceSelection {
                    surface_id,
                    anchor_offset,
                    focus_offset: moved.focus.offset,
                    focus_affinity: moved.focus.affinity,
                },
                cditor_editor_protocol::command::CommandSource::Keyboard,
            ))
            .map_err(|error| error.to_string())?;
    } else if let SurfaceId::Block(block_id) = surface_id {
        let focus = cditor_core::edit::TextPosition {
            block_id,
            offset: moved.focus.offset,
            affinity: moved.focus.affinity,
        };
        let anchor = if extend_selection {
            runtime
                .document_snapshot()
                .ok()
                .and_then(|snapshot| snapshot.selection)
                .map(|selection| selection.anchor)
                .unwrap_or_else(|| {
                    cditor_core::edit::TextPosition::downstream(block_id, anchor_offset)
                })
        } else {
            focus
        };
        runtime
            .dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
                cditor_editor_protocol::command::CditorCommand::SetDocumentSelection {
                    selection: cditor_core::edit::DocumentSelection { anchor, focus },
                },
                cditor_editor_protocol::command::CommandSource::Keyboard,
            ))
            .map_err(|error| error.to_string())?;
    }
    Ok(true)
}

fn dispatch_caret_navigation(
    runtime: &EditorSessionHandle,
    direction: CaretDirection,
    extend_selection: bool,
) -> Result<bool, String> {
    runtime
        .dispatch(cditor_editor_protocol::command::CommandEnvelope::new(
            cditor_editor_protocol::command::CditorCommand::MoveCaret {
                direction,
                extend_selection,
            },
            cditor_editor_protocol::command::CommandSource::Keyboard,
        ))
        .map(|outcome| outcome.changed())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "keyboard_tests.rs"]
mod tests;
