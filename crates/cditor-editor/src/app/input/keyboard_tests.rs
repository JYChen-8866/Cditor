use super::*;

#[test]
fn mermaid_preview_blocks_hidden_source_mutations() {
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
use crate::{
    text::{ParleyLayoutOptions, RichTextLayoutInput, build_parley_layout},
    theme::GuiTheme,
};
use cditor_core::rich_text::{
    BlockPayload, BlockPayloadRecord, InlineMark, InlineSpan, RichBlockKind, TableCellPayload,
    TablePayload, TableRowPayload,
};
use cditor_editor_protocol::command::{CommandEnvelope, CommandSource, EditorCommand};
use cditor_import_export::clipboard::CditorClipboardEnvelope;
use gpui::{Bounds, point, px, size};

fn dispatch_clipboard_data(
    runtime: &mut DocumentRuntime,
    text: &str,
    selection: Option<&ClipboardSelection>,
) -> bool {
    let metadata_json = selection.map(|selection| {
        serde_json::to_string(&CditorClipboardEnvelope::new(None, selection.clone(), text)).unwrap()
    });
    runtime
        .dispatch(CommandEnvelope::new(
            EditorCommand::ApplyClipboardData {
                text: text.to_owned(),
                metadata_json,
            },
            CommandSource::Keyboard,
        ))
        .unwrap()
        .changed()
}

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
fn keyboard_navigation_consumes_parley_layout_cache() {
    let text = "abc אבג 123";
    let mut runtime = paragraph_runtime(text);
    runtime.focus_block_at_offset(1, 0).unwrap();
    let input = RichTextLayoutInput {
        block_id: 1,
        surface_id: crate::text::TextLayoutSurfaceId::Block(1),
        content_version: runtime.block_content_version(1).unwrap(),
        layout_version: 1,
        kind: RichBlockKind::Paragraph,
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans: vec![InlineSpan::plain(text)],
        width_px: 500.0,
        theme_version: 1,
        font_version: 1,
    };
    let layout = build_parley_layout(
        &input,
        GuiTheme::light(),
        &ParleyLayoutOptions {
            width: Some(500.0),
            base_text_color: GuiTheme::light().text,
            ..ParleyLayoutOptions::default()
        },
    );
    let mut layouts = HashMap::new();
    layouts.insert(
        1,
        RichTextPlatformLayout {
            block_id: 1,
            surface_id: input.surface_id,
            content_version: input.content_version,
            layout_version: input.layout_version,
            input_session_identity: None,
            snapshot: layout,
            accessibility: None,
            bounds: Bounds::new(point(px(0.0), px(0.0)), size(px(500.0), px(24.0))),
            measured_height: 24.0,
            table_cell_position: None,
        },
    );

    assert!(
        move_caret_with_parley(
            &layouts,
            &Default::default(),
            &mut None,
            &mut runtime,
            ParleyMoveCommand::NextVisual,
            false,
        )
        .unwrap()
    );
    let position = runtime.caret_position_for_block(1).unwrap();
    assert!(position.offset > 0);
    assert!(text.is_char_boundary(position.offset));
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

    assert!(dispatch_clipboard_data(
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
fn repeated_vertical_navigation_preserves_original_x_across_a_short_line() {
    let text = "abcdefghij\nx\nabcdefghij";
    let mut runtime = paragraph_runtime(text);
    runtime.focus_block_at_offset(1, 8).unwrap();
    let mut layouts = HashMap::new();
    layouts.insert(
        1,
        crate::text::test_platform_layout(
            1,
            runtime.block_content_version(1).unwrap(),
            text,
            Bounds::new(point(px(0.0), px(0.0)), size(px(500.0), px(80.0))),
            None,
        ),
    );
    let mut preferred_x = None;

    assert!(
        move_caret_with_parley(
            &layouts,
            &Default::default(),
            &mut preferred_x,
            &mut runtime,
            ParleyMoveCommand::NextLine,
            false,
        )
        .unwrap()
    );
    assert!(preferred_x.is_some());
    assert!(
        move_caret_with_parley(
            &layouts,
            &Default::default(),
            &mut preferred_x,
            &mut runtime,
            ParleyMoveCommand::NextLine,
            false,
        )
        .unwrap()
    );
    assert!(runtime.caret_offset_for_block(1).unwrap() >= 20);

    move_caret_with_parley(
        &layouts,
        &Default::default(),
        &mut preferred_x,
        &mut runtime,
        ParleyMoveCommand::PreviousVisual,
        false,
    )
    .unwrap();
    assert!(preferred_x.is_none());
}

#[test]
fn paste_text_from_clipboard_never_reuses_stale_rich_state_for_external_text() {
    let mut runtime = paragraph_runtime("hello ");

    assert!(dispatch_clipboard_data(&mut runtime, "plain", None));

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

    assert!(dispatch_clipboard_data(
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

    assert!(dispatch_clipboard_data(
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

    assert!(dispatch_clipboard_data(
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

    assert!(dispatch_clipboard_data(&mut target, "a\tb\nc\td", None));

    let payload = target.payload_window.get(2).unwrap();
    let BlockPayload::Table(table) = &payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.column_count(), 2);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a"));
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some("d"));
}
