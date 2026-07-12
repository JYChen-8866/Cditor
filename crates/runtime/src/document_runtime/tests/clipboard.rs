use super::*;

fn rich_record(
    block_id: BlockId,
    kind: RichBlockKind,
    spans: Vec<InlineSpan>,
) -> BlockPayloadRecord {
    BlockPayloadRecord {
        block_id,
        content_version: 1,
        kind,
        payload: BlockPayload::RichText { spans },
    }
}

#[test]
fn cross_block_clipboard_preserves_partial_spans_and_block_boundaries() {
    let source = vec![
        rich_record(
            1,
            RichBlockKind::Paragraph,
            vec![
                InlineSpan::plain("a"),
                InlineSpan {
                    text: "b".to_owned(),
                    marks: vec![InlineMark::Bold],
                },
            ],
        ),
        rich_record(
            2,
            RichBlockKind::Quote,
            vec![InlineSpan {
                text: "cd".to_owned(),
                marks: vec![InlineMark::Italic],
            }],
        ),
        rich_record(
            3,
            RichBlockKind::Paragraph,
            vec![
                InlineSpan {
                    text: "e".to_owned(),
                    marks: vec![InlineMark::Underline],
                },
                InlineSpan::plain("f"),
            ],
        ),
    ];
    let mut source = DocumentRuntime::from_payloads(1, source, 720.0);
    source.set_document_text_selection(1, 1, 3, 1).unwrap();
    let selection = source.clipboard_selection_snapshot().unwrap();
    assert_eq!(selection.plain_text(), "b\ncd\ne");

    let mut target = DocumentRuntime::from_payloads(
        2,
        vec![BlockPayloadRecord::rich_text(
            10,
            RichBlockKind::Paragraph,
            "XY",
        )],
        720.0,
    );
    target.focus_block_at_offset(10, 1).unwrap();
    assert!(target.paste_clipboard_selection(&selection).unwrap());

    assert_eq!(target.index.block_ids, vec![10, 11, 12]);
    assert_eq!(target.payload_window.get(10).unwrap().plain_text(), "Xb");
    assert_eq!(target.payload_window.get(11).unwrap().plain_text(), "cd");
    assert_eq!(target.payload_window.get(12).unwrap().plain_text(), "eY");
    assert!(matches!(target.kind_for_block(11), RichBlockKind::Quote));
    let BlockPayload::RichText { spans } = &target.payload_window.get(12).unwrap().payload else {
        panic!("expected rich text");
    };
    assert!(
        spans
            .iter()
            .any(|span| { span.text == "e" && span.marks.contains(&InlineMark::Underline) })
    );
}

#[test]
fn cross_block_clipboard_preserves_first_kind_marks_and_nested_hierarchy() {
    let records = vec![
        BlockIndexRecord::new(
            1,
            None,
            0,
            kind_tag_for_rich_block_kind(&RichBlockKind::Todo { checked: true }),
            0,
        ),
        BlockIndexRecord::new(
            2,
            Some(1),
            1,
            kind_tag_for_rich_block_kind(&RichBlockKind::Todo { checked: false }),
            0,
        ),
        BlockIndexRecord::new(
            3,
            Some(2),
            2,
            kind_tag_for_rich_block_kind(&RichBlockKind::Todo { checked: false }),
            0,
        ),
    ];
    let payloads = vec![
        rich_record(
            1,
            RichBlockKind::Todo { checked: true },
            vec![InlineSpan {
                text: "asd".to_owned(),
                marks: vec![InlineMark::Bold],
            }],
        ),
        rich_record(
            2,
            RichBlockKind::Todo { checked: false },
            vec![InlineSpan::plain("寒夫")],
        ),
        rich_record(
            3,
            RichBlockKind::Todo { checked: false },
            vec![InlineSpan::plain("埃塞")],
        ),
    ];
    let mut source = DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0);
    source
        .set_document_text_selection(1, 0, 3, "埃塞".len())
        .unwrap();
    let selection = source.clipboard_selection_snapshot().unwrap();

    let ClipboardSelection::TextFragments { fragments } = &selection else {
        panic!("expected text fragments");
    };
    assert_eq!(fragments[1].parent_source_id, Some(1));
    assert_eq!(fragments[2].parent_source_id, Some(2));
    assert_eq!(fragments[2].depth, 2);

    let mut target = DocumentRuntime::from_payloads(
        2,
        vec![BlockPayloadRecord::rich_text(
            10,
            RichBlockKind::Paragraph,
            "",
        )],
        720.0,
    );
    target.focus_block_at_offset(10, 0).unwrap();
    assert!(target.paste_clipboard_selection(&selection).unwrap());

    assert_eq!(target.index.block_ids, vec![10, 11, 12]);
    assert_eq!(target.index.parent_ids, vec![None, Some(10), Some(11)]);
    assert_eq!(target.index.depths, vec![0, 1, 2]);
    assert!(matches!(
        target.kind_for_block(10),
        RichBlockKind::Todo { checked: true }
    ));
    assert!(matches!(
        target.kind_for_block(11),
        RichBlockKind::Todo { checked: false }
    ));
    let BlockPayload::RichText { spans } = &target.payload_window.get(10).unwrap().payload else {
        panic!("expected rich text");
    };
    assert!(spans[0].marks.contains(&InlineMark::Bold));
}

