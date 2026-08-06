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
    text::{RichTextLayoutInput, TextLayoutOptions, build_text_layout},
    theme::GuiTheme,
};
use cditor_core::clipboard::{
    CditorClipboardEnvelope, ClipboardBlock, ClipboardBlockFragment, ClipboardFragmentBoundary,
};
use cditor_core::rich_text::{
    BlockPayload, BlockPayloadRecord, InlineMark, InlineSpan, RichBlockKind, TableCellPayload,
    TablePayload, TableRowPayload,
};
use gpui::{Bounds, point, px, size};

fn dispatch_clipboard_data(
    runtime: &mut DocumentRuntime,
    text: &str,
    selection: Option<&ClipboardSelection>,
) -> bool {
    let metadata_json = selection.map(|selection| {
        serde_json::to_string(&CditorClipboardEnvelope::new(None, selection.clone(), text)).unwrap()
    });
    cditor_session::project_clipboard_import(runtime, text, metadata_json.as_deref())
        .unwrap()
        .outcome
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
    runtime.focus_block(1);
    crate::test_support::focus_block_at_offset(&mut runtime, 1, text.len());
    runtime
}

fn text_block_runtime(kind: RichBlockKind, payload: BlockPayload) -> DocumentRuntime {
    let text_len = payload.plain_text().len();
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind,
            payload,
        }],
        720.0,
    );
    runtime.focus_block(1);
    crate::test_support::focus_block_at_offset(&mut runtime, 1, text_len);
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
fn keyboard_navigation_consumes_text_layout_layout_cache() {
    let text = "abc אבג 123";
    let mut runtime = paragraph_runtime(text);
    crate::test_support::focus_block_at_offset(&mut runtime, 1, 0);
    let input = RichTextLayoutInput {
        block_id: 1,
        surface_id: crate::text::TextLayoutSurfaceId::Block(1),
        content_version: runtime.block_content_version(1).unwrap(),
        layout_version: runtime.block_layout_version(1).unwrap(),
        kind: RichBlockKind::Paragraph,
        text_align: cditor_core::rich_text::TextAlign::Start,
        spans: vec![InlineSpan::plain(text)].into(),
        width_px: 500.0,
        theme_version: 1,
        font_version: 1,
    };
    let layout = build_text_layout(
        &input,
        GuiTheme::light(),
        &TextLayoutOptions {
            width: Some(500.0),
            base_text_color: GuiTheme::light().text,
            ..TextLayoutOptions::default()
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
            wrap_width_px: 500.0,
            text_align: input.text_align,
            input_session_identity: None,
            snapshot: layout.into(),
            accessibility: None,
            bounds: Bounds::new(point(px(0.0), px(0.0)), size(px(500.0), px(24.0))),
            measured_height: 24.0,
            table_cell_position: None,
        },
    );

    let session = cditor_session::EditorSession::new(runtime, false).into_handle();
    assert!(
        move_caret_with_text_layout(
            &layouts,
            &Default::default(),
            &mut None,
            &session,
            TextLayoutMoveCommand::NextVisual,
            false,
        )
        .unwrap()
    );
    let offset = session
        .text_block_context(1)
        .unwrap()
        .unwrap()
        .caret
        .unwrap();
    assert!(offset > 0);
    assert!(text.is_char_boundary(offset));

    layouts.get_mut(&1).unwrap().layout_version = session
        .surface_version(SurfaceId::Block(1))
        .unwrap()
        .unwrap()
        .layout_version
        .saturating_add(1);
    assert!(
        !move_caret_with_text_layout(
            &layouts,
            &Default::default(),
            &mut None,
            &session,
            TextLayoutMoveCommand::NextVisual,
            false,
        )
        .unwrap(),
        "a stale layout version must not answer visual navigation"
    );
    assert_eq!(
        session.text_block_context(1).unwrap().unwrap().caret,
        Some(offset)
    );
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

    let payload = runtime.block_payload_record(1).unwrap();
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
    crate::test_support::focus_block_at_offset(&mut runtime, 1, 8);
    let mut layouts = HashMap::new();
    let mut layout = crate::text::test_platform_layout(
        1,
        runtime.block_content_version(1).unwrap(),
        text,
        Bounds::new(point(px(0.0), px(0.0)), size(px(500.0), px(80.0))),
        None,
    );
    layout.layout_version = runtime.block_layout_version(1).unwrap();
    layouts.insert(1, layout);
    let mut preferred_x = None;
    let session = cditor_session::EditorSession::new(runtime, false).into_handle();

    assert!(
        move_caret_with_text_layout(
            &layouts,
            &Default::default(),
            &mut preferred_x,
            &session,
            TextLayoutMoveCommand::NextLine,
            false,
        )
        .unwrap()
    );
    assert!(preferred_x.is_some());
    assert!(
        move_caret_with_text_layout(
            &layouts,
            &Default::default(),
            &mut preferred_x,
            &session,
            TextLayoutMoveCommand::NextLine,
            false,
        )
        .unwrap()
    );
    assert!(
        session
            .text_block_context(1)
            .unwrap()
            .unwrap()
            .caret
            .unwrap()
            >= 20
    );

    move_caret_with_text_layout(
        &layouts,
        &Default::default(),
        &mut preferred_x,
        &session,
        TextLayoutMoveCommand::PreviousVisual,
        false,
    )
    .unwrap();
    assert!(preferred_x.is_none());
}

