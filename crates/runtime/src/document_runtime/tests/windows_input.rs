use super::*;
use cditor_core::rich_text::{TableCellPayload, TablePayload, TableRowPayload};

fn paragraph_runtime(text: &str, caret: usize) -> DocumentRuntime {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            text,
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, caret).unwrap();
    runtime
}

fn table_runtime(text: &str, caret: usize) -> DocumentRuntime {
    let mut table = TablePayload {
        rows: vec![TableRowPayload {
            cells: vec![TableCellPayload::plain(text)],
            height: Default::default(),
        }],
        columns: Vec::new(),
        header_rows: 0,
        header_cols: 0,
        header_style: Default::default(),
    };
    table.normalize();
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(table),
        }],
        720.0,
    );
    runtime.focus_table_cell_at_offset(1, 0, 0, caret).unwrap();
    runtime
}

#[test]
fn home_and_end_use_current_soft_line_boundaries() {
    let mut runtime = paragraph_runtime("first\nsecond\nthird", 9);

    assert!(
        runtime
            .move_focused_caret_to_line_boundary(false, false)
            .unwrap()
    );
    assert_eq!(runtime.caret_offset_for_block(1), Some(6));
    assert!(
        runtime
            .move_focused_caret_to_line_boundary(true, false)
            .unwrap()
    );
    assert_eq!(runtime.caret_offset_for_block(1), Some(12));
}

#[test]
fn shift_home_extends_and_reverses_selection() {
    let mut runtime = paragraph_runtime("first\nsecond", 10);

    assert!(
        runtime
            .move_focused_caret_to_line_boundary(false, true)
            .unwrap()
    );
    assert_eq!(runtime.focused_text_selection_range(), Some(6..10));
    assert_eq!(runtime.caret_offset_for_block(1), Some(6));
}

#[test]
fn table_cells_share_home_end_selection_behavior() {
    let mut runtime = table_runtime("one\ntwo", 6);

    assert!(
        runtime
            .move_focused_caret_to_line_boundary(false, true)
            .unwrap()
    );
    let (_, _, _, selection, reversed, _) = runtime.focused_table_cell_selection_state().unwrap();
    assert_eq!(selection, 4..6);
    assert!(reversed);
    assert_eq!(runtime.focused_table_cell_offset(), Some((1, 0, 0, 4)));
}

#[test]
fn legacy_crlf_content_has_windows_line_boundaries() {
    let mut runtime = paragraph_runtime("first\r\nsecond", 11);

    runtime
        .move_focused_caret_to_line_boundary(false, false)
        .unwrap();
    assert_eq!(runtime.caret_offset_for_block(1), Some(7));
}