#[test]
fn whole_block_clipboard_remaps_ids_and_preserves_complex_payload_hierarchy() {
    let records = vec![
        BlockIndexRecord::new(
            1,
            None,
            0,
            kind_tag_for_rich_block_kind(&RichBlockKind::Toggle),
            0,
        ),
        BlockIndexRecord::new(
            2,
            Some(1),
            1,
            kind_tag_for_rich_block_kind(&RichBlockKind::Whiteboard),
            0,
        ),
    ];
    let payloads = vec![
        BlockPayloadRecord::rich_text(1, RichBlockKind::Toggle, "root"),
        BlockPayloadRecord {
            block_id: 2,
            content_version: 4,
            kind: RichBlockKind::Whiteboard,
            payload: BlockPayload::Whiteboard(cditor_core::rich_text::WhiteboardPayload {
                scene_json: r#"{"elements":[{"id":"shape-1"}]}"#.to_owned(),
            }),
        },
    ];
    let mut source = DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0);
    assert!(source.select_visible_block_range(1, 2));
    let selection = source.clipboard_selection_snapshot().unwrap();

    let mut target = DocumentRuntime::from_payloads(
        2,
        vec![BlockPayloadRecord::rich_text(
            10,
            RichBlockKind::Paragraph,
            "anchor",
        )],
        720.0,
    );
    target.focus_block_at_offset(10, 6).unwrap();
    assert!(target.paste_clipboard_selection(&selection).unwrap());

    assert_eq!(target.index.block_ids, vec![10, 11, 12]);
    assert_eq!(
        target.index.parent_ids[target.index.index_of(12).unwrap()],
        Some(11)
    );
    let BlockPayload::Whiteboard(whiteboard) = &target.payload_window.get(12).unwrap().payload
    else {
        panic!("expected whiteboard payload");
    };
    assert!(whiteboard.scene_json.contains("shape-1"));

    assert!(target.undo_focused_block().is_ok());
    assert_eq!(target.index.block_ids, vec![10]);
}

#[test]
fn table_clipboard_metadata_preserves_cell_marks_and_style() {
    let mut table = cditor_core::rich_text::TablePayload {
        rows: vec![cditor_core::rich_text::TableRowPayload {
            cells: vec![cditor_core::rich_text::TableCellPayload {
                spans: vec![InlineSpan {
                    text: "styled".to_owned(),
                    marks: vec![InlineMark::Bold],
                }],
                style: cditor_core::rich_text::TableCellStyle {
                    background_color: Some("#ff0000".to_owned()),
                },
                ..Default::default()
            }],
            height: Default::default(),
        }],
        ..Default::default()
    };
    table.normalize();
    let selection = ClipboardSelection::Table { table };
    let mut target = DocumentRuntime::from_payloads(2, vec![sample_table_payload()], 720.0);
    target.focus_table_cell_at_offset(10, 0, 0, 0).unwrap();
    assert!(target.paste_clipboard_selection(&selection).unwrap());

    let BlockPayload::Table(table) = &target.payload_window.get(10).unwrap().payload else {
        panic!("expected table");
    };
    let cell = &table.rows[0].cells[0];
    assert!(cell.spans[0].marks.contains(&InlineMark::Bold));
    assert_eq!(cell.style.background_color.as_deref(), Some("#ff0000"));
}

#[test]
fn table_cut_clear_preserves_cell_style() {
    let mut runtime = DocumentRuntime::from_payloads(2, vec![sample_table_payload()], 720.0);
    let range = TableRange::normalized(0, 0, 0, 1);
    runtime
        .set_table_cell_background_color(10, range, Some("#00ff00".to_owned()))
        .unwrap();
    assert!(runtime.clear_table_range(10, range).unwrap());

    let BlockPayload::Table(table) = &runtime.payload_window.get(10).unwrap().payload else {
        panic!("expected table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some(""));
    assert_eq!(
        table.rows[0].cells[0].style.background_color.as_deref(),
        Some("#00ff00")
    );
}

#[test]
fn inline_clipboard_paste_into_table_cell_preserves_marks() {
    let mut runtime = DocumentRuntime::from_payloads(2, vec![sample_table_payload()], 720.0);
    runtime.focus_table_cell_at_offset(10, 0, 0, 1).unwrap();
    let selection = ClipboardSelection::Inline {
        spans: vec![InlineSpan {
            text: "bold".to_owned(),
            marks: vec![InlineMark::Bold],
        }],
    };
    assert!(runtime.paste_clipboard_selection(&selection).unwrap());

    let BlockPayload::Table(table) = &runtime.payload_window.get(10).unwrap().payload else {
        panic!("expected table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("Abold"));
    assert!(
        table.rows[0].cells[0]
            .spans
            .iter()
            .any(|span| span.text == "bold" && span.marks.contains(&InlineMark::Bold))
    );
}
