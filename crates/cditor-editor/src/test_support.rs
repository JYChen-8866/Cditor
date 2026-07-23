use std::ops::Range;

use cditor_core::{
    edit::{DocumentSelection, TextAffinity, TextPosition},
    ids::{BlockId, SurfaceId},
    rich_text::InlineColorTarget,
};
use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};
use cditor_runtime::{DocumentRuntime, RealtimeInput, RealtimeInputRequest};

fn dispatch(runtime: &mut DocumentRuntime, command: EditorCommand) {
    runtime
        .dispatch(CommandEnvelope::new(command, CommandSource::Automation))
        .unwrap();
}

pub(crate) fn focus_block_at_offset(
    runtime: &mut DocumentRuntime,
    block_id: BlockId,
    offset: usize,
) {
    set_document_text_selection(runtime, block_id, offset, block_id, offset);
}

pub(crate) fn set_document_text_selection(
    runtime: &mut DocumentRuntime,
    anchor_block_id: BlockId,
    anchor_offset: usize,
    focus_block_id: BlockId,
    focus_offset: usize,
) {
    dispatch(
        runtime,
        EditorCommand::SetDocumentSelection {
            selection: DocumentSelection {
                anchor: TextPosition::downstream(anchor_block_id, anchor_offset),
                focus: TextPosition::downstream(focus_block_id, focus_offset),
            },
        },
    );
}

pub(crate) fn select_block_range(
    runtime: &mut DocumentRuntime,
    anchor_block_id: BlockId,
    focus_block_id: BlockId,
) {
    dispatch(
        runtime,
        EditorCommand::SetBlockSelectionRange {
            anchor_block_id,
            focus_block_id,
        },
    );
}

pub(crate) fn focus_table_cell_at_offset(
    runtime: &mut DocumentRuntime,
    block_id: BlockId,
    row: usize,
    col: usize,
    offset: usize,
) {
    dispatch(
        runtime,
        EditorCommand::FocusTableCell {
            block_id,
            row,
            col,
            offset: Some(offset),
            affinity: TextAffinity::Downstream,
        },
    );
}

pub(crate) fn focus_text_surface_at_offset(
    runtime: &mut DocumentRuntime,
    surface_id: SurfaceId,
    offset: usize,
) {
    dispatch(
        runtime,
        EditorCommand::SetTextSurfaceSelection {
            surface_id,
            anchor_offset: offset,
            focus_offset: offset,
            focus_affinity: TextAffinity::Downstream,
        },
    );
}

pub(crate) fn replace_realtime_text(
    runtime: &mut DocumentRuntime,
    range: Option<Range<usize>>,
    text: &str,
) {
    let expected = runtime.input_session_identity().unwrap();
    runtime
        .apply_realtime_input(RealtimeInputRequest {
            expected,
            input: RealtimeInput::ReplaceText { range, text },
        })
        .unwrap();
}

pub(crate) fn set_inline_color_for_range(
    runtime: &mut DocumentRuntime,
    block_id: BlockId,
    range: Range<usize>,
    target: InlineColorTarget,
    color: Option<&str>,
) {
    set_document_text_selection(runtime, block_id, range.start, block_id, range.end);
    dispatch(
        runtime,
        EditorCommand::SetInlineColor {
            target,
            color: color.map(str::to_owned),
        },
    );
}
