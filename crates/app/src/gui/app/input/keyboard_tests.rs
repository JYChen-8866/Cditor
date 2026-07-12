use super::*;

#[test]
fn mermaid_preview_blocks_hidden_source_mutations() {
    assert!(mermaid_preview_blocks_command(GuiInputCommand::InsertChar(
        'x'
    )));
    assert!(mermaid_preview_blocks_command(
        GuiInputCommand::DeleteBackward
    ));
    assert!(mermaid_preview_blocks_command(
        GuiInputCommand::PasteClipboard
    ));
    assert!(!mermaid_preview_blocks_command(
        GuiInputCommand::HandleEnter
    ));
    assert!(!mermaid_preview_blocks_command(
        GuiInputCommand::MoveCaretDown {
            extend_selection: false
        }
    ));
}
use cditor_core::rich_text::{
    BlockPayload, BlockPayloadRecord, InlineMark, InlineSpan, RichBlockKind, TableCellPayload,
    TablePayload, TableRowPayload,
};

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
fn paste_text_from_clipboard_uses_validated_rich_metadata() {
    let mut runtime = paragraph_runtime("hello ");
    let selection = ClipboardSelection::Inline {
        spans: vec![InlineSpan {
            text: "bold".to_owned(),
            marks: vec![InlineMark::Bold],
        }],
    };

    assert!(paste_text_from_clipboard(
        &mut runtime,
        "bold",
        Some(&selection)
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
fn paste_text_from_clipboard_never_reuses_stale_rich_state_for_external_text() {
    let mut runtime = paragraph_runtime("hello ");

    assert!(paste_text_from_clipboard(&mut runtime, "plain", None));

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
fn paste_text_from_external_clipboard_parses_inline_markdown() {
    let mut runtime = paragraph_runtime("");

    assert!(paste_text_from_clipboard(
        &mut runtime,
        "**bold** and `code`",
        None
    ));

    let payload = runtime.payload_window.get(1).unwrap();
    let BlockPayload::RichText { spans } = &payload.payload else {
        panic!("expected rich text payload");
    };
    assert!(
        spans
            .iter()
            .any(|span| span.text == "bold" && span.marks.contains(&InlineMark::Bold))
    );
    assert!(
        spans
            .iter()
            .any(|span| span.text == "code" && span.marks.contains(&InlineMark::Code))
    );
}

#[test]
fn paste_text_with_rich_metadata_prefers_detected_markdown_structure() {
    let markdown = "- 第一周\n### 阶段一\n- 阅读文档";
    let selection = ClipboardSelection::Inline {
        spans: vec![InlineSpan::plain(markdown)],
    };
    let mut runtime = paragraph_runtime("");

    assert!(paste_text_from_clipboard(
        &mut runtime,
        markdown,
        Some(&selection)
    ));

    let kinds = runtime
        .projection_for_window()
        .blocks
        .iter()
        .map(|block| block.kind.clone())
        .collect::<Vec<_>>();
    assert!(matches!(kinds.first(), Some(RichBlockKind::BulletedList)));
    assert!(matches!(
        kinds.get(1),
        Some(RichBlockKind::Heading { level: 3 })
    ));
    assert!(matches!(kinds.get(2), Some(RichBlockKind::BulletedList)));
}

#[test]
fn paste_text_from_clipboard_uses_validated_table_metadata() {
    let source = table_runtime(1, &[&["a", "b"], &["c", "d"]]);
    let snapshot = source
        .table_clipboard_for_whole_table(1)
        .expect("table clipboard");
    let selection = ClipboardSelection::Table {
        table: snapshot.table.clone(),
    };
    let mut target = table_runtime(2, &[&["x"]]);
    target.focus_table_cell_at_offset(2, 0, 0, 0).unwrap();

    assert!(paste_text_from_clipboard(
        &mut target,
        &snapshot.plain_text,
        Some(&selection)
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