#[test]
fn paste_text_from_clipboard_never_reuses_stale_rich_state_for_external_text() {
    let mut runtime = paragraph_runtime("hello ");

    assert!(dispatch_clipboard_data(&mut runtime, "plain", None));

    let payload = runtime.block_payload_record(1).unwrap();
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
fn paste_external_markdown_into_paragraph_parses_inline_marks() {
    let mut runtime = paragraph_runtime("");
    assert!(runtime.input_session_target().is_some());

    assert!(dispatch_clipboard_data(
        &mut runtime,
        "**bold** and `code`",
        None
    ));

    let payload = runtime.block_payload_record(1).unwrap();
    let BlockPayload::RichText { spans } = &payload.payload else {
        panic!("expected rich text payload");
    };
    assert_eq!(payload.plain_text(), "bold and code");
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
fn paste_external_space_after_markdown_marker_does_not_transform_block() {
    let mut runtime = paragraph_runtime("#");

    assert!(dispatch_clipboard_data(&mut runtime, " ", None));

    let payload = runtime.block_payload_record(1).unwrap();
    assert!(matches!(payload.kind, RichBlockKind::Paragraph));
    assert_eq!(payload.plain_text(), "# ");
}

#[test]
fn multiline_markdown_paste_into_code_stays_literal_inside_code_block() {
    let mut runtime = text_block_runtime(
        RichBlockKind::Code {
            language: Some("rust".to_owned()),
        },
        BlockPayload::Code {
            language: Some("rust".to_owned()),
            text: "let value = ".to_owned(),
        },
    );

    assert!(dispatch_clipboard_data(
        &mut runtime,
        "# source\n- first\n- second",
        None
    ));

    let payload = runtime.block_payload_record(1).unwrap();
    assert!(matches!(
        &payload.kind,
        RichBlockKind::Code { language } if language.as_deref() == Some("rust")
    ));
    assert!(matches!(
        &payload.payload,
        BlockPayload::Code { language, text }
            if language.as_deref() == Some("rust")
                && text == "let value = # source\n- first\n- second"
    ));
    assert_eq!(runtime.projection_for_window().blocks.len(), 1);
}

#[test]
fn multiline_external_paste_into_heading_parses_markdown_blocks() {
    let mut runtime = text_block_runtime(
        RichBlockKind::Heading { level: 2 },
        BlockPayload::RichText {
            spans: vec![InlineSpan::plain("Title ")],
        },
    );

    assert!(dispatch_clipboard_data(
        &mut runtime,
        "**continued**\n- child",
        None
    ));

    let blocks = runtime.projection_for_window().blocks;
    assert_eq!(blocks.len(), 2);
    let first = runtime.block_payload_record(1).unwrap();
    assert!(matches!(first.kind, RichBlockKind::Paragraph));
    assert_eq!(first.plain_text(), "Title continued");
    let BlockPayload::RichText { spans } = &first.payload else {
        panic!("expected rich text payload");
    };
    assert!(
        spans
            .iter()
            .any(|span| span.text == "continued" && span.marks.contains(&InlineMark::Bold))
    );
    let second = runtime.block_payload_record(blocks[1].block_id).unwrap();
    assert!(matches!(second.kind, RichBlockKind::BulletedList));
    assert_eq!(second.plain_text(), "child");
}

#[test]
fn multiline_external_paste_into_quote_parses_markdown_blocks() {
    let mut runtime = text_block_runtime(
        RichBlockKind::Quote,
        BlockPayload::RichText {
            spans: vec![InlineSpan::plain("Preamble ")],
        },
    );

    assert!(dispatch_clipboard_data(
        &mut runtime,
        "> Quoted\n\n- child",
        None
    ));

    let blocks = runtime.projection_for_window().blocks;
    assert_eq!(blocks.len(), 2);
    let first = runtime.block_payload_record(1).unwrap();
    assert!(matches!(first.kind, RichBlockKind::Quote));
    assert_eq!(first.plain_text(), "Preamble Quoted");
    let second = runtime.block_payload_record(blocks[1].block_id).unwrap();
    assert!(matches!(second.kind, RichBlockKind::BulletedList));
    assert_eq!(second.plain_text(), "child");
}

#[test]
fn internal_block_clipboard_wins_over_markdown_detection_and_preserves_style() {
    let selection = ClipboardSelection::Blocks {
        blocks: vec![
            ClipboardBlock {
                source_id: 10,
                parent_source_id: None,
                depth: 0,
                kind: RichBlockKind::Heading { level: 2 },
                payload: BlockPayload::RichText {
                    spans: vec![InlineSpan {
                        text: "Heading".to_owned(),
                        marks: vec![InlineMark::Bold],
                    }],
                },
            },
            ClipboardBlock {
                source_id: 11,
                parent_source_id: None,
                depth: 0,
                kind: RichBlockKind::Quote,
                payload: BlockPayload::RichText {
                    spans: vec![InlineSpan {
                        text: "Quoted".to_owned(),
                        marks: vec![InlineMark::Italic],
                    }],
                },
            },
        ],
    };
    let text = selection.plain_text();
    let mut runtime = paragraph_runtime("");

    assert!(dispatch_clipboard_data(
        &mut runtime,
        &text,
        Some(&selection)
    ));

    assert_eq!(runtime.projection_for_window().blocks.len(), 3);
    let heading = runtime.block_payload_record(2).unwrap();
    assert!(matches!(heading.kind, RichBlockKind::Heading { level: 2 }));
    let BlockPayload::RichText { spans } = &heading.payload else {
        panic!("expected rich text heading");
    };
    assert!(spans[0].marks.contains(&InlineMark::Bold));
    let quote = runtime.block_payload_record(3).unwrap();
    assert!(matches!(quote.kind, RichBlockKind::Quote));
    let BlockPayload::RichText { spans } = &quote.payload else {
        panic!("expected rich text quote");
    };
    assert!(spans[0].marks.contains(&InlineMark::Italic));
}

#[test]
fn internal_whole_document_clipboard_preserves_blocks_and_marks() {
    let selection = ClipboardSelection::TextFragments {
        fragments: vec![
            ClipboardBlockFragment {
                source_id: 10,
                parent_source_id: None,
                depth: 0,
                kind: RichBlockKind::Heading { level: 1 },
                spans: vec![InlineSpan {
                    text: "Title".to_owned(),
                    marks: vec![InlineMark::Bold],
                }],
                boundary: ClipboardFragmentBoundary::StartPartial,
                starts_at_block_start: true,
                ends_at_block_end: true,
            },
            ClipboardBlockFragment {
                source_id: 11,
                parent_source_id: None,
                depth: 0,
                kind: RichBlockKind::Quote,
                spans: vec![InlineSpan {
                    text: "Body".to_owned(),
                    marks: vec![InlineMark::Italic],
                }],
                boundary: ClipboardFragmentBoundary::EndPartial,
                starts_at_block_start: true,
                ends_at_block_end: true,
            },
        ],
    };
    let text = selection.plain_text();
    let mut runtime = paragraph_runtime("");

    assert!(dispatch_clipboard_data(
        &mut runtime,
        &text,
        Some(&selection)
    ));

    let blocks = runtime.projection_for_window().blocks;
    assert_eq!(blocks.len(), 2);
    assert!(matches!(
        blocks[0].kind,
        RichBlockKind::Heading { level: 1 }
    ));
    assert!(matches!(blocks[1].kind, RichBlockKind::Quote));
    let BlockPayload::RichText { spans } = &runtime.block_payload_record(1).unwrap().payload else {
        panic!("expected rich text heading");
    };
    assert!(spans[0].marks.contains(&InlineMark::Bold));
    let BlockPayload::RichText { spans } = &runtime.block_payload_record(2).unwrap().payload else {
        panic!("expected rich text quote");
    };
    assert!(spans[0].marks.contains(&InlineMark::Italic));
}

#[test]
fn external_markdown_at_document_selection_creates_markdown_blocks() {
    let mut runtime = paragraph_runtime("");
    crate::test_support::select_block_range(&mut runtime, 1, 1);

    assert!(dispatch_clipboard_data(
        &mut runtime,
        "# Heading\n\n> Quoted",
        None
    ));

    let blocks = runtime.projection_for_window().blocks;
    assert_eq!(blocks.len(), 2);
    assert!(matches!(
        blocks[0].kind,
        RichBlockKind::Heading { level: 1 }
    ));
    assert!(matches!(blocks[1].kind, RichBlockKind::Quote));
    assert_eq!(
        runtime.block_payload_record(1).unwrap().plain_text(),
        "Heading"
    );
    assert_eq!(
        runtime
            .block_payload_record(blocks[1].block_id)
            .unwrap()
            .plain_text(),
        "Quoted"
    );
}

#[test]
fn paste_markdown_with_separator_creates_multiple_blocks() {
    let mut runtime = paragraph_runtime("");

    assert!(dispatch_clipboard_data(
        &mut runtime,
        "# Success\nline1\nline2\nline3\nline4\n---",
        None
    ));

    let blocks = runtime.projection_for_window().blocks;
    assert_eq!(blocks.len(), 6);
    assert!(matches!(
        blocks[0].kind,
        RichBlockKind::Heading { level: 1 }
    ));
    assert!(matches!(blocks[5].kind, RichBlockKind::Separator));
    assert!(
        blocks[1..5]
            .iter()
            .all(|block| matches!(block.kind, RichBlockKind::Paragraph))
    );
}

#[test]
fn paste_text_with_rich_metadata_stays_inside_active_text_block() {
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

    let blocks = runtime.projection_for_window().blocks;
    assert_eq!(blocks.len(), 1);
    assert!(matches!(blocks[0].kind, RichBlockKind::Paragraph));
    assert_eq!(
        runtime.block_payload_record(1).unwrap().plain_text(),
        markdown
    );
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
    crate::test_support::focus_table_cell_at_offset(&mut target, 2, 0, 0, 0);

    assert!(dispatch_clipboard_data(
        &mut target,
        &snapshot.plain_text,
        Some(&selection)
    ));

    let payload = target.block_payload_record(2).unwrap();
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
    crate::test_support::focus_table_cell_at_offset(&mut target, 2, 0, 0, 0);

    assert!(dispatch_clipboard_data(&mut target, "a\tb\nc\td", None));

    let payload = target.block_payload_record(2).unwrap();
    let BlockPayload::Table(table) = &payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.column_count(), 2);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a"));
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some("d"));
}
