use cditor_core::block::BlockPrefixSnapshot;

use super::*;

fn runtime_with_paragraph_blocks(count: usize) -> DocumentRuntime {
    let records = (1..=count as BlockId)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
            .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(block_id, 32.0))
        })
        .collect::<Vec<_>>();
    let payloads = (1..=count as BlockId)
        .map(|block_id| BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, ""))
        .collect::<Vec<_>>();
    DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0)
}

fn runtime_with_kind_depths(
    kinds_and_depths: Vec<(RichBlockKind, u16, Option<BlockId>)>,
) -> DocumentRuntime {
    runtime_with_kind_depths_and_text(
        kinds_and_depths
            .into_iter()
            .map(|(kind, depth, parent_id)| (kind, depth, parent_id, "item"))
            .collect(),
    )
}

fn runtime_with_kind_depths_and_text(
    blocks: Vec<(RichBlockKind, u16, Option<BlockId>, &str)>,
) -> DocumentRuntime {
    let records = blocks
        .iter()
        .enumerate()
        .map(|(index, (kind, depth, parent_id, _text))| {
            let block_id = (index + 1) as BlockId;
            BlockIndexRecord::new(
                block_id,
                *parent_id,
                *depth,
                kind_tag_for_rich_block_kind(kind),
                0,
            )
            .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(block_id, 32.0))
        })
        .collect::<Vec<_>>();
    let payloads = blocks
        .into_iter()
        .enumerate()
        .map(|(index, (kind, _, _, text))| {
            BlockPayloadRecord::rich_text((index + 1) as BlockId, kind, text)
        })
        .collect::<Vec<_>>();
    DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0)
}

fn runtime_with_rich_spans(spans: Vec<InlineSpan>) -> DocumentRuntime {
    let record = BlockIndexRecord::new(
        1,
        None,
        0,
        kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
        0,
    )
    .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(1, 32.0));
    let payload = BlockPayloadRecord {
        block_id: 1,
        content_version: 1,
        kind: RichBlockKind::Paragraph,
        payload: BlockPayload::RichText { spans },
    };
    DocumentRuntime::from_index_records(1, vec![record], vec![payload], 1, 720.0)
}

#[test]
fn convert_focused_block_kind_preserves_text_for_slash_menu() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "hello")]);
    runtime.focus_block(1);

    assert!(
        runtime
            .convert_focused_block_kind(RichBlockKind::Heading { level: 2 })
            .unwrap()
    );

    let projection = runtime.projection_for_window();
    assert_eq!(
        projection.blocks[0].kind,
        RichBlockKind::Heading { level: 2 }
    );
    assert_eq!(runtime.focused_text(), Some("hello"));
}

#[test]
fn convert_focused_block_kind_to_table_creates_default_2_by_2_grid() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "hello")]);
    runtime.focus_block(1);
    let document_index = runtime.index.index_of(1).unwrap();
    runtime.index.layout_meta[document_index].update_height(36.0);
    runtime.height_index.update_height(0, 36.0).unwrap();
    runtime.queue_measured_height(1, 1, 48.0).unwrap();

    assert!(
        runtime
            .convert_focused_block_kind(RichBlockKind::Table)
            .unwrap()
    );

    let payload = runtime.payload_window.get(1).unwrap();
    let BlockPayload::Table(table) = &payload.payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.rows.len(), 2);
    assert!(table.rows.iter().all(|row| row.cells.len() == 2));
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("hello"));
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(1, 0).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some(""));
    let projection = runtime.projection_for_window();
    let document_index = runtime.index.index_of(1).unwrap();
    assert_eq!(
        runtime.index.layout_meta[document_index].measured_height,
        None
    );
    assert!(!runtime.pending_measured_heights.contains_key(&1));
    assert!(
        projection.blocks[0].layout.effective_height() >= 120.0,
        "converted table should not keep paragraph height: {}",
        projection.blocks[0].layout.effective_height()
    );
}

#[test]
fn convert_focused_table_block_to_paragraph_exports_cell_plain_text() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime.focus_block(10);

    assert!(
        runtime
            .convert_focused_block_kind(RichBlockKind::Paragraph)
            .unwrap()
    );

    let payload = runtime.block_payload_record(10).unwrap();
    assert_eq!(payload.kind, RichBlockKind::Paragraph);
    let BlockPayload::RichText { spans } = payload.payload else {
        panic!("expected paragraph payload");
    };
    assert_eq!(
        cditor_core::rich_text::plain_text_from_spans(&spans),
        "A\tB\nC\tD"
    );
    assert!(runtime.table_runtime(10).is_none());
    assert_eq!(runtime.focused_text(), Some("A\tB\nC\tD"));
}

#[test]
fn selected_focused_rich_text_keeps_inline_marks_for_internal_clipboard() {
    let mut runtime = runtime_with_rich_spans(vec![
        InlineSpan::plain("a "),
        InlineSpan {
            text: "bold".to_owned(),
            marks: vec![InlineMark::Bold],
        },
        InlineSpan::plain(" c"),
    ]);
    runtime.focus_block_at_offset(1, 2).unwrap();
    runtime.set_document_text_selection(1, 2, 1, 6).unwrap();

    let snapshot = runtime.selected_focused_rich_text().unwrap();
    assert_eq!(snapshot.text, "bold");
    assert_eq!(snapshot.spans.len(), 1);
    assert_eq!(snapshot.spans[0].text, "bold");
    assert!(snapshot.spans[0].marks.contains(&InlineMark::Bold));
}

#[test]
fn replace_focused_range_with_rich_text_spans_preserves_inserted_marks() {
    let mut runtime = runtime_with_rich_spans(vec![InlineSpan::plain("hello")]);
    runtime.focus_block_at_offset(1, 5).unwrap();

    assert!(
        runtime
            .replace_focused_range_with_rich_text_spans(&[InlineSpan {
                text: " bold".to_owned(),
                marks: vec![InlineMark::Bold],
            }])
            .unwrap()
    );

    let payload = runtime.payload_window.get(1).unwrap();
    match &payload.payload {
        BlockPayload::RichText { spans } => {
            assert_eq!(payload.plain_text(), "hello bold");
            assert!(
                spans
                    .iter()
                    .any(|span| span.text == " bold" && span.marks.contains(&InlineMark::Bold))
            );
        }
        _ => panic!("expected rich text payload"),
    }
}

#[test]
fn set_code_block_language_updates_kind_and_payload() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            payload: BlockPayload::Code {
                language: Some("rust".to_owned()),
                text: "fn main() {}".to_owned(),
            },
        }],
        720.0,
    );

    assert!(
        runtime
            .set_code_block_language(1, Some(" TypeScript ".to_owned()))
            .unwrap()
    );
    let payload = runtime.payload_window.get(1).unwrap();
    assert!(matches!(
        &payload.kind,
        RichBlockKind::Code { language } if language.as_deref() == Some("typescript")
    ));
    assert!(matches!(
        &payload.payload,
        BlockPayload::Code { language, text }
            if language.as_deref() == Some("typescript") && text == "fn main() {}"
    ));

    assert!(
        runtime
            .set_code_block_language(1, Some(" ".to_owned()))
            .unwrap()
    );
    let payload = runtime.payload_window.get(1).unwrap();
    assert!(matches!(
        &payload.kind,
        RichBlockKind::Code { language } if language.is_none()
    ));
}

#[test]
fn measured_height_marks_layout_dirty_until_saved() {
    let mut runtime = runtime_with_single_payload(
        RichBlockKind::Image,
        BlockPayload::Image(ImagePayload {
            source: "/tmp/paste.png".to_owned(),
            alt: "paste.png".to_owned(),
            caption: String::new(),
            display_width_ratio_milli: None,
        }),
    );

    assert!(!runtime.has_dirty_layout());
    assert!(runtime.apply_measured_height(1, 1, 512.0).unwrap());
    assert!(runtime.has_dirty_layout());
    runtime.mark_layout_saved();
    assert!(!runtime.has_dirty_layout());
}

#[test]
fn image_projection_clamps_legacy_short_layout_height() {
    let mut runtime = runtime_with_single_payload(
        RichBlockKind::Image,
        BlockPayload::Image(ImagePayload {
            source: "/tmp/paste.png".to_owned(),
            alt: "paste.png".to_owned(),
            caption: String::new(),
            display_width_ratio_milli: None,
        }),
    );
    runtime.index.layout_meta[0].estimated_height = 220.0;

    let projection = runtime.projection_for_window_planned();

    assert_eq!(
        projection.blocks[0].layout.effective_height(),
        IMAGE_BLOCK_ESTIMATED_HEIGHT_PX
    );
}

#[test]
fn image_asset_insert_creates_image_block_and_trailing_paragraph() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "hello")]);
    runtime.focus_block_at_offset(1, 5).unwrap();

    let (image_block_id, trailing_block_id) = runtime
        .insert_image_asset_after_focused(ImagePayload {
            source: "/tmp/paste.png".to_owned(),
            alt: "paste.png".to_owned(),
            caption: String::new(),
            display_width_ratio_milli: None,
        })
        .unwrap();

    assert_eq!(runtime.index.total_count(), 3);
    assert_eq!(runtime.kind_at_index(1), RichBlockKind::Image);
    assert_eq!(runtime.kind_at_index(2), RichBlockKind::Paragraph);
    assert_eq!(runtime.focused_block_id(), Some(trailing_block_id));
    let image_payload = runtime.block_payload_record(image_block_id).unwrap();
    assert!(matches!(image_payload.payload, BlockPayload::Image(_)));
}

#[test]
fn markdown_paste_heading_replaces_current_block_and_preserves_prefix_suffix() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "hello world")]);
    runtime.focus_block_at_offset(1, 5).unwrap();

    assert!(runtime.insert_markdown_paste("# Title").unwrap());

    assert_eq!(runtime.index.total_count(), 1);
    assert_eq!(
        runtime.kind_at_index(0),
        RichBlockKind::Heading { level: 1 }
    );
    assert_eq!(
        runtime.payload_window.get(1).unwrap().plain_text(),
        "helloTitle world"
    );
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.caret_offset_for_block(1), Some("helloTitle".len()));
}

#[test]
fn markdown_paste_multiline_list_inserts_structured_siblings() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![(
        RichBlockKind::Paragraph,
        0,
        None,
        "prefix suffix",
    )]);
    runtime.focus_block_at_offset(1, 7).unwrap();

    assert!(runtime.insert_markdown_paste("- one\n- two").unwrap());

    assert_eq!(runtime.index.total_count(), 2);
    assert_eq!(runtime.kind_at_index(0), RichBlockKind::BulletedList);
    assert_eq!(runtime.kind_at_index(1), RichBlockKind::BulletedList);
    assert_eq!(
        runtime.payload_window.get(1).unwrap().plain_text(),
        "prefix one"
    );
    assert_eq!(
        runtime.payload_window.get(3).unwrap().plain_text(),
        "twosuffix"
    );
    assert_eq!(runtime.focused_block_id(), Some(3));
    assert_eq!(runtime.caret_offset_for_block(3), Some("two".len()));
}

#[test]
fn markdown_paste_detection_scans_all_lines_like_v1() {
    assert!(cditor_core::rich_text::looks_like_markdown_paste(
        "plain intro\n- item"
    ));
}

#[test]
fn markdown_paste_deletes_cross_block_selection_and_undo_restores_it() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "abc"),
        (RichBlockKind::Paragraph, 0, None, "def"),
        (RichBlockKind::Paragraph, 0, None, "ghi"),
    ]);
    runtime.set_document_text_selection(1, 1, 3, 1).unwrap();

    assert!(runtime.insert_markdown_paste("- x\n- y").unwrap());
    assert_eq!(runtime.index.total_count(), 2);
    assert_eq!(runtime.kind_at_index(0), RichBlockKind::BulletedList);
    assert_eq!(runtime.kind_at_index(1), RichBlockKind::BulletedList);
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "ax");
    assert_eq!(runtime.payload_window.get(5).unwrap().plain_text(), "yhi");

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.index.total_count(), 3);
    assert_eq!(runtime.kind_at_index(0), RichBlockKind::Paragraph);
    assert_eq!(runtime.kind_at_index(1), RichBlockKind::Paragraph);
    assert_eq!(runtime.kind_at_index(2), RichBlockKind::Paragraph);
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "abc");
    assert_eq!(runtime.payload_window.get(2).unwrap().plain_text(), "def");
    assert_eq!(runtime.payload_window.get(3).unwrap().plain_text(), "ghi");
    assert_eq!(runtime.focused_block_id(), Some(3));
    assert_eq!(runtime.caret_offset_for_block(3), Some(1));

    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(runtime.index.total_count(), 2);
    assert_eq!(runtime.kind_at_index(0), RichBlockKind::BulletedList);
    assert_eq!(runtime.kind_at_index(1), RichBlockKind::BulletedList);
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "ax");
    assert_eq!(runtime.payload_window.get(5).unwrap().plain_text(), "yhi");
}

#[test]
fn markdown_paste_undo_redo_restores_structure_and_payloads() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![(
        RichBlockKind::Paragraph,
        0,
        None,
        "prefix suffix",
    )]);
    runtime.focus_block_at_offset(1, 7).unwrap();

    assert!(runtime.insert_markdown_paste("- one\n- two").unwrap());
    assert_eq!(runtime.index.total_count(), 2);
    assert_eq!(runtime.kind_at_index(0), RichBlockKind::BulletedList);

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.index.total_count(), 1);
    assert_eq!(runtime.kind_at_index(0), RichBlockKind::Paragraph);
    assert_eq!(
        runtime.payload_window.get(1).unwrap().plain_text(),
        "prefix suffix"
    );
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.caret_offset_for_block(1), Some("prefix ".len()));

    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(runtime.index.total_count(), 2);
    assert_eq!(runtime.kind_at_index(0), RichBlockKind::BulletedList);
    assert_eq!(runtime.kind_at_index(1), RichBlockKind::BulletedList);
    assert_eq!(
        runtime.payload_window.get(1).unwrap().plain_text(),
        "prefix one"
    );
    assert_eq!(
        runtime.payload_window.get(3).unwrap().plain_text(),
        "twosuffix"
    );
    assert_eq!(runtime.focused_block_id(), Some(3));
    assert_eq!(runtime.caret_offset_for_block(3), Some("two".len()));
}

#[test]
fn markdown_paste_table_with_suffix_adds_trailing_paragraph() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![(
        RichBlockKind::Paragraph,
        0,
        None,
        "before after",
    )]);
    runtime.focus_block_at_offset(1, 7).unwrap();

    assert!(
        runtime
            .insert_markdown_paste("| a | b |\n| - | - |")
            .unwrap()
    );

    assert_eq!(runtime.index.total_count(), 3);
    assert_eq!(runtime.kind_at_index(0), RichBlockKind::Paragraph);
    assert_eq!(runtime.kind_at_index(1), RichBlockKind::Table);
    assert_eq!(runtime.kind_at_index(2), RichBlockKind::Paragraph);
    assert_eq!(
        runtime.payload_window.get(1).unwrap().plain_text(),
        "before "
    );
    assert_eq!(runtime.payload_window.get(3).unwrap().plain_text(), "after");
    assert_eq!(runtime.focused_block_id(), Some(3));
    assert_eq!(runtime.caret_offset_for_block(3), Some("after".len()));
}

#[test]
fn enter_on_bulleted_list_splits_and_inherits_kind() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![(
        RichBlockKind::BulletedList,
        0,
        None,
        "hello world",
    )]);
    runtime.focus_block_at_offset(1, 5).unwrap();

    runtime.handle_enter().unwrap();

    assert_eq!(runtime.index.total_count(), 2);
    assert_eq!(runtime.kind_at_index(0), RichBlockKind::BulletedList);
    assert_eq!(runtime.kind_at_index(1), RichBlockKind::BulletedList);
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "hello");
    assert_eq!(
        runtime.payload_window.get(2).unwrap().plain_text(),
        " world"
    );
    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(runtime.caret_offset_for_block(2), Some(0));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::BlockText { block_id: 2 })
    );
    assert_eq!(runtime.input_session_selected_range(), Some(0..0));
}

#[test]
fn enter_on_numbered_list_splits_and_inherits_kind() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::NumberedList, 0, None, "one two")]);
    runtime.focus_block_at_offset(1, 3).unwrap();

    runtime.handle_enter().unwrap();

    assert_eq!(runtime.kind_at_index(0), RichBlockKind::NumberedList);
    assert_eq!(runtime.kind_at_index(1), RichBlockKind::NumberedList);
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "one");
    assert_eq!(runtime.payload_window.get(2).unwrap().plain_text(), " two");
    let projection = runtime.projection_for_window_planned();
    assert_eq!(
        projection.blocks[0].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 1 }
    );
    assert_eq!(
        projection.blocks[1].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 2 }
    );
}

#[test]
fn enter_on_todo_splits_and_new_item_is_unchecked() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![(
        RichBlockKind::Todo { checked: true },
        0,
        None,
        "done later",
    )]);
    runtime.focus_block_at_offset(1, 4).unwrap();

    runtime.handle_enter().unwrap();

    assert_eq!(
        runtime.kind_at_index(0),
        RichBlockKind::Todo { checked: true }
    );
    assert_eq!(
        runtime.kind_at_index(1),
        RichBlockKind::Todo { checked: false }
    );
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "done");
    assert_eq!(
        runtime.payload_window.get(2).unwrap().plain_text(),
        " later"
    );
}

#[test]
fn enter_splits_trailing_rich_spans_and_preserves_marks() {
    let record = BlockIndexRecord::new(
        1,
        None,
        0,
        kind_tag_for_rich_block_kind(&RichBlockKind::BulletedList),
        0,
    )
    .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(1, 32.0));
    let payload = BlockPayloadRecord {
        block_id: 1,
        content_version: 1,
        kind: RichBlockKind::BulletedList,
        payload: BlockPayload::RichText {
            spans: vec![
                InlineSpan::plain("ab"),
                InlineSpan {
                    text: "cd".to_owned(),
                    marks: vec![InlineMark::Bold],
                },
            ],
        },
    };
    let mut runtime = DocumentRuntime::from_index_records(1, vec![record], vec![payload], 1, 720.0);
    runtime.focus_block_at_offset(1, 3).unwrap();

    runtime.handle_enter().unwrap();

    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "abc");
    assert_eq!(runtime.payload_window.get(2).unwrap().plain_text(), "d");
    match &runtime.payload_window.get(2).unwrap().payload {
        BlockPayload::RichText { spans } => {
            assert_eq!(spans.len(), 1);
            assert_eq!(spans[0].text, "d");
            assert_eq!(spans[0].marks, vec![InlineMark::Bold]);
        }
        other => panic!("expected rich text payload, got {other:?}"),
    }
}

#[test]
fn command_enter_splits_but_forces_paragraph() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::NumberedList, 0, None, "abcde")]);
    runtime.focus_block_at_offset(1, 2).unwrap();

    let new_block_id = runtime.insert_paragraph_after_focused().unwrap();

    assert_eq!(new_block_id, 2);
    assert_eq!(runtime.kind_at_index(0), RichBlockKind::NumberedList);
    assert_eq!(runtime.kind_at_index(1), RichBlockKind::Paragraph);
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "ab");
    assert_eq!(runtime.payload_window.get(2).unwrap().plain_text(), "cde");
    assert_eq!(runtime.focused_block_id(), Some(2));
}

#[test]
fn indent_focused_block_requires_previous_block_that_supports_children() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    runtime.focus_block(2);
    let before_version = runtime.index.structure_version;

    assert!(runtime.indent_focused_block().unwrap());

    assert_eq!(runtime.index.structure_version, before_version + 1);
    assert_eq!(runtime.index.parent_ids[1], Some(1));
    assert_eq!(runtime.index.depths[1], 1);
    let projection = runtime.projection();
    assert!(projection.blocks[0].chrome.has_children);
    assert_eq!(projection.blocks[1].chrome.list_info.depth, 1);

    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::Paragraph, 0, None),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    runtime.focus_block(2);
    assert!(!runtime.indent_focused_block().unwrap());
    assert_eq!(runtime.index.parent_ids[1], None);
    assert_eq!(runtime.index.depths[1], 0);
}

fn runtime_with_single_payload(kind: RichBlockKind, payload: BlockPayload) -> DocumentRuntime {
    let record = BlockIndexRecord::new(1, None, 0, kind_tag_for_rich_block_kind(&kind), 0)
        .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(1, 32.0));
    let payload = BlockPayloadRecord {
        block_id: 1,
        content_version: 1,
        kind,
        payload,
    };
    DocumentRuntime::from_index_records(1, vec![record], vec![payload], 1, 720.0)
}

#[test]
fn code_tab_inserts_four_spaces_without_structure_change() {
    let mut runtime = runtime_with_single_payload(
        RichBlockKind::Code {
            language: Some("rust".to_owned()),
        },
        BlockPayload::Code {
            language: Some("rust".to_owned()),
            text: "fn main()".to_owned(),
        },
    );
    runtime.focus_block_at_offset(1, 2).unwrap();
    let before_structure_version = runtime.index.structure_version;

    assert!(runtime.indent_focused_block().unwrap());

    assert_eq!(runtime.focused_text().unwrap(), "fn     main()");
    assert_eq!(runtime.caret_offset_for_block(1), Some(6));
    assert_eq!(runtime.index.structure_version, before_structure_version);
    assert_eq!(runtime.pending_structure_transaction_count(), 0);
    match &runtime.payload_window.get(1).unwrap().payload {
        BlockPayload::Code { text, .. } => assert_eq!(text, "fn     main()"),
        other => panic!("expected code payload, got {other:?}"),
    }
}

#[test]
fn code_shift_tab_removes_line_indent_without_structure_change() {
    let mut runtime = runtime_with_single_payload(
        RichBlockKind::Code {
            language: Some("rust".to_owned()),
        },
        BlockPayload::Code {
            language: Some("rust".to_owned()),
            text: "fn main() {\n    value\n}".to_owned(),
        },
    );
    runtime
        .focus_block_at_offset(1, "fn main() {\n    va".len())
        .unwrap();
    let before_structure_version = runtime.index.structure_version;

    assert!(runtime.outdent_focused_block().unwrap());

    assert_eq!(runtime.focused_text().unwrap(), "fn main() {\nvalue\n}");
    assert_eq!(
        runtime.caret_offset_for_block(1),
        Some("fn main() {\nva".len())
    );
    assert_eq!(runtime.index.structure_version, before_structure_version);
    assert_eq!(runtime.pending_structure_transaction_count(), 0);
}

#[test]
fn raw_markdown_tab_and_shift_tab_are_payload_only() {
    let mut runtime = runtime_with_single_payload(
        RichBlockKind::RawMarkdown,
        BlockPayload::RichText {
            spans: vec![InlineSpan::plain("alpha")],
        },
    );
    runtime.focus_block_at_offset(1, 0).unwrap();
    let before_structure_version = runtime.index.structure_version;

    assert!(runtime.indent_focused_block().unwrap());
    assert_eq!(runtime.focused_text().unwrap(), "    alpha");
    assert!(runtime.outdent_focused_block().unwrap());
    assert_eq!(runtime.focused_text().unwrap(), "alpha");
    assert_eq!(runtime.index.structure_version, before_structure_version);
    assert_eq!(runtime.pending_structure_transaction_count(), 0);
}

#[test]
fn tab_indents_block_under_previous_sibling_children_tail() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 1, Some(1)),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    runtime.focus_block(3);

    assert!(runtime.indent_focused_block().unwrap());

    assert_eq!(runtime.index.block_ids, vec![1, 2, 3]);
    assert_eq!(runtime.index.parent_ids[2], Some(1));
    assert_eq!(runtime.index.depths[2], 1);
    assert_eq!(runtime.direct_child_position(Some(1), 3), Some(1));
    assert_eq!(runtime.focused_block_id(), Some(3));
    assert_eq!(runtime.pending_structure_transaction_count(), 1);
}

#[test]
fn indent_and_outdent_preserve_caret_and_input_session_selection() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::BulletedList, 0, None, "parent"),
        (RichBlockKind::BulletedList, 0, None, "abcdef"),
    ]);
    runtime.focus_block_at_offset(2, 2).unwrap();

    assert!(runtime.indent_focused_block().unwrap());

    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(runtime.caret_offset_for_block(2), Some(2));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::BlockText { block_id: 2 })
    );
    assert_eq!(runtime.input_session_selected_range(), Some(2..2));

    assert!(runtime.outdent_focused_block().unwrap());

    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(runtime.caret_offset_for_block(2), Some(2));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::BlockText { block_id: 2 })
    );
    assert_eq!(runtime.input_session_selected_range(), Some(2..2));
}

#[test]
fn tab_first_sibling_does_nothing() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    runtime.focus_block(1);
    let before_version = runtime.index.structure_version;

    assert!(!runtime.indent_focused_block().unwrap());

    assert_eq!(runtime.index.structure_version, before_version);
    assert_eq!(runtime.index.parent_ids, vec![None, None]);
    assert_eq!(runtime.pending_structure_transaction_count(), 0);
}

#[test]
fn tab_previous_non_container_does_nothing() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::Paragraph, 0, None),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    runtime.focus_block(2);
    let before_version = runtime.index.structure_version;

    assert!(!runtime.indent_focused_block().unwrap());

    assert_eq!(runtime.index.structure_version, before_version);
    assert_eq!(runtime.index.parent_ids[1], None);
    assert_eq!(runtime.pending_structure_transaction_count(), 0);
}

#[test]
fn outdent_focused_block_moves_subtree_up_one_level() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 1, Some(1)),
        (RichBlockKind::Todo { checked: false }, 2, Some(2)),
    ]);
    runtime.focus_block(2);
    let before_version = runtime.index.structure_version;

    assert!(runtime.outdent_focused_block().unwrap());

    assert_eq!(runtime.index.structure_version, before_version + 1);
    assert_eq!(runtime.index.parent_ids[1], None);
    assert_eq!(runtime.index.depths[1], 0);
    assert_eq!(runtime.index.parent_ids[2], Some(2));
    assert_eq!(runtime.index.depths[2], 1);
    let projection = runtime.projection();
    assert_eq!(projection.blocks[1].chrome.list_info.depth, 0);
    assert_eq!(projection.blocks[2].chrome.list_info.depth, 1);
}

#[test]
fn shift_tab_outdents_block_after_parent_subtree() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 1, Some(1)),
        (RichBlockKind::Todo { checked: false }, 2, Some(2)),
        (RichBlockKind::BulletedList, 1, Some(1)),
    ]);
    runtime.focus_block(2);

    assert!(runtime.outdent_focused_block().unwrap());

    assert_eq!(runtime.index.block_ids, vec![1, 4, 2, 3]);
    assert_eq!(runtime.index.parent_ids[2], None);
    assert_eq!(runtime.index.depths[2], 0);
    assert_eq!(runtime.index.parent_ids[3], Some(2));
    assert_eq!(runtime.index.depths[3], 1);
    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(runtime.pending_structure_transaction_count(), 1);
}

#[test]
fn shift_tab_root_block_does_nothing() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    runtime.focus_block(2);
    let before_version = runtime.index.structure_version;

    assert!(!runtime.outdent_focused_block().unwrap());

    assert_eq!(runtime.index.structure_version, before_version);
    assert_eq!(runtime.index.parent_ids, vec![None, None]);
    assert_eq!(runtime.pending_structure_transaction_count(), 0);
}

#[test]
fn indent_outdent_preserve_subtree_children_and_queue_transactions() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(2)),
    ]);
    runtime.focus_block(2);

    assert!(runtime.indent_focused_block().unwrap());
    assert_eq!(runtime.index.parent_ids[1], Some(1));
    assert_eq!(runtime.index.depths[1], 1);
    assert_eq!(runtime.index.parent_ids[2], Some(2));
    assert_eq!(runtime.index.depths[2], 2);
    assert_eq!(runtime.pending_structure_transaction_count(), 1);

    assert!(runtime.outdent_focused_block().unwrap());
    assert_eq!(runtime.index.parent_ids[1], None);
    assert_eq!(runtime.index.depths[1], 0);
    assert_eq!(runtime.index.parent_ids[2], Some(2));
    assert_eq!(runtime.index.depths[2], 1);
    assert_eq!(runtime.pending_structure_transaction_count(), 2);
}

#[test]
fn numbered_ordinal_recomputes_after_enter_indent_outdent() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::NumberedList, 0, None, "one"),
        (RichBlockKind::NumberedList, 0, None, "two"),
    ]);
    runtime.focus_block_at_offset(1, 3).unwrap();
    runtime.handle_enter().unwrap();
    assert_eq!(runtime.index.block_ids, vec![1, 3, 2]);

    let projection = runtime.projection_for_window_planned();
    assert_eq!(
        projection.blocks[0].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 1 }
    );
    assert_eq!(
        projection.blocks[1].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 2 }
    );
    assert_eq!(
        projection.blocks[2].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 3 }
    );

    runtime.focus_block(2);
    assert!(runtime.indent_focused_block().unwrap());
    let projection = runtime.projection_for_window_planned();
    assert_eq!(
        projection.blocks[0].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 1 }
    );
    assert_eq!(
        projection.blocks[1].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 2 }
    );
    assert_eq!(
        projection.blocks[2].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 1 }
    );

    assert!(runtime.outdent_focused_block().unwrap());
    let projection = runtime.projection_for_window_planned();
    assert_eq!(
        projection.blocks[0].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 1 }
    );
    assert_eq!(
        projection.blocks[1].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 2 }
    );
    assert_eq!(
        projection.blocks[2].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 3 }
    );
}

#[test]
fn indent_outdent_undo_redo_restore_tree() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(2)),
    ]);
    runtime.focus_block(2);

    assert!(runtime.indent_focused_block().unwrap());
    assert_eq!(runtime.index.parent_ids[1], Some(1));
    assert_eq!(runtime.index.depths[2], 2);

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.index.parent_ids[1], None);
    assert_eq!(runtime.index.depths[1], 0);
    assert_eq!(runtime.index.parent_ids[2], Some(2));
    assert_eq!(runtime.index.depths[2], 1);

    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(runtime.index.parent_ids[1], Some(1));
    assert_eq!(runtime.index.depths[1], 1);
    assert_eq!(runtime.index.parent_ids[2], Some(2));
    assert_eq!(runtime.index.depths[2], 2);
}

#[test]
fn move_block_subtree_before_moves_children_and_preserves_total_height() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(1)),
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::NumberedList, 0, None),
    ]);
    let total_height = runtime.height_index.total_height();
    let before_version = runtime.index.structure_version;

    assert!(runtime.move_block_subtree_before(1, Some(4)).unwrap());

    assert_eq!(runtime.index.structure_version, before_version + 1);
    assert_eq!(runtime.index.block_ids, vec![3, 1, 2, 4]);
    assert_eq!(runtime.index.parent_ids[1], None);
    assert_eq!(runtime.index.parent_ids[2], Some(1));
    assert_eq!(runtime.index.depths[1], 0);
    assert_eq!(runtime.index.depths[2], 1);
    assert_eq!(runtime.height_index.total_height(), total_height);
    let projection = runtime.projection();
    assert_eq!(
        projection.blocks[0].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 1 }
    );
    assert_eq!(
        projection.blocks[1].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 2 }
    );
    assert_eq!(
        projection.blocks[3].chrome.prefix,
        BlockPrefixSnapshot::Number { ordinal: 3 }
    );
}

#[test]
fn move_block_subtree_commit_preserves_scroll_top_and_total_height() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(1)),
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::BulletedList, 0, None),
    ]);
    runtime
        .scroll
        .scroll_to_global_offset(96.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let before_scroll_top = runtime.scroll.global_scroll_top;
    let before_total_height = runtime.height_index.total_height();

    assert!(runtime.move_block_subtree_before(1, Some(4)).unwrap());

    assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
    assert_eq!(runtime.height_index.total_height(), before_total_height);
    assert_eq!(
        runtime.scroll.model_total_height,
        runtime.scroll_extent_height(before_total_height)
    );
    assert_eq!(
        runtime.scroll.displayed_total_height,
        runtime.scroll_extent_height(before_total_height)
    );
}

#[test]
fn move_block_subtree_to_parent_reparents_and_updates_depth_delta() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::Paragraph, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(2)),
    ]);
    let total_height = runtime.height_index.total_height();

    assert!(runtime.move_block_subtree_to_parent(2, Some(1), 0).unwrap());

    assert_eq!(runtime.index.block_ids, vec![1, 2, 3]);
    assert_eq!(runtime.index.parent_ids[1], Some(1));
    assert_eq!(runtime.index.depths[1], 1);
    assert_eq!(runtime.index.parent_ids[2], Some(2));
    assert_eq!(runtime.index.depths[2], 2);
    assert_eq!(runtime.height_index.total_height(), total_height);
    let projection = runtime.projection();
    assert!(projection.blocks[0].chrome.has_children);
    assert_eq!(projection.blocks[1].chrome.list_info.depth, 1);
    assert_eq!(projection.blocks[2].chrome.list_info.depth, 2);
}

#[test]
fn undo_and_redo_restore_structure_move_without_full_snapshot() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(1)),
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::NumberedList, 0, None),
    ]);

    assert!(runtime.move_block_subtree_before(1, Some(4)).unwrap());
    assert_eq!(runtime.index.block_ids, vec![3, 1, 2, 4]);

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.index.block_ids, vec![1, 2, 3, 4]);
    assert_eq!(runtime.index.parent_ids[1], Some(1));
    assert_eq!(runtime.index.depths[1], 1);

    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(runtime.index.block_ids, vec![3, 1, 2, 4]);
    assert_eq!(runtime.index.parent_ids[2], Some(1));
    assert_eq!(runtime.index.depths[2], 1);
}

#[test]
fn structure_move_queues_persistable_transactions_for_move_undo_and_redo() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(1)),
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::NumberedList, 0, None),
    ]);

    assert!(runtime.move_block_subtree_before(1, Some(4)).unwrap());
    assert_eq!(runtime.pending_structure_transaction_count(), 1);
    let txs = runtime.drain_pending_structure_transactions();
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].kind, EditTransactionKind::BlockStructureChange);
    assert_eq!(
        txs[0].ops,
        vec![EditOperation::MoveBlockToParent {
            block_id: 1,
            parent_id: None,
            sibling_index: 1,
        }]
    );
    assert_eq!(
        txs[0].inverse_ops,
        vec![EditOperation::MoveBlockToParent {
            block_id: 1,
            parent_id: None,
            sibling_index: 0,
        }]
    );

    assert!(runtime.undo_focused_block().unwrap());
    let undo_txs = runtime.drain_pending_structure_transactions();
    assert_eq!(
        undo_txs[0].ops,
        vec![EditOperation::MoveBlockToParent {
            block_id: 1,
            parent_id: None,
            sibling_index: 0,
        }]
    );

    assert!(runtime.redo_focused_block().unwrap());
    let redo_txs = runtime.drain_pending_structure_transactions();
    assert_eq!(
        redo_txs[0].ops,
        vec![EditOperation::MoveBlockToParent {
            block_id: 1,
            parent_id: None,
            sibling_index: 1,
        }]
    );
}

#[test]
fn undo_order_prefers_newer_text_edit_over_older_structure_move() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::NumberedList, 0, None),
        (RichBlockKind::Todo { checked: false }, 1, Some(1)),
        (RichBlockKind::Paragraph, 0, None),
        (RichBlockKind::NumberedList, 0, None),
    ]);

    assert!(runtime.move_block_subtree_before(1, Some(4)).unwrap());
    runtime.focus_block_at_offset(3, 0).unwrap();
    runtime.insert_char('x').unwrap();
    assert_eq!(runtime.focused_text(), Some("xitem"));

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.focused_text(), Some("item"));
    assert_eq!(runtime.index.block_ids, vec![3, 1, 2, 4]);

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.index.block_ids, vec![1, 2, 3, 4]);
}

#[test]
fn move_block_subtree_to_parent_rejects_invalid_parent() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::Paragraph, 0, None),
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 1, Some(2)),
    ]);

    assert!(!runtime.move_block_subtree_to_parent(2, Some(1), 0).unwrap());
    assert!(!runtime.move_block_subtree_to_parent(2, Some(3), 0).unwrap());
    assert_eq!(runtime.index.block_ids, vec![1, 2, 3]);
}

#[test]
fn move_block_subtree_before_rejects_target_inside_source_subtree() {
    let mut runtime = runtime_with_kind_depths(vec![
        (RichBlockKind::BulletedList, 0, None),
        (RichBlockKind::BulletedList, 1, Some(1)),
        (RichBlockKind::BulletedList, 0, None),
    ]);

    assert!(!runtime.move_block_subtree_before(1, Some(2)).unwrap());
    assert_eq!(runtime.index.block_ids, vec![1, 2, 3]);
}

#[test]
fn enter_on_empty_root_list_turns_it_into_paragraph() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![(
        RichBlockKind::Todo { checked: false },
        0,
        None,
        "",
    )]);
    runtime.focus_block(1);

    runtime.handle_enter().unwrap();

    assert!(matches!(
        runtime.payload_window.get(1).map(|record| &record.kind),
        Some(RichBlockKind::Paragraph)
    ));
    let projection = runtime.projection();
    assert!(matches!(
        projection.blocks[0].kind,
        RichBlockKind::Paragraph
    ));
    assert_eq!(
        projection.blocks[0].chrome.prefix,
        BlockPrefixSnapshot::None
    );
}

#[test]
fn enter_on_empty_nested_list_outdents_it() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::BulletedList, 0, None, "parent"),
        (RichBlockKind::BulletedList, 1, Some(1), ""),
    ]);
    runtime.focus_block(2);

    runtime.handle_enter().unwrap();

    assert!(matches!(
        runtime.payload_window.get(2).map(|record| &record.kind),
        Some(RichBlockKind::BulletedList)
    ));
    assert_eq!(runtime.index.parent_ids[1], None);
    assert_eq!(runtime.index.depths[1], 0);
    let projection = runtime.projection();
    assert_eq!(projection.blocks[1].chrome.list_info.depth, 0);
}

#[test]
fn enter_on_empty_root_todo_turns_paragraph_and_clears_checkbox() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![(
        RichBlockKind::Todo { checked: true },
        0,
        None,
        "",
    )]);
    runtime.focus_block(1);

    runtime.handle_enter().unwrap();

    assert!(matches!(
        runtime.payload_window.get(1).map(|record| &record.kind),
        Some(RichBlockKind::Paragraph)
    ));
    let projection = runtime.projection();
    assert_eq!(projection.blocks.len(), 1);
    assert_eq!(
        projection.blocks[0].chrome.prefix,
        BlockPrefixSnapshot::None
    );
}

#[test]
fn enter_on_empty_nested_todo_outdents_and_preserves_todo_kind() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Todo { checked: false }, 0, None, "parent"),
        (RichBlockKind::Todo { checked: true }, 1, Some(1), ""),
    ]);
    runtime.focus_block(2);

    runtime.handle_enter().unwrap();

    assert!(matches!(
        runtime.payload_window.get(2).map(|record| &record.kind),
        Some(RichBlockKind::Todo { checked: true })
    ));
    assert_eq!(runtime.index.parent_ids[1], None);
    assert_eq!(runtime.index.depths[1], 0);
    let projection = runtime.projection();
    assert_eq!(projection.blocks[1].chrome.list_info.depth, 0);
    assert_eq!(
        projection.blocks[1].chrome.prefix,
        BlockPrefixSnapshot::Todo { checked: true }
    );
}

#[test]
fn enter_on_whitespace_only_list_item_uses_trim_empty_check() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::NumberedList, 0, None, "  \n\t  ")]);
    runtime.focus_block(1);

    runtime.handle_enter().unwrap();

    assert_eq!(runtime.index.total_count(), 1);
    assert!(matches!(
        runtime.payload_window.get(1).map(|record| &record.kind),
        Some(RichBlockKind::Paragraph)
    ));
}

#[test]
fn enter_on_empty_list_does_not_create_block_or_move_scroll_top() {
    let mut blocks = Vec::new();
    for index in 0..50 {
        let kind = if index == 20 {
            RichBlockKind::BulletedList
        } else {
            RichBlockKind::Paragraph
        };
        let text = if index == 20 { "" } else { "item" };
        blocks.push((kind, 0, None, text));
    }
    let mut runtime = runtime_with_kind_depths_and_text(blocks);
    runtime
        .scroll
        .scroll_to_global_offset(320.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    runtime.focus_block(21);
    let before_scroll_top = runtime.scroll.global_scroll_top;
    let before_count = runtime.index.total_count();

    runtime.handle_enter().unwrap();

    assert_eq!(runtime.index.total_count(), before_count);
    assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
    assert!(matches!(
        runtime.payload_window.get(21).map(|record| &record.kind),
        Some(RichBlockKind::Paragraph)
    ));
}

#[test]
fn toggle_todo_checked_updates_payload_kind_and_projection_prefix() {
    let mut document = RichTextDocument::empty(1);
    document.push_root_block(RichBlockRecord::todo(1, false, "ship it"));
    let mut runtime = DocumentRuntime::from_rich_text_document(document, 720.0);

    assert!(runtime.toggle_todo_checked(1).unwrap());

    assert!(matches!(
        runtime.payload_window.get(1).map(|record| &record.kind),
        Some(RichBlockKind::Todo { checked: true })
    ));
    let projection = runtime.projection_for_window();
    assert!(matches!(
        projection.blocks[0].kind,
        RichBlockKind::Todo { checked: true }
    ));
    assert_eq!(
        projection.blocks[0].chrome.prefix,
        BlockPrefixSnapshot::Todo { checked: true }
    );
}

#[test]
fn runtime_with_100k_blocks_fixture_builds_without_large_strings() {
    let runtime = runtime_with_paragraph_blocks(100_000);

    assert_eq!(runtime.index.total_count(), 100_000);
    assert_eq!(runtime.visible_index.total_visible_count(), 100_000);
    assert_eq!(runtime.payload_window.payloads.len(), 100_000);
    assert_eq!(runtime.height_index.total_height(), 3_200_000.0);
    assert!(runtime.page_layout.page_count() >= 100);
}

#[test]
fn large_mixed_demo_keeps_payloads_windowed() {
    let mut runtime = DocumentRuntime::large_mixed_demo();

    assert_eq!(
        runtime.index.total_count(),
        cditor_core::demo_fixtures::LARGE_MIXED_DEMO_BLOCKS
    );
    assert!(runtime.payload_window.payloads.len() < 2_000);
    assert!(runtime.payload_window.block_range.start == 0);

    runtime
        .scroll
        .scroll_to_global_offset(1_000_000.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let projection = runtime.projection_for_window_planned();

    assert!(!projection.blocks.is_empty());
    assert!(projection.blocks.len() <= 320);
    assert!(runtime.payload_window.payloads.len() < 5_000);
    assert!(runtime.payload_window.block_range.start > 0);
}

#[test]
fn target_for_global_offset_maps_100k_document_precisely() {
    let runtime = runtime_with_paragraph_blocks(100_000);
    let samples = [0.0, 1.0, 31.9, 32.0, 50_000.0, 3_199_999.0];

    for global_y in samples {
        let target = runtime.target_for_global_offset(global_y).unwrap();
        assert_eq!(
            target.block_index,
            (target.global_scroll_top / 32.0).floor().min(99_999.0) as usize
        );
        assert_eq!(target.block_id, target.block_index as BlockId + 1);
        assert!(target.block_top <= target.global_scroll_top + f64::EPSILON);
        assert!(target.offset_in_block >= 0.0);
        assert!(target.offset_in_block <= 32.0);
        assert_eq!(
            runtime.page_layout.page_for_block_index(target.block_index),
            Some(target.page_index)
        );
    }
}

#[test]
fn planned_window_hysteresis_keeps_boundary_window_stable() {
    let mut runtime = runtime_with_paragraph_blocks(3_000);
    runtime.window_planner = WindowPlanner::new(
        0,
        0,
        WindowPlannerPolicy {
            enter_threshold_viewports: 0.5,
            min_stable_frames_before_trim: 0,
            min_ms_between_window_commits: 0,
            ..WindowPlannerPolicy::default()
        },
    );
    let first_page_height = runtime.page_layout.pages[0].height;
    runtime
        .scroll
        .scroll_to_global_offset(
            first_page_height - 10.0,
            cditor_editor::scroll::ScrollOrigin::UserWheel,
        )
        .unwrap();
    let initial = runtime.current_page_window_planned();
    runtime
        .scroll
        .scroll_to_global_offset(
            first_page_height + 10.0,
            cditor_editor::scroll::ScrollOrigin::UserWheel,
        )
        .unwrap();
    let near_boundary = runtime.current_page_window_planned();

    assert_eq!(near_boundary, initial);
}

#[test]
fn planned_window_keeps_focused_page_pinned() {
    let mut runtime = runtime_with_paragraph_blocks(10_000);
    runtime.window_planner = WindowPlanner::new(0, 0, WindowPlannerPolicy::default());
    runtime.focus_block(1);
    let target_page = runtime.page_layout.page_count() - 1;
    let offset = runtime.page_layout.offset_of_page(target_page).unwrap();
    runtime
        .scroll
        .scroll_to_global_offset(offset, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();

    let planned = runtime.current_page_window_planned();
    let focused_page = runtime.page_layout.page_for_block_index(0).unwrap();
    assert!(planned.contains(&focused_page));
    assert!(planned.contains(&target_page));
}

#[test]
fn document_runtime_projects_v2_blocks_without_ui_truth() {
    let runtime = DocumentRuntime::demo();
    let projection = runtime.projection();
    assert_eq!(projection.total_visible_blocks, 4);
    assert_eq!(projection.blocks.len(), 4);
    assert_eq!(projection.blocks[0].block_id, 1);
    assert!(matches!(
        projection.blocks[0].kind,
        RichBlockKind::Heading { level: 1 }
    ));
}

#[test]
fn projection_for_window_exposes_total_visible_count_and_spacers() {
    let runtime = DocumentRuntime::demo();

    let projection = runtime.projection_for_window();

    assert_eq!(
        projection.total_visible_blocks,
        runtime.visible_index.total_visible_count()
    );
    assert_eq!(projection.before_window_height, 0.0);
    assert_eq!(projection.placeholder_window_height, None);
    assert_eq!(
        projection.after_window_height,
        projection.down_placer_height
    );
}

#[test]
fn scrollbar_drag_uses_runtime_frozen_projection_instead_of_placeholder() {
    let records = (1..=1_000 as BlockId)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
            .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(block_id, 32.0))
        })
        .collect::<Vec<_>>();
    let payloads = (1..=1_000 as BlockId)
        .map(|block_id| BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, ""))
        .collect::<Vec<_>>();
    let mut runtime = DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0);
    let loaded = runtime.projection_for_window_planned();
    assert!(!loaded.render_window.is_placeholder());
    runtime.payload_window.block_range = 0..64;
    runtime
        .payload_window
        .payloads
        .retain(|block_id, _| *block_id <= 64);

    runtime
        .scroll
        .scroll_to_global_offset(20_000.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let policy = ScrollbarPolicy::default();
    runtime.begin_scrollbar_drag(policy);

    let frozen = runtime.projection_for_window_planned();

    assert!(!frozen.render_window.is_placeholder());
    assert_eq!(frozen.placeholder_window_height, None);
    assert!(!frozen.blocks.is_empty());
    assert_eq!(frozen.blocks[0].block_id, loaded.blocks[0].block_id);
    assert_eq!(
        frozen.render_window.block_range,
        loaded.render_window.block_range
    );
}

#[test]
fn projection_uses_placeholder_window_when_payload_window_is_not_loaded() {
    let records = (1..=1_000 as BlockId)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
            .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(block_id, 32.0))
        })
        .collect::<Vec<_>>();
    let runtime =
        DocumentRuntime::from_index_records_with_window(1, records, Vec::new(), 1, 720.0, 0..0);

    let projection = runtime.projection_for_window();

    assert!(projection.render_window.is_placeholder());
    assert!(projection.blocks.is_empty());
    assert_eq!(
        projection.placeholder_window_height,
        Some(projection.render_window.height())
    );
    assert_eq!(
        projection.before_window_height
            + projection.placeholder_window_height.unwrap_or_default()
            + projection.after_window_height,
        runtime.scroll_extent_height(runtime.page_layout.total_height())
    );
}

#[test]
fn focus_block_at_offset_sets_caret_without_ui_truth() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );

    runtime.focus_block_at_offset(1, 2).unwrap();

    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.caret_offset_for_block(1), Some(2));
    let projection = runtime.projection_for_window();
    assert_eq!(projection.blocks[0].caret_offset, Some(2));
    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(editing.input_target, InputTarget::BlockText { block_id: 1 });
    assert_eq!(editing.selected_range, 2..2);
    assert_eq!(editing.marked_range, None);
}

#[test]
fn document_text_selection_updates_input_session_selection_truth() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 1).unwrap();

    runtime.set_document_text_selection(1, 5, 1, 2).unwrap();

    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(editing.input_target, InputTarget::BlockText { block_id: 1 });
    assert_eq!(editing.selected_range, 2..5);
    assert!(editing.selection_reversed);
    assert_eq!(runtime.input_session_selected_range(), Some(2..5));
}

#[test]
fn caret_movement_updates_input_session_selection_truth() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 3).unwrap();

    runtime.move_caret_right(true).unwrap();

    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(editing.selected_range, 3..4);
    assert!(!editing.selection_reversed);

    runtime.move_caret_left(false).unwrap();

    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(editing.selected_range, 3..3);
    assert!(!editing.selection_reversed);
}

#[test]
fn stale_input_session_content_version_is_rejected() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();
    runtime
        .begin_or_update_composition_with_selection(1, 2..2, "你", Some("你".len().."你".len()))
        .unwrap();
    runtime
        .payload_window
        .payloads
        .get_mut(&1)
        .unwrap()
        .content_version += 1;

    assert_eq!(runtime.input_session_target(), None);
    assert_eq!(runtime.input_session_selected_range(), None);
    assert_eq!(runtime.input_session_marked_range(), None);
    assert_eq!(runtime.active_composition(), None);
    assert_eq!(runtime.focused_text_for_platform_input(), None);
}

#[test]
fn insert_char_uses_caret_offset() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    runtime.set_caret_offset(1, 2).unwrap();

    runtime.insert_char('X').unwrap();

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "abXcd");
    assert_eq!(projection.blocks[0].caret_offset, Some(3));
}

#[test]
fn composition_preview_does_not_commit_until_commit() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "ab",
        )],
        720.0,
    );
    runtime.begin_or_update_composition(1, 1..1, "中").unwrap();

    assert!(runtime.undo_events.is_empty());
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "ab");
    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "a中b");
    assert_eq!(projection.blocks[0].marked_range, Some(1.."a中".len()));
    assert_eq!(projection.blocks[0].caret_offset, Some("a中".len()));
    assert_eq!(runtime.composition_preview_text().as_deref(), Some("a中b"));
    assert_eq!(runtime.focused_text_for_platform_input().unwrap().1, "a中b");
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(1.."a中".len())
    );
    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(editing.input_target, InputTarget::BlockText { block_id: 1 });
    assert_eq!(editing.selected_range, "a中".len().."a中".len());
    assert_eq!(editing.marked_range, Some(1.."a中".len()));
    assert_eq!(
        runtime
            .editing
            .as_ref()
            .unwrap()
            .composition
            .as_ref()
            .unwrap()
            .preview_text,
        "中"
    );

    assert!(runtime.commit_composition().unwrap());
    assert_eq!(runtime.undo_events.len(), 1);
    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "a中b");
    assert!(runtime.editing.as_ref().unwrap().composition.is_none());
    assert_eq!(
        runtime.input_session_selected_range(),
        Some("a中".len().."a中".len())
    );
    assert_eq!(runtime.input_session_marked_range(), None);
}

#[test]
fn composition_commit_undo_redo_restores_block_text() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "ab",
        )],
        720.0,
    );

    runtime.begin_or_update_composition(1, 1..1, "中").unwrap();

    assert!(runtime.undo_events.is_empty());
    assert!(runtime.commit_composition().unwrap());
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "a中b");

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "ab");

    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "a中b");
}

#[test]
fn multistage_ime_commit_creates_one_undo_step() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "ab",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 1).unwrap();

    runtime.begin_or_update_composition(1, 1..1, "n").unwrap();
    runtime.begin_or_update_composition(1, 1..1, "ni").unwrap();
    runtime.begin_or_update_composition(1, 1..1, "你").unwrap();

    assert!(runtime.undo_events.is_empty());
    assert!(runtime.commit_composition().unwrap());
    assert_eq!(runtime.undo_events.len(), 1);
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "a你b");

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "ab");
    assert!(!runtime.undo_focused_block().unwrap());
}

#[test]
fn replace_text_in_focused_range_commits_text_and_clears_composition() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();
    runtime.begin_or_update_composition(1, 1..3, "中").unwrap();

    assert!(runtime.replace_text_in_focused_range(None, "字").unwrap());

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "a字d");
    assert!(runtime.active_composition().is_none());
}

#[test]
fn table_cell_composition_preview_and_commit_stay_inside_cell() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                rows: vec![cditor_core::rich_text::TableRowPayload {
                    cells: vec![cditor_core::rich_text::TableCellPayload::plain("ab")],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        }],
        720.0,
    );
    runtime.focus_table_cell_at_offset(1, 0, 0, 1).unwrap();

    runtime.begin_or_update_composition(1, 1..1, "中").unwrap();

    assert!(runtime.undo_events.is_empty());
    assert_eq!(runtime.focused_text_for_platform_input().unwrap().1, "a中b");
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(1.."a中".len())
    );
    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    let BlockPayload::Table(table) = &payload.payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a中b"));
    assert_eq!(runtime.payload_window.get(1).unwrap().plain_text(), "ab");
    assert_eq!(
        projection.blocks[0].focused_table_cell_offset,
        Some("a中".len())
    );
    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(
        editing.input_target,
        InputTarget::TableCell {
            block_id: 1,
            row: 0,
            col: 0
        }
    );
    assert_eq!(editing.selected_range, "a中".len().."a中".len());
    assert_eq!(editing.marked_range, Some(1.."a中".len()));

    assert!(runtime.commit_composition().unwrap());
    assert_eq!(runtime.undo_events.len(), 1);
    let BlockPayload::Table(table) = &runtime.payload_window.get(1).unwrap().payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a中b"));
    assert!(runtime.active_composition().is_none());
    assert_eq!(
        runtime.input_session_selected_range(),
        Some("a中".len().."a中".len())
    );
    assert_eq!(runtime.input_session_marked_range(), None);
}

#[test]
fn table_cell_composition_commit_undo_redo_restores_cell_text() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                rows: vec![cditor_core::rich_text::TableRowPayload {
                    cells: vec![cditor_core::rich_text::TableCellPayload::plain("ab")],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        }],
        720.0,
    );
    runtime.focus_table_cell_at_offset(1, 0, 0, 1).unwrap();

    runtime.begin_or_update_composition(1, 1..1, "中").unwrap();

    assert!(runtime.undo_events.is_empty());
    assert!(runtime.commit_composition().unwrap());
    let BlockPayload::Table(table) = &runtime.payload_window.get(1).unwrap().payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a中b"));

    assert!(runtime.undo_focused_block().unwrap());
    let BlockPayload::Table(table) = &runtime.payload_window.get(1).unwrap().payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("ab"));

    assert!(runtime.redo_focused_block().unwrap());
    let BlockPayload::Table(table) = &runtime.payload_window.get(1).unwrap().payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a中b"));
}

#[test]
fn table_cell_emoji_composition_preview_and_commit_stay_inside_cell() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                rows: vec![cditor_core::rich_text::TableRowPayload {
                    cells: vec![cditor_core::rich_text::TableCellPayload::plain("ab")],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        }],
        720.0,
    );
    runtime.focus_table_cell_at_offset(1, 0, 0, 1).unwrap();

    runtime.begin_or_update_composition(1, 1..1, "😀").unwrap();

    assert_eq!(runtime.focused_text_for_platform_input().unwrap().1, "a😀b");
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(1.."a😀".len())
    );

    assert!(runtime.commit_composition().unwrap());
    let BlockPayload::Table(table) = &runtime.payload_window.get(1).unwrap().payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a😀b"));
    assert_eq!(
        runtime.input_session_selected_range(),
        Some("a😀".len().."a😀".len())
    );
}

#[test]
fn table_cell_cjk_and_emoji_composition_commit_preserves_utf8_boundaries() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                rows: vec![cditor_core::rich_text::TableRowPayload {
                    cells: vec![cditor_core::rich_text::TableCellPayload::plain("ab")],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        }],
        720.0,
    );
    runtime.focus_table_cell_at_offset(1, 0, 0, 1).unwrap();

    let preview = "中かな한글😀";
    runtime
        .begin_or_update_composition(1, 1..1, preview)
        .unwrap();

    assert_eq!(
        runtime.focused_text_for_platform_input().unwrap().1,
        format!("a{preview}b")
    );
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(1..format!("a{preview}").len())
    );
    assert!(runtime.commit_composition().unwrap());

    let BlockPayload::Table(table) = &runtime.payload_window.get(1).unwrap().payload else {
        panic!("payload should be table");
    };
    assert_eq!(
        table.cell_plain_text(0, 0).as_deref(),
        Some(format!("a{preview}b").as_str())
    );
    assert_eq!(
        runtime.focused_table_cell_offset(),
        Some((1, 0, 0, format!("a{preview}").len()))
    );
}

#[test]
fn platform_input_text_comes_from_input_session_target() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                rows: vec![cditor_core::rich_text::TableRowPayload {
                    cells: vec![
                        cditor_core::rich_text::TableCellPayload::plain("left"),
                        cditor_core::rich_text::TableCellPayload::plain("right"),
                    ],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        }],
        720.0,
    );

    runtime.focus_table_cell_at_offset(1, 0, 1, 2).unwrap();

    assert_eq!(
        runtime.focused_text_for_platform_input(),
        Some((1, "right".to_owned()))
    );
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::TableCell {
            block_id: 1,
            row: 0,
            col: 1
        })
    );
}

#[test]
fn table_cell_replace_text_prioritizes_active_composition() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload {
                rows: vec![cditor_core::rich_text::TableRowPayload {
                    cells: vec![cditor_core::rich_text::TableCellPayload::plain("abcd")],
                    height: Default::default(),
                }],
                columns: Vec::new(),
                header_rows: 0,
                header_cols: 0,
                header_style: Default::default(),
            }),
        }],
        720.0,
    );
    runtime.focus_table_cell_at_offset(1, 0, 0, 2).unwrap();
    runtime.begin_or_update_composition(1, 1..3, "中").unwrap();

    assert!(runtime.replace_text_in_focused_range(None, "文").unwrap());

    let BlockPayload::Table(table) = &runtime.payload_window.get(1).unwrap().payload else {
        panic!("payload should be table");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("a文d"));
    assert_eq!(
        runtime.focused_table_cell_offset(),
        Some((1, 0, 0, "a文".len()))
    );
    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(
        editing.input_target,
        InputTarget::TableCell {
            block_id: 1,
            row: 0,
            col: 0
        }
    );
    assert_eq!(editing.selected_range, "a文".len().."a文".len());
    assert!(runtime.active_composition().is_none());
}

#[test]
fn replace_text_space_path_applies_block_markdown_shortcut() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "#",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 1).unwrap();

    assert!(runtime.replace_text_in_focused_range(None, " ").unwrap());

    let projection = runtime.projection_for_window();
    assert!(matches!(
        projection.blocks[0].kind,
        RichBlockKind::Heading { level: 1 }
    ));
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "");
}

#[test]
fn bold_markdown_shortcut_creates_bold_not_italic() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "**abc**",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, "**abc**".len()).unwrap();

    assert!(runtime.apply_inline_markdown_shortcut(1).unwrap());

    let payload = runtime.payload_window.get(1).unwrap();
    let BlockPayload::RichText { spans } = &payload.payload else {
        panic!("expected rich text payload");
    };
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].text, "abc");
    assert_eq!(spans[0].marks, vec![InlineMark::Bold]);
    assert_eq!(runtime.caret_offset_for_block(1), Some("abc".len()));
    assert_eq!(
        runtime.input_session_selected_range(),
        Some("abc".len().."abc".len())
    );
}

#[test]
fn inserting_inside_bold_span_preserves_bold_mark() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Paragraph,
            payload: BlockPayload::RichText {
                spans: vec![InlineSpan {
                    text: "ab".to_owned(),
                    marks: vec![InlineMark::Bold],
                }],
            },
        }],
        720.0,
    );
    runtime.focus_block_at_offset(1, 1).unwrap();

    runtime.insert_char('X').unwrap();

    let payload = runtime.payload_window.get(1).unwrap();
    let BlockPayload::RichText { spans } = &payload.payload else {
        panic!("expected rich text payload");
    };
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].text, "aXb");
    assert_eq!(spans[0].marks, vec![InlineMark::Bold]);
}

#[test]
fn deleting_inside_bold_span_preserves_remaining_bold_mark() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Paragraph,
            payload: BlockPayload::RichText {
                spans: vec![InlineSpan {
                    text: "abc".to_owned(),
                    marks: vec![InlineMark::Bold],
                }],
            },
        }],
        720.0,
    );
    runtime.focus_block_at_offset(1, "abc".len()).unwrap();

    assert!(runtime.delete_backward().unwrap());

    let payload = runtime.payload_window.get(1).unwrap();
    let BlockPayload::RichText { spans } = &payload.payload else {
        panic!("expected rich text payload");
    };
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].text, "ab");
    assert_eq!(spans[0].marks, vec![InlineMark::Bold]);
}

#[test]
fn replace_text_path_applies_inline_markdown_shortcut() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "**bold**",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, "**bold**".len()).unwrap();

    assert!(runtime.replace_text_in_focused_range(None, "!").unwrap());

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    let BlockPayload::RichText { spans } = &payload.payload else {
        panic!("payload should be rich text");
    };
    assert_eq!(payload.plain_text(), "bold!");
    assert!(spans.iter().any(|span| {
        span.text == "bold"
            && span
                .marks
                .contains(&cditor_core::rich_text::InlineMark::Bold)
    }));
    assert_eq!(projection.blocks[0].caret_offset, Some("bold!".len()));
    assert_eq!(
        runtime.input_session_selected_range(),
        Some("bold!".len().."bold!".len())
    );
}

#[test]
fn move_focused_caret_to_offset_updates_caret_without_selection() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();

    assert!(runtime.move_focused_caret_to_offset(1, 5, false).unwrap());

    let projection = runtime.projection_for_window();
    assert_eq!(projection.blocks[0].caret_offset, Some(5));
    assert_eq!(runtime.focused_text_selection_range(), None);
}

#[test]
fn move_focused_caret_to_offset_extends_same_block_selection() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();

    assert!(runtime.move_focused_caret_to_offset(1, 5, true).unwrap());

    let projection = runtime.projection_for_window();
    assert_eq!(projection.blocks[0].caret_offset, Some(5));
    assert_eq!(runtime.focused_text_selection_range(), Some(2..5));
    assert_eq!(runtime.selected_focused_text().as_deref(), Some("cde"));
}

#[test]
fn insert_char_uses_middle_caret_offset() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();

    runtime.insert_char('X').unwrap();

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "abXcd");
    assert_eq!(projection.blocks[0].caret_offset, Some(3));
}

#[test]
fn replace_text_in_focused_range_can_insert_in_middle_without_selection() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();

    assert!(runtime.replace_text_in_focused_range(None, "中").unwrap());

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "ab中cd");
    assert_eq!(projection.blocks[0].caret_offset, Some("ab中".len()));
}

#[test]
fn replace_text_in_focused_range_inserts_string_at_middle_caret() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 3).unwrap();

    assert!(runtime.replace_text_in_focused_range(None, "XYZ").unwrap());

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "abcXYZdef");
    assert_eq!(projection.blocks[0].caret_offset, Some("abcXYZ".len()));
}

#[test]
fn replace_text_in_focused_range_replaces_selection_and_caret_after_inserted_text() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();
    runtime.set_document_text_selection(1, 2, 1, 4).unwrap();

    assert!(runtime.replace_text_in_focused_range(None, "XYZ").unwrap());

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "abXYZef");
    assert_eq!(projection.blocks[0].caret_offset, Some("abXYZ".len()));
    assert_eq!(runtime.focused_text_selection_range(), None);
}

#[test]
fn ime_preview_and_commit_can_start_in_middle_of_text() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();

    runtime.begin_or_update_composition(1, 2..2, "你").unwrap();
    assert_eq!(
        runtime.composition_preview_text().as_deref(),
        Some("ab你cd")
    );
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(2.."ab你".len())
    );
    assert!(runtime.commit_composition().unwrap());

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "ab你cd");
    assert_eq!(projection.blocks[0].caret_offset, Some("ab你".len()));
}

#[test]
fn replace_text_prioritizes_active_composition_over_stale_selection() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcdef",
        )],
        720.0,
    );
    runtime.set_document_text_selection(1, 1, 1, 5).unwrap();
    runtime
        .begin_or_update_composition_with_selection(1, 3..3, "中", None)
        .unwrap();

    assert!(runtime.replace_text_in_focused_range(None, "文").unwrap());

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "abc文def");
    assert_eq!(projection.blocks[0].caret_offset, Some("abc文".len()));
}

#[test]
fn ime_preview_tracks_selected_subrange_inside_marked_text() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();

    runtime
        .begin_or_update_composition_with_selection(1, 2..2, "你好", Some("你".len().."你好".len()))
        .unwrap();

    assert_eq!(
        runtime.composition_preview_text().as_deref(),
        Some("ab你好cd")
    );
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(2.."ab你好".len())
    );
    assert_eq!(
        runtime.active_composition_selected_range(),
        Some("ab你".len().."ab你好".len())
    );
    assert_eq!(runtime.caret_offset_for_block(1), Some("ab你好".len()));
}

#[test]
fn ime_composition_update_replaces_base_range_for_cjk_text() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "abcd",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 2).unwrap();

    runtime.begin_or_update_composition(1, 2..2, "に").unwrap();
    assert_eq!(
        runtime.composition_preview_text().as_deref(),
        Some("abにcd")
    );

    runtime
        .begin_or_update_composition(1, 2..2, "日本")
        .unwrap();
    assert_eq!(
        runtime.composition_preview_text().as_deref(),
        Some("ab日本cd")
    );

    runtime.begin_or_update_composition(1, 2..2, "한").unwrap();
    assert_eq!(
        runtime.composition_preview_text().as_deref(),
        Some("ab한cd")
    );
    assert_eq!(
        runtime.active_composition_marked_range(),
        Some(2.."ab한".len())
    );

    assert!(runtime.commit_composition().unwrap());
    assert_eq!(
        runtime.payload_window.get(1).unwrap().plain_text(),
        "ab한cd"
    );
}

#[test]
fn backspace_at_start_merges_non_empty_paragraph_into_previous() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "hello "),
        (RichBlockKind::Paragraph, 0, None, "world"),
    ]);
    runtime.focus_block_at_offset(2, 0).unwrap();
    let before_scroll_top = runtime.scroll.global_scroll_top;

    assert!(runtime.delete_backward().unwrap());

    assert_eq!(runtime.index.total_count(), 1);
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.focused_text(), Some("hello world"));
    assert_eq!(runtime.selected_focused_text(), Some("world".to_owned()));
    assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
}

#[test]
fn list_item_backspace_first_resets_then_second_merges() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "a"),
        (RichBlockKind::BulletedList, 0, None, "b"),
    ]);
    runtime.focus_block_at_offset(2, 0).unwrap();

    assert!(runtime.delete_backward().unwrap());
    assert!(matches!(
        runtime.kind_for_block(2),
        RichBlockKind::Paragraph
    ));
    assert_eq!(runtime.index.total_count(), 2);

    assert!(runtime.delete_backward().unwrap());
    assert_eq!(runtime.index.total_count(), 1);
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.focused_text(), Some("ab"));
}

#[test]
fn empty_block_backspace_and_delete_remove_block_and_focus_adjacent() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "a"),
        (RichBlockKind::Paragraph, 0, None, ""),
        (RichBlockKind::Paragraph, 0, None, "c"),
    ]);
    runtime.focus_block_at_offset(2, 0).unwrap();

    assert!(runtime.delete_backward().unwrap());
    assert_eq!(runtime.index.total_count(), 2);
    assert_eq!(runtime.focused_block_id(), Some(1));

    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "a"),
        (RichBlockKind::Paragraph, 0, None, ""),
        (RichBlockKind::Paragraph, 0, None, "c"),
    ]);
    runtime.focus_block_at_offset(2, 0).unwrap();

    assert!(runtime.delete_forward().unwrap());
    assert_eq!(runtime.index.total_count(), 2);
    assert_eq!(runtime.focused_block_id(), Some(3));
}

#[test]
fn last_empty_block_is_not_deleted() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "")]);
    runtime.focus_block_at_offset(1, 0).unwrap();

    assert!(!runtime.delete_backward().unwrap());
    assert_eq!(runtime.index.total_count(), 1);
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.focused_text(), Some(""));
}

#[test]
fn delete_at_end_merges_next_block_into_current() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "a"),
        (RichBlockKind::Paragraph, 0, None, "b"),
    ]);
    runtime.focus_block_at_offset(1, 1).unwrap();

    assert!(runtime.delete_forward().unwrap());
    assert_eq!(runtime.index.total_count(), 1);
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.focused_text(), Some("ab"));
}

#[test]
fn arrow_keys_cross_block_boundaries_and_shift_extends_selection() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "ab"),
        (RichBlockKind::Paragraph, 0, None, "cd"),
    ]);
    runtime.focus_block_at_offset(2, 0).unwrap();

    assert!(runtime.move_caret_left(false).unwrap());
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.caret_offset_for_block(1), Some(2));

    assert!(runtime.move_caret_right(false).unwrap());
    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(runtime.caret_offset_for_block(2), Some(0));

    runtime.focus_block_at_offset(1, 2).unwrap();
    assert!(runtime.move_caret_right(true).unwrap());
    assert!(runtime.has_cross_block_text_selection());
}

#[test]
fn delete_document_selection_collapses_cross_block_range() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "ab"),
        (RichBlockKind::Paragraph, 0, None, "middle"),
        (RichBlockKind::Paragraph, 0, None, "cd"),
    ]);
    runtime
        .set_document_text_selection(1, 1, 3, 1)
        .expect("selection spans loaded blocks");

    assert!(runtime.delete_document_selection().unwrap());

    assert_eq!(runtime.index.total_count(), 1);
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.focused_text(), Some("ad"));
    assert_eq!(runtime.caret_offset_for_block(1), Some(1));
}

#[test]
fn up_down_fallback_focuses_adjacent_visible_blocks() {
    let mut runtime = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "a"),
        (RichBlockKind::Paragraph, 0, None, "b"),
        (RichBlockKind::Paragraph, 0, None, "c"),
    ]);
    runtime.focus_block_at_offset(2, 1).unwrap();

    assert!(runtime.move_caret_up(false).unwrap());
    assert_eq!(runtime.focused_block_id(), Some(1));
    assert_eq!(runtime.caret_offset_for_block(1), Some(1));

    assert!(runtime.move_caret_down(false).unwrap());
    assert_eq!(runtime.focused_block_id(), Some(2));
    assert_eq!(runtime.caret_offset_for_block(2), Some(0));
}

#[test]
fn delete_backward_uses_caret_offset() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "a👨‍👩‍👧‍👦b",
        )],
        720.0,
    );
    let caret_after_emoji = "a👨‍👩‍👧‍👦".len();
    runtime.set_caret_offset(1, caret_after_emoji).unwrap();

    assert!(runtime.delete_backward().unwrap());

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "ab");
    assert_eq!(projection.blocks[0].caret_offset, Some(1));
    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(editing.input_target, InputTarget::BlockText { block_id: 1 });
    assert_eq!(editing.selected_range, 1..1);
    assert_eq!(runtime.input_session_selected_range(), Some(1..1));
}

#[test]
fn backspace_at_start_resets_textual_block_styles_to_paragraph() {
    let kinds = [
        RichBlockKind::Heading { level: 1 },
        RichBlockKind::Quote,
        RichBlockKind::Callout {
            variant: cditor_core::rich_text::CalloutVariant::Note,
        },
        RichBlockKind::Todo { checked: true },
        RichBlockKind::BulletedList,
        RichBlockKind::NumberedList,
        RichBlockKind::Toggle,
        RichBlockKind::Math,
        RichBlockKind::Mermaid,
        RichBlockKind::FootnoteDefinition,
        RichBlockKind::Comment,
        RichBlockKind::RawMarkdown,
        RichBlockKind::Custom("legacy-text".to_owned()),
    ];

    for kind in kinds {
        let mut runtime = DocumentRuntime::from_payloads(
            1,
            vec![BlockPayloadRecord::rich_text(1, kind.clone(), "keep text")],
            720.0,
        );
        runtime.focus_block_at_offset(1, 0).unwrap();

        assert!(runtime.delete_backward().unwrap(), "{kind:?} should reset");

        let projection = runtime.projection_for_window();
        assert_eq!(projection.blocks[0].kind, RichBlockKind::Paragraph);
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), "keep text");
        assert_eq!(projection.blocks[0].caret_offset, Some(0));
    }
}

#[test]
fn backspace_at_start_resets_code_and_html_payloads_to_paragraph_without_losing_text() {
    let cases = [
        BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            payload: BlockPayload::Code {
                language: Some("rust".to_owned()),
                text: "fn main() {}".to_owned(),
            },
        },
        BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Html,
            payload: BlockPayload::Html {
                html: "<b>hello</b>".to_owned(),
                sanitized: true,
            },
        },
    ];

    for record in cases {
        let expected_text = record.plain_text();
        let mut runtime = DocumentRuntime::from_payloads(1, vec![record], 720.0);
        runtime.focus_block_at_offset(1, 0).unwrap();

        assert!(runtime.delete_backward().unwrap());

        let projection = runtime.projection_for_window();
        assert_eq!(projection.blocks[0].kind, RichBlockKind::Paragraph);
        let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
            panic!("payload should be loaded");
        };
        assert_eq!(payload.plain_text(), expected_text);
        assert_eq!(projection.blocks[0].caret_offset, Some(0));
    }
}

#[test]
fn backspace_at_start_keeps_plain_paragraph_unchanged() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "plain",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 0).unwrap();

    assert!(!runtime.delete_backward().unwrap());

    let projection = runtime.projection_for_window();
    assert_eq!(projection.blocks[0].kind, RichBlockKind::Paragraph);
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "plain");
    assert_eq!(projection.blocks[0].caret_offset, Some(0));
}

#[test]
fn measured_height_above_viewport_restores_viewport_top_anchor() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let before = runtime.scroll.global_scroll_top;

    assert!(runtime.apply_measured_height(1, 1, 64.0).unwrap());

    assert_eq!(before, 3_200.0);
    assert_eq!(runtime.scroll.global_scroll_top, before + 32.0);
}

#[test]
fn measured_height_below_viewport_does_not_move_scroll_top() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let before = runtime.scroll.global_scroll_top;

    assert!(runtime.apply_measured_height(900, 1, 64.0).unwrap());

    assert_eq!(runtime.scroll.global_scroll_top, before);
}

#[test]
fn measured_height_rejects_stale_content_version() {
    let mut runtime = DocumentRuntime::demo();
    runtime.focus_block(3);
    runtime.insert_char('!').unwrap();

    let applied = runtime.apply_measured_height(3, 1, 96.0).unwrap();

    assert!(!applied);
    let block_index = runtime.index.index_of(3).unwrap();
    assert_ne!(
        runtime.index.layout_meta[block_index].measured_height,
        Some(96.0)
    );
}

#[test]
fn editing_code_block_preserves_code_payload() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 1,
            content_version: 1,
            kind: RichBlockKind::Code {
                language: Some("rust".to_owned()),
            },
            payload: BlockPayload::Code {
                language: Some("rust".to_owned()),
                text: "fn main()".to_owned(),
            },
        }],
        720.0,
    );

    runtime.focus_block_at_offset(1, 2).unwrap();
    runtime.insert_char('x').unwrap();

    let payload = runtime.payload_window.get(1).unwrap();
    match &payload.payload {
        BlockPayload::Code { language, text } => {
            assert_eq!(language.as_deref(), Some("rust"));
            assert_eq!(text, "fnx main()");
        }
        _ => panic!("expected code payload after editing code block"),
    }
    assert_eq!(runtime.caret_offset_for_block(1), Some(3));
}

#[test]
fn code_block_estimate_includes_language_label_and_chrome() {
    let payload = BlockPayloadRecord {
        block_id: 1,
        content_version: 1,
        kind: RichBlockKind::Code {
            language: Some("rust".to_owned()),
        },
        payload: BlockPayload::Code {
            language: Some("rust".to_owned()),
            text: "fn main() {\n    let value = 1;\n    value + 1\n}".to_owned(),
        },
    };

    assert!(estimate_payload_height(&payload, 0) >= RichBlockRecord::DEFAULT_CODE_HEIGHT);
    assert!(estimate_payload_height(&payload, 0) >= 130.0);
}

#[test]
fn enter_in_quote_soft_wraps_and_grows_block_height() {
    let records = vec![
        BlockIndexRecord::new(
            1,
            None,
            0,
            kind_tag_for_rich_block_kind(&RichBlockKind::Quote),
            0,
        )
        .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(1, 36.0)),
    ];
    let payloads = vec![BlockPayloadRecord::rich_text(
        1,
        RichBlockKind::Quote,
        "> 引用块: UI 只是投影，runtime 才是真相。",
    )];
    let mut runtime = DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0);
    runtime.focus_block(1);
    let before = runtime
        .index
        .index_of(1)
        .map(|index| runtime.index.layout_meta[index].effective_height())
        .unwrap();

    runtime.handle_enter().unwrap();

    assert!(runtime.focused_text().unwrap().contains('\n'));
    let after = runtime
        .index
        .index_of(1)
        .map(|index| runtime.index.layout_meta[index].effective_height())
        .unwrap();
    assert!(
        after > before,
        "quote height should grow: {before} -> {after}"
    );
}

#[test]
fn document_text_selection_projects_partial_and_full_ranges() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "abcd"),
            BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "efgh"),
            BlockPayloadRecord::rich_text(3, RichBlockKind::Paragraph, "ijkl"),
        ],
        720.0,
    );

    runtime.set_document_text_selection(1, 2, 3, 1).unwrap();
    let projection = runtime.projection_for_window();

    assert_eq!(
        projection.blocks[0].selection_range,
        Some(SelectionRange::Partial(2..4))
    );
    assert_eq!(
        projection.blocks[1].selection_range,
        Some(SelectionRange::Full)
    );
    assert_eq!(
        projection.blocks[2].selection_range,
        Some(SelectionRange::Partial(0..1))
    );
    assert_eq!(
        runtime.selected_document_text().as_deref(),
        Some("cd\nefgh\ni")
    );
}

#[test]
fn focused_text_selection_replaces_and_moves_with_shift_arrows() {
    let mut runtime = DocumentRuntime::demo();
    runtime.focus_block_at_offset(3, 0).unwrap();

    assert!(runtime.move_caret_right(true).unwrap());
    let selected = runtime.selected_focused_text().unwrap();
    assert_eq!(selected.chars().count(), 1);

    runtime.insert_char('你').unwrap();
    assert!(runtime.focused_text_selection_range().is_none());
    assert!(runtime.focused_text().unwrap().starts_with('你'));
}

#[test]
fn select_all_copy_cut_paste_and_inline_mark_work_on_focused_text() {
    let mut runtime = DocumentRuntime::demo();
    runtime.focus_block_at_offset(3, 0).unwrap();
    assert!(runtime.select_focused_text_all());
    let selected = runtime.selected_focused_text().unwrap();
    assert!(!selected.is_empty());

    assert!(
        runtime
            .toggle_inline_mark_on_selection(InlineMark::Bold)
            .unwrap()
    );
    let payload = runtime.payload_window.get(3).unwrap();
    match &payload.payload {
        BlockPayload::RichText { spans } => {
            assert!(
                spans
                    .iter()
                    .any(|span| span.marks.contains(&InlineMark::Bold))
            );
        }
        _ => panic!("expected rich text payload"),
    }
    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(editing.input_target, InputTarget::BlockText { block_id: 3 });
    assert_eq!(editing.selected_range, 0..selected.len());

    assert!(
        runtime
            .replace_text_in_focused_range(None, "粘贴文本")
            .unwrap()
    );
    assert_eq!(runtime.focused_text(), Some("粘贴文本"));
}

#[test]
fn queued_measured_heights_do_not_apply_until_flush() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let before_scroll_top = runtime.scroll.global_scroll_top;
    let before_total_height = runtime.height_index.total_height();

    assert!(runtime.queue_measured_height(1, 1, 64.0).unwrap());

    assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
    assert_eq!(runtime.height_index.total_height(), before_total_height);

    assert!(runtime.flush_pending_height_corrections().unwrap());
    assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top + 32.0);
    assert_eq!(
        runtime.height_index.total_height(),
        before_total_height + 32.0
    );
}

#[test]
fn flush_measured_heights_restores_anchor_once_for_batched_changes() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let before = runtime.scroll.global_scroll_top;

    assert!(runtime.queue_measured_height(1, 1, 64.0).unwrap());
    assert!(runtime.queue_measured_height(2, 1, 72.0).unwrap());
    assert!(runtime.queue_measured_height(3, 1, 80.0).unwrap());
    assert!(runtime.flush_pending_height_corrections().unwrap());

    assert_eq!(
        runtime.scroll.global_scroll_top,
        before + 32.0 + 40.0 + 48.0
    );
}

#[test]
fn flush_discards_stale_measured_height_versions() {
    let mut runtime = DocumentRuntime::demo();
    runtime.focus_block(3);

    assert!(runtime.queue_measured_height(3, 1, 96.0).unwrap());
    runtime.insert_char('!').unwrap();

    assert!(!runtime.flush_pending_height_corrections().unwrap());
    let block_index = runtime.index.index_of(3).unwrap();
    assert_ne!(
        runtime.index.layout_meta[block_index].measured_height,
        Some(96.0)
    );
}

#[test]
fn flush_below_viewport_heights_does_not_move_scroll_top() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let before = runtime.scroll.global_scroll_top;

    assert!(runtime.queue_measured_height(900, 1, 64.0).unwrap());
    assert!(runtime.queue_measured_height(901, 1, 72.0).unwrap());
    assert!(runtime.flush_pending_height_corrections().unwrap());

    assert_eq!(runtime.scroll.global_scroll_top, before);
}

#[test]
fn wheel_scroll_height_flush_preserves_user_scroll_top_without_bounce() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    runtime.scroll_by_delta(-64.0).unwrap();
    let before_scroll_top = runtime.scroll.global_scroll_top;
    let before_total_height = runtime.height_index.total_height();

    assert!(runtime.queue_measured_height(1, 1, 64.0).unwrap());
    assert!(
        runtime
            .flush_pending_height_corrections_with_priority(HeightCorrectionPriority::DeferRemote)
            .unwrap()
    );

    assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
    assert_eq!(
        runtime.height_index.total_height(),
        before_total_height + 32.0
    );
    assert_eq!(
        runtime.scroll.model_total_height,
        runtime.scroll_extent_height(before_total_height + 32.0)
    );
    assert!(runtime.pending_measured_heights.is_empty());
}

#[test]
fn scrollbar_drag_freezes_displayed_total_and_defers_anchor_restore() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let policy = ScrollbarPolicy {
        track_height: 720.0,
        ..ScrollbarPolicy::default()
    };
    let before_scroll_top = runtime.scroll.global_scroll_top;
    let before_total_height = runtime.scroll.displayed_total_height;

    let visual = runtime.begin_scrollbar_drag(policy);
    assert!(visual.enabled);
    assert!(runtime.queue_measured_height(1, 1, 64.0).unwrap());
    assert!(runtime.flush_pending_height_corrections().unwrap());

    assert_eq!(runtime.scroll.global_scroll_top, before_scroll_top);
    assert_eq!(
        runtime.scroll.model_total_height,
        runtime.scroll_extent_height(before_total_height + 32.0)
    );
    assert_eq!(runtime.scroll.displayed_total_height, before_total_height);

    let end = runtime.finish_scrollbar_drag().unwrap().unwrap();
    assert_eq!(end.pending_layout_corrections, 1);
    assert_eq!(
        runtime.scroll.displayed_total_height,
        runtime.scroll.model_total_height
    );
}

#[test]
fn scrollbar_drag_uses_frozen_total_height_for_thumb_mapping() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    let policy = ScrollbarPolicy {
        track_height: 720.0,
        ..ScrollbarPolicy::default()
    };
    let visual = runtime.begin_scrollbar_drag(policy);
    let max_thumb_top = policy.track_height - visual.thumb_height;

    let update = runtime
        .drag_scrollbar_to_thumb_top(policy, max_thumb_top)
        .unwrap()
        .unwrap();

    assert_eq!(update.drag_ratio, 1.0);
    assert_eq!(
        runtime.scroll.global_scroll_top,
        runtime.scroll.max_scroll_top()
    );
    assert!(runtime.finish_scrollbar_drag().unwrap().is_some());
}

#[test]
fn rich_text_height_updates_after_wrap() {
    let long_text = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            long_text,
        )],
        720.0,
    );
    let snapshot = runtime.projection_for_window().blocks[0].clone();
    let content_version = runtime.block_content_version(snapshot.block_id).unwrap();
    let measured_height = RichBlockRecord::DEFAULT_TEXT_HEIGHT * 2.0;

    let applied = runtime
        .apply_measured_height(snapshot.block_id, content_version, measured_height)
        .unwrap();

    assert!(applied);
    let updated = runtime.projection_for_window();
    assert_eq!(
        updated.blocks[0].layout.measured_height,
        Some(measured_height)
    );
    assert_eq!(runtime.height_index.total_height(), measured_height);
    assert_eq!(runtime.page_layout.total_height(), measured_height);
}

#[test]
fn document_runtime_scroll_by_delta_clamps_and_updates_page_window() {
    let mut runtime = runtime_with_paragraph_blocks(100_000);
    let initial_window = runtime.current_page_window();

    runtime.scroll_by_delta(50_000.0).unwrap();

    assert_eq!(runtime.scroll.global_scroll_top, 50_000.0);
    assert_ne!(runtime.current_page_window(), initial_window);

    runtime.scroll_by_delta(-100_000.0).unwrap();
    assert_eq!(runtime.scroll.global_scroll_top, 0.0);

    runtime.scroll_by_delta(10_000_000.0).unwrap();
    assert_eq!(
        runtime.scroll.global_scroll_top,
        runtime.scroll.max_scroll_top()
    );
}

#[test]
fn projection_window_spacer_heights_sum_to_total() {
    let mut runtime = runtime_with_paragraph_blocks(100_000);
    let middle_page = runtime.page_layout.page_count() / 2;
    let middle_offset = runtime.page_layout.offset_of_page(middle_page).unwrap();
    runtime
        .scroll
        .scroll_to_global_offset(
            middle_offset,
            cditor_editor::scroll::ScrollOrigin::ProgrammaticVirtualScroll,
        )
        .unwrap();

    let projection = runtime.projection_for_window();
    let projected_total = projection.before_window_height
        + projection.render_window.height()
        + projection.after_window_height;

    assert!(projection.before_window_height > 0.0);
    assert!(
        (projected_total - runtime.scroll_extent_height(runtime.page_layout.total_height())).abs()
            < 0.001
    );
}

#[test]
fn projection_for_window_limits_blocks_for_100k_document() {
    let runtime = runtime_with_paragraph_blocks(100_000);

    let projection = runtime.projection_for_window();

    assert_eq!(projection.total_visible_blocks, 100_000);
    assert!(projection.blocks.len() < 10_000);
    assert_eq!(
        projection.render_window.block_range.len(),
        projection.blocks.len()
    );
    assert_eq!(
        projection.render_window.page_range,
        runtime.current_page_window()
    );
    assert_eq!(projection.blocks.first().unwrap().visible_index, 0);
    assert_eq!(
        projection.blocks.last().unwrap().visible_index + 1,
        projection.render_window.block_range.end
    );
}

#[test]
fn current_page_window_clamps_first_middle_and_last_pages() {
    let mut runtime = runtime_with_paragraph_blocks(3_000);
    let page_count = runtime.page_layout.page_count();
    assert!(page_count >= 4);

    assert_eq!(runtime.current_page_window().start, 0);
    assert!(runtime.current_page_window().contains(&0));

    let middle_page = page_count / 2;
    let middle_offset = runtime.page_layout.offset_of_page(middle_page).unwrap();
    runtime
        .scroll
        .scroll_to_global_offset(
            middle_offset,
            cditor_editor::scroll::ScrollOrigin::ProgrammaticVirtualScroll,
        )
        .unwrap();
    let middle_window = runtime.current_page_window();
    assert!(middle_window.contains(&middle_page));
    assert_eq!(middle_window.start, middle_page.saturating_sub(1));
    assert!(middle_window.end <= page_count);

    runtime
        .scroll
        .scroll_to_global_offset(
            runtime.scroll.model_total_height,
            cditor_editor::scroll::ScrollOrigin::ProgrammaticVirtualScroll,
        )
        .unwrap();
    let last_window = runtime.current_page_window();
    assert!(last_window.contains(&(page_count - 1)));
    assert_eq!(last_window.end, page_count);
}

#[test]
fn document_runtime_insert_char_updates_payload_and_pins_editing_block() {
    let mut runtime = DocumentRuntime::demo();
    runtime.focus_block(3);
    runtime.insert_char('!').unwrap();
    let projection = runtime.projection();
    let block = projection
        .blocks
        .iter()
        .find(|block| block.block_id == 3)
        .unwrap();
    assert!(block.focused);
    assert!(block.pinned);
    let BlockPayloadView::Loaded(payload) = &block.payload else {
        panic!("payload should be loaded");
    };
    assert!(payload.plain_text().ends_with('!'));
}

#[test]
fn document_runtime_delete_backward_removes_one_grapheme() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "a👨‍👩‍👧‍👦",
        )],
        720.0,
    );
    runtime.focus_block(1);

    assert!(runtime.delete_backward().unwrap());

    let projection = runtime.projection();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "a");
}

#[test]
fn select_all_marks_visible_projection_without_ui_truth() {
    let mut runtime = DocumentRuntime::demo();
    assert!(runtime.select_all_visible_blocks());

    let projection = runtime.projection();
    assert!(projection.blocks.iter().all(|block| block.selected));
    assert_eq!(projection.blocks.len(), 4);
}

#[test]
fn undo_and_redo_restore_focused_block_text_snapshot() {
    let mut runtime = DocumentRuntime::demo();
    runtime.focus_block(3);
    runtime.insert_char('!').unwrap();

    assert!(runtime.undo_focused_block().unwrap());
    let projection = runtime.projection();
    let block = projection
        .blocks
        .iter()
        .find(|block| block.block_id == 3)
        .unwrap();
    let BlockPayloadView::Loaded(payload) = &block.payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "点击窗口后直接输入文本。");

    assert!(runtime.redo_focused_block().unwrap());
    let projection = runtime.projection();
    let block = projection
        .blocks
        .iter()
        .find(|block| block.block_id == 3)
        .unwrap();
    let BlockPayloadView::Loaded(payload) = &block.payload else {
        panic!("payload should be loaded");
    };
    assert!(payload.plain_text().ends_with('!'));
}

#[test]
fn undo_and_redo_restore_block_kind_style_snapshot() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "#",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 1).unwrap();

    runtime.insert_space_or_markdown_shortcut().unwrap();
    assert!(matches!(
        runtime.projection().blocks[0].kind,
        RichBlockKind::Heading { level: 1 }
    ));

    assert!(runtime.undo_focused_block().unwrap());
    let projection = runtime.projection();
    assert_eq!(projection.blocks[0].kind, RichBlockKind::Paragraph);
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "#");

    assert!(runtime.redo_focused_block().unwrap());
    let projection = runtime.projection();
    assert!(matches!(
        projection.blocks[0].kind,
        RichBlockKind::Heading { level: 1 }
    ));
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "");
    let editing = runtime.editing.as_ref().unwrap();
    assert_eq!(editing.input_target, InputTarget::BlockText { block_id: 1 });
    assert_eq!(editing.selected_range, 0..0);
}

#[test]
fn undo_and_redo_restore_inline_mark_style_snapshot() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "bold",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 0).unwrap();
    runtime.move_focused_caret_to_offset(1, 4, true).unwrap();

    assert!(
        runtime
            .toggle_inline_mark_on_selection(InlineMark::Bold)
            .unwrap()
    );
    assert!(
        runtime
            .loaded_payload_records_snapshot()
            .iter()
            .any(|record| {
                matches!(
                    &record.payload,
                    BlockPayload::RichText { spans }
                        if spans.iter().any(|span| span.marks.contains(&InlineMark::Bold))
                )
            })
    );

    assert!(runtime.undo_focused_block().unwrap());
    assert!(
        runtime
            .loaded_payload_records_snapshot()
            .iter()
            .all(|record| {
                matches!(
                    &record.payload,
                    BlockPayload::RichText { spans }
                        if spans.iter().all(|span| !span.marks.contains(&InlineMark::Bold))
                )
            })
    );

    assert!(runtime.redo_focused_block().unwrap());
    assert!(
        runtime
            .loaded_payload_records_snapshot()
            .iter()
            .any(|record| {
                matches!(
                    &record.payload,
                    BlockPayload::RichText { spans }
                        if spans.iter().any(|span| span.marks.contains(&InlineMark::Bold))
                )
            })
    );
}

#[test]
fn ctrl_enter_inserts_new_paragraph_after_focused_block() {
    let mut runtime = DocumentRuntime::demo();
    runtime.focus_block(3);

    let new_block_id = runtime.insert_paragraph_after_focused().unwrap();

    assert_eq!(new_block_id, 5);
    assert_eq!(runtime.focused_block_id(), Some(5));
    let projection = runtime.projection();
    assert_eq!(projection.blocks.len(), 5);
    assert_eq!(projection.blocks[3].block_id, 5);
    assert!(matches!(
        projection.blocks[3].kind,
        RichBlockKind::Paragraph
    ));
}

#[test]
fn shift_enter_inserts_soft_line_break_in_focused_block() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "first line",
        )],
        720.0,
    );
    runtime.focus_block(1);
    let before_height = runtime.projection().blocks[0].layout.effective_height();
    let before_total_height = runtime.height_index.total_height();

    runtime.insert_soft_line_break().unwrap();

    let projection = runtime.projection();
    let block = &projection.blocks[0];
    let BlockPayloadView::Loaded(payload) = &block.payload else {
        panic!("payload should be loaded");
    };
    assert!(payload.plain_text().ends_with('\n'));
    assert!(
        block.layout.effective_height() > before_height,
        "soft line break should grow block height: {} <= {before_height}",
        block.layout.effective_height()
    );
    assert!(runtime.height_index.total_height() > before_total_height);
    assert_eq!(
        runtime.page_layout.total_height(),
        runtime.height_index.total_height()
    );
    assert_eq!(
        runtime.scroll.model_total_height,
        runtime.scroll_extent_height(runtime.height_index.total_height())
    );
}

#[test]
fn space_shortcut_turns_marker_into_heading_block() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "#",
        )],
        720.0,
    );
    runtime.focus_block(1);

    runtime.insert_space_or_markdown_shortcut().unwrap();

    let projection = runtime.projection();
    assert!(matches!(
        projection.blocks[0].kind,
        RichBlockKind::Heading { level: 1 }
    ));
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "");
}

#[test]
fn space_shortcut_turns_markdown_task_marker_into_todo_block() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "- [ ]",
        )],
        720.0,
    );
    runtime.focus_block(1);

    runtime.insert_space_or_markdown_shortcut().unwrap();

    let projection = runtime.projection();
    assert_eq!(
        projection.blocks[0].kind,
        RichBlockKind::Todo { checked: false }
    );
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "");
}

#[test]
fn space_shortcut_turns_checked_markdown_task_marker_into_todo_block() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "- [x]",
        )],
        720.0,
    );
    runtime.focus_block(1);

    runtime.insert_space_or_markdown_shortcut().unwrap();

    let projection = runtime.projection();
    assert_eq!(
        projection.blocks[0].kind,
        RichBlockKind::Todo { checked: true }
    );
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "");
}

#[test]
fn typing_markdown_task_marker_from_empty_block_turns_bullet_into_todo() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "-",
        )],
        720.0,
    );
    runtime.focus_block_at_offset(1, 1).unwrap();

    runtime.insert_space_or_markdown_shortcut().unwrap();
    assert_eq!(
        runtime.projection().blocks[0].kind,
        RichBlockKind::BulletedList
    );

    runtime.insert_char('[').unwrap();
    runtime.insert_space_or_markdown_shortcut().unwrap();
    runtime.insert_char(']').unwrap();
    runtime.insert_space_or_markdown_shortcut().unwrap();

    let projection = runtime.projection();
    assert_eq!(
        projection.blocks[0].kind,
        RichBlockKind::Todo { checked: false }
    );
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "");
}

#[test]
fn enter_shortcut_turns_code_fence_into_code_block() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "```rust",
        )],
        720.0,
    );
    runtime.focus_block(1);

    runtime.handle_enter().unwrap();

    let projection = runtime.projection();
    assert!(matches!(
        projection.blocks[0].kind,
        RichBlockKind::Code { ref language } if language.as_deref() == Some("rust")
    ));
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    assert_eq!(payload.plain_text(), "");
}

#[test]
fn inline_markdown_shortcut_updates_payload_spans() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "hello **bold**",
        )],
        720.0,
    );
    runtime.focus_block(1);
    runtime.insert_char('!').unwrap();

    let projection = runtime.projection();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("payload should be loaded");
    };
    let BlockPayload::RichText { spans } = &payload.payload else {
        panic!("payload should be rich text");
    };
    assert_eq!(payload.plain_text(), "hello bold!");
    assert!(spans.iter().any(|span| {
        span.text == "bold"
            && span
                .marks
                .contains(&cditor_core::rich_text::InlineMark::Bold)
    }));
}

#[test]
fn planned_payload_window_without_records_does_not_render_per_block_placeholders() {
    let records = (1..=1_000 as BlockId)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
            .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(block_id, 32.0))
        })
        .collect::<Vec<_>>();
    let payloads = (1..=64 as BlockId)
        .map(|block_id| BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, ""))
        .collect::<Vec<_>>();
    let mut runtime =
        DocumentRuntime::from_index_records_with_window(1, records, payloads, 1, 720.0, 0..64);
    runtime.plan_payload_window_load(400..430);
    runtime
        .scroll
        .scroll_to_global_offset(400.0 * 32.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();

    let projection = runtime.projection_for_window();

    assert!(projection.render_window.is_placeholder());
    assert!(projection.blocks.is_empty());
    assert!(projection.placeholder_window_height.is_some());
}

#[test]
fn payload_window_store_request_prioritizes_focus_and_selection_endpoints() {
    let mut runtime = runtime_with_paragraph_blocks(10);
    runtime.focus_block(5);
    runtime.select_all_visible_blocks();

    let request = runtime.plan_payload_window_load(3..6);

    assert_eq!(request.generation, 1);
    assert_eq!(request.block_range, 3..6);
    assert_eq!(&request.block_ids[..3], &[5, 1, 10]);
    assert!(request.block_ids.contains(&4));
    assert!(request.block_ids.contains(&6));
}

#[test]
fn payload_window_store_discards_stale_generation_result() {
    let mut runtime = runtime_with_paragraph_blocks(4);
    let stale = runtime.plan_payload_window_load(0..2);
    let current = runtime.plan_payload_window_load(2..4);
    assert_eq!(current.generation, 2);

    let decision = runtime.apply_payload_window_result(PayloadWindowLoadResult {
        request: stale,
        records: Vec::new(),
        missing_block_ids: Vec::new(),
    });

    assert_eq!(
        decision,
        PayloadWindowApplyDecision::DiscardedStaleGeneration {
            expected: 2,
            actual: 1,
        }
    );
    assert_eq!(runtime.payload_window.block_range, 2..4);
}

#[test]
fn payload_window_store_marks_loading_and_missing_payload_errors() {
    let records = (1..=3)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
        })
        .collect::<Vec<_>>();
    let mut runtime =
        DocumentRuntime::from_index_records_with_window(1, records, Vec::new(), 1, 720.0, 0..0);

    let request = runtime.plan_payload_window_load(0..2);
    assert!(runtime.payload_window.loading.contains(&1));
    assert!(runtime.payload_window.loading.contains(&2));

    let decision = runtime.apply_payload_window_result(PayloadWindowLoadResult {
        request,
        records: Vec::new(),
        missing_block_ids: vec![1, 2],
    });

    assert_eq!(decision, PayloadWindowApplyDecision::Applied);
    assert!(runtime.payload_window.loading.is_empty());
    assert!(runtime.payload_window.failed.contains_key(&1));
    assert!(runtime.payload_window.failed.contains_key(&2));
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn payload_window_store_loads_requested_window_from_postgres() {
    let (document_store, payload_store, _layout_store, document, base_block_id) =
        postgres_runtime_fixture(81_001).await;
    let records = sample_index_records(base_block_id, 4);
    let payloads = sample_payloads(base_block_id, 4);
    document_store
        .save_block_index_records(document.id, &records, 1)
        .await
        .unwrap();
    payload_store
        .save_block_payloads(document.id, &payloads)
        .await
        .unwrap();
    let mut runtime = DocumentRuntime::from_index_records_with_window(
        81_001,
        records,
        Vec::new(),
        1,
        720.0,
        0..0,
    );

    let decision = runtime
        .load_payload_window_from_store(&payload_store, 1..3)
        .await
        .unwrap();

    assert_eq!(decision, PayloadWindowApplyDecision::Applied);
    assert_eq!(runtime.payload_window.block_range, 1..3);
    assert_eq!(runtime.payload_window.payloads.len(), 2);
    assert!(
        runtime
            .payload_window
            .payloads
            .contains_key(&(base_block_id + 1))
    );
    assert!(
        runtime
            .payload_window
            .payloads
            .contains_key(&(base_block_id + 2))
    );
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn runtime_from_store_loads_metadata_snapshot_layout_and_initial_payload_window() {
    let (document_store, payload_store, layout_store, document, base_block_id) =
        postgres_runtime_fixture(80_001).await;
    let records = sample_index_records(base_block_id, 4);
    let payloads = sample_payloads(base_block_id, 4);
    document_store
        .save_block_index_records(document.id, &records, 1)
        .await
        .unwrap();
    payload_store
        .save_block_payloads(document.id, &payloads)
        .await
        .unwrap();
    document_store
        .save_document_index_snapshot(document.id, 0, 1, &records)
        .await
        .unwrap();
    let layout_key = runtime_store_layout_key();
    layout_store
        .save_block_layout(
            document.id,
            &cditor_storage::layout_cache::BlockLayoutRow::new(
                base_block_id,
                layout_key,
                HeightEstimate::new(123.0, HeightConfidence::Exact, 0.0),
            ),
        )
        .await
        .unwrap();

    let (runtime, report) = DocumentRuntime::from_store(
        document.id,
        &document_store,
        &payload_store,
        &layout_store,
        DocumentRuntimeFromStoreOptions {
            initial_payload_window_blocks: 2,
            layout_key,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(report.document_title, document.title);
    assert_eq!(report.index_source, DocumentRuntimeIndexSource::Snapshot);
    assert_eq!(report.total_blocks, 4);
    assert_eq!(report.payloads_loaded, 2);
    assert_eq!(report.payloads_missing, 0);
    assert_eq!(report.layout_cache_hits, 1);
    assert_eq!(runtime.index.total_count(), 4);
    assert_eq!(runtime.payload_window.block_range, 0..2);
    assert_eq!(runtime.payload_window.payloads.len(), 2);
    assert_eq!(runtime.index.layout_meta[0].measured_height, Some(123.0));
}

#[tokio::test]
#[ignore = "requires docker compose postgres_test and CDITOR_TEST_DATABASE_URL"]
async fn runtime_from_store_rebuilds_from_blocks_when_snapshot_is_stale() {
    let (document_store, payload_store, layout_store, document, base_block_id) =
        postgres_runtime_fixture(80_002).await;
    let stale_records = sample_index_records(base_block_id, 2);
    document_store
        .save_block_index_records(document.id, &stale_records, 1)
        .await
        .unwrap();
    document_store
        .save_document_index_snapshot(document.id, 0, 1, &stale_records)
        .await
        .unwrap();

    let current_records = sample_index_records(base_block_id, 3);
    let current_payloads = sample_payloads(base_block_id, 1);
    document_store
        .save_block_index_records(document.id, &current_records, 2)
        .await
        .unwrap();
    payload_store
        .save_block_payloads(document.id, &current_payloads)
        .await
        .unwrap();

    let (runtime, report) = DocumentRuntime::from_store(
        document.id,
        &document_store,
        &payload_store,
        &layout_store,
        DocumentRuntimeFromStoreOptions {
            initial_payload_window_blocks: 2,
            layout_key: runtime_store_layout_key(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert_eq!(report.index_source, DocumentRuntimeIndexSource::Blocks);
    assert_eq!(runtime.index.total_count(), 3);
    assert_eq!(runtime.index.structure_version, 2);
    assert_eq!(report.payloads_loaded, 1);
    assert_eq!(report.payloads_missing, 1);
}

fn sample_table_payload() -> BlockPayloadRecord {
    let mut table = cditor_core::rich_text::TablePayload {
        rows: vec![
            cditor_core::rich_text::TableRowPayload {
                cells: vec![
                    cditor_core::rich_text::TableCellPayload::plain("A"),
                    cditor_core::rich_text::TableCellPayload::plain("B"),
                ],
                height: Default::default(),
            },
            cditor_core::rich_text::TableRowPayload {
                cells: vec![
                    cditor_core::rich_text::TableCellPayload::plain("C"),
                    cditor_core::rich_text::TableCellPayload::plain("D"),
                ],
                height: Default::default(),
            },
        ],
        columns: Vec::new(),
        header_rows: 1,
        header_cols: 0,
        header_style: cditor_core::rich_text::TableHeaderStyle::default(),
    };
    table.normalize();
    BlockPayloadRecord {
        block_id: 10,
        content_version: 1,
        kind: RichBlockKind::Table,
        payload: BlockPayload::Table(table),
    }
}

#[test]
fn table_cell_focus_is_projected_without_ui_entity_state() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell(10, 0, 1).unwrap();
    let projection = runtime.projection_for_window();

    assert_eq!(runtime.focused_block_id(), Some(10));
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 1, 1)));
    assert_eq!(
        projection.blocks[0].focused_table_cell,
        Some(TableCellPosition { row: 0, col: 1 })
    );
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table view state");
    assert_eq!(
        table_view.focused_cell,
        Some(TableCellPosition { row: 0, col: 1 })
    );
    assert_eq!(table_view.focused_cell_offset, Some(1));
}

#[test]
fn blur_table_cell_exits_cell_editing_without_writing_old_cell() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell_at_offset(10, 0, 1, 1).unwrap();
    assert!(runtime.blur_table_cell());
    runtime.insert_char('x').unwrap();

    assert_eq!(runtime.focused_block_id(), Some(10));
    assert_eq!(runtime.focused_table_cell_offset(), None);
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some("B"));
}

#[test]
fn focused_table_cell_persists_selection_marked_range_and_direction() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell_at_offset(10, 0, 1, 1).unwrap();
    assert_eq!(
        runtime.focused_table_cell_selection_state(),
        Some((10, 0, 1, 1..1, false, None))
    );

    runtime
        .begin_or_update_composition_with_selection(10, 0..1, "中", Some(0.."中".len()))
        .unwrap();
    assert_eq!(
        runtime.focused_table_cell_selection_state(),
        Some((10, 0, 1, 0.."中".len(), false, Some(0.."中".len())))
    );
    assert_eq!(runtime.input_session_selected_range(), Some(0.."中".len()));
    assert_eq!(runtime.input_session_marked_range(), Some(0.."中".len()));

    runtime.cancel_composition();
    assert_eq!(
        runtime.focused_table_cell_selection_state(),
        Some((10, 0, 1, 0.."中".len(), false, None))
    );

    runtime
        .replace_text_in_focused_range(Some(0..1), "Z")
        .unwrap();
    assert_eq!(
        runtime.focused_table_cell_selection_state(),
        Some((10, 0, 1, 1..1, false, None))
    );
}

#[test]
fn insert_char_updates_focused_table_cell_payload() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell(10, 0, 1).unwrap();
    runtime.insert_char('!').unwrap();

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 1), Some("B!".to_owned()));
    assert_eq!(payload.content_version, 2);
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 1, 2)));
}

#[test]
fn delete_backward_and_forward_update_focused_table_cell_payload() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell(10, 0, 1).unwrap();
    runtime.insert_char('中').unwrap();
    assert!(runtime.delete_backward().unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 1, 1)));
    runtime.insert_char('x').unwrap();
    runtime.focused_table_cell = Some(FocusedTableCell::collapsed(10, 0, 1, 1));
    assert!(runtime.delete_forward().unwrap());

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 1), Some("B".to_owned()));
}

#[test]
fn table_cell_arrow_navigation_updates_focus_and_input_session() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell_at_offset(10, 0, 1, 1).unwrap();
    assert!(runtime.move_focused_table_cell_left().unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 1, 0)));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::TableCell {
            block_id: 10,
            row: 0,
            col: 1,
        })
    );
    assert_eq!(runtime.input_session_selected_range(), Some(0..0));

    assert!(runtime.move_focused_table_cell_left().unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 0, 1)));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::TableCell {
            block_id: 10,
            row: 0,
            col: 0,
        })
    );
    assert_eq!(runtime.input_session_selected_range(), Some(1..1));

    assert!(runtime.move_focused_table_cell_down().unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 1, 0, 1)));
}

#[test]
fn table_cell_tab_navigation_updates_focus_and_input_session() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell_at_offset(10, 0, 0, 1).unwrap();

    assert!(runtime.move_focused_table_cell_tab(false).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 1, 0)));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::TableCell {
            block_id: 10,
            row: 0,
            col: 1,
        })
    );
    assert_eq!(runtime.input_session_selected_range(), Some(0..0));

    assert!(runtime.move_focused_table_cell_tab(true).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 0, 1)));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::TableCell {
            block_id: 10,
            row: 0,
            col: 0,
        })
    );
    assert_eq!(runtime.input_session_selected_range(), Some(1..1));
}

#[test]
fn plain_text_input_on_table_block_without_cell_focus_preserves_table_payload() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    assert!(!runtime.text_models.contains_key(&10));
    runtime.focus_block(10);
    assert_eq!(runtime.focused_text(), None);
    runtime.insert_char('x').unwrap();
    assert!(!runtime.replace_text_in_focused_range(None, "y").unwrap());
    runtime
        .begin_or_update_composition_with_selection(10, 0..0, "中", None)
        .unwrap();

    assert_eq!(runtime.focused_text_for_platform_input(), None);
    assert!(runtime.active_composition().is_none());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.cell_plain_text(0, 0), Some("A".to_owned()));
    assert_eq!(table.cell_plain_text(0, 1), Some("B".to_owned()));
}

#[test]
fn merge_table_cells_updates_payload_runtime_and_content_version() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    assert!(
        runtime
            .merge_table_cells(
                10,
                cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1)
            )
            .unwrap()
    );

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(payload.content_version, 2);
    assert_eq!(
        table.rows[0].cells[0].merge,
        cditor_core::rich_text::TableCellMerge::Origin {
            row_span: 2,
            col_span: 2
        }
    );
    assert_eq!(table.cell_origin(1, 1), Some((0, 0)));
    assert_eq!(table.cell_plain_text(1, 1), Some("A\tB\nC\tD".to_owned()));
}

#[test]
fn focusing_covered_table_cell_targets_merge_origin_cell() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    runtime.focus_table_cell_at_offset(10, 1, 1, 1).unwrap();

    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 0, 1)));
    assert_eq!(
        runtime.input_session_target(),
        Some(InputTarget::TableCell {
            block_id: 10,
            row: 0,
            col: 0,
        })
    );
    assert_eq!(runtime.input_session_selected_range(), Some(1..1));
}

#[test]
fn projection_outputs_visible_cells_for_merged_table_geometry() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    let merged = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 0, col: 0 }))
        .expect("merged origin cell");

    assert_eq!(table_view.visible_cells.len(), 1);
    assert_eq!(merged.row_span, 2);
    assert_eq!(merged.col_span, 2);
    assert_eq!(merged.x_px, 0.0);
    assert_eq!(merged.y_px, 0.0);
    assert_eq!(merged.width_px, 240.0);
    assert_eq!(merged.height_px, 72.0);
}

#[test]
fn merged_table_projection_hit_test_area_targets_origin_for_covered_cell_space() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    let covered_cell_center_x = 180.0;
    let covered_cell_center_y = 54.0;
    let hit = table_view
        .visible_cells
        .iter()
        .find(|cell| {
            covered_cell_center_x >= cell.x_px
                && covered_cell_center_x < cell.x_px + cell.width_px
                && covered_cell_center_y >= cell.y_px
                && covered_cell_center_y < cell.y_px + cell.height_px
        })
        .expect("covered cell area should be inside merged origin rect");

    assert_eq!(table_view.visible_cells.len(), 1);
    assert_eq!(hit.position, TableCellPosition { row: 0, col: 0 });
    runtime.focus_table_cell_at_offset(10, 1, 1, 1).unwrap();
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 0, 1)));
}

#[test]
fn table_selection_axis_and_cell_predicates_are_block_scoped() {
    let runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    let row_selection = runtime
        .table_row_selection_range(10, 1)
        .expect("row selection");
    assert_eq!(
        row_selection,
        cditor_core::rich_text::TableRange::normalized(1, 0, 1, 1)
    );

    let column_selection = runtime
        .table_column_selection_range(10, 1)
        .expect("column selection");
    assert_eq!(
        column_selection,
        cditor_core::rich_text::TableRange::normalized(0, 1, 1, 1)
    );

    let cell_selection = runtime
        .table_cell_selection_range(10, 1, 1)
        .expect("cell selection");
    assert_eq!(
        cell_selection,
        cditor_core::rich_text::TableRange::normalized(1, 1, 1, 1)
    );
    assert!(runtime.table_row_selection_range(10, 2).is_none());
    assert!(runtime.table_column_selection_range(10, 2).is_none());
    assert!(runtime.table_cell_selection_range(10, 2, 0).is_none());
    assert!(runtime.table_row_selection_range(11, 1).is_none());
}

#[test]
fn table_range_and_whole_table_selection_are_bounds_checked() {
    let runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    assert_eq!(
        runtime
            .table_range_selection_range(
                10,
                cditor_core::rich_text::TableRange::normalized(1, 0, 0, 1)
            )
            .expect("range selection"),
        cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1)
    );
    assert_eq!(
        runtime
            .whole_table_selection_range(10)
            .expect("whole table selection"),
        cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1)
    );
    assert!(
        runtime
            .table_range_selection_range(
                10,
                cditor_core::rich_text::TableRange::normalized(0, 0, 2, 1)
            )
            .is_none()
    );
    assert!(runtime.whole_table_selection_range(11).is_none());
}

#[test]
fn table_clipboard_exports_cell_range_row_column_and_whole_table() {
    let runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    let cell = runtime
        .table_clipboard_for_cell(10, 0, 1)
        .expect("cell clipboard");
    assert_eq!(
        cell.range,
        cditor_core::rich_text::TableRange::normalized(0, 1, 0, 1)
    );
    assert_eq!(cell.table.row_count(), 1);
    assert_eq!(cell.table.column_count(), 1);
    assert_eq!(cell.plain_text, "B");
    assert_eq!(cell.markdown, "| B |\n| --- |");

    let range = runtime
        .table_clipboard_for_range(
            10,
            cditor_core::rich_text::TableRange::normalized(1, 1, 0, 0),
        )
        .expect("range clipboard");
    assert_eq!(
        range.range,
        cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1)
    );
    assert_eq!(range.plain_text, "A\tB\nC\tD");

    let row = runtime
        .table_clipboard_for_row(10, 1)
        .expect("row clipboard");
    assert_eq!(
        row.range,
        cditor_core::rich_text::TableRange::normalized(1, 0, 1, 1)
    );
    assert_eq!(row.table.row_count(), 1);
    assert_eq!(row.table.column_count(), 2);
    assert_eq!(row.plain_text, "C\tD");

    let column = runtime
        .table_clipboard_for_column(10, 1)
        .expect("column clipboard");
    assert_eq!(
        column.range,
        cditor_core::rich_text::TableRange::normalized(0, 1, 1, 1)
    );
    assert_eq!(column.table.row_count(), 2);
    assert_eq!(column.table.column_count(), 1);
    assert_eq!(column.plain_text, "B\nD");

    let whole = runtime
        .table_clipboard_for_whole_table(10)
        .expect("whole table clipboard");
    assert_eq!(
        whole.range,
        cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1)
    );
    assert_eq!(whole.plain_text, "A\tB\nC\tD");
    assert_eq!(whole.markdown, "| A | B |\n| --- | --- |\n| C | D |");
    assert!(runtime.table_clipboard_for_cell(10, 3, 0).is_none());
    assert!(runtime.table_clipboard_for_whole_table(99).is_none());
}

#[test]
fn table_clipboard_preserves_merge_align_and_track_sizes_for_internal_snapshot() {
    let mut payload = sample_table_payload();
    let BlockPayload::Table(table) = &mut payload.payload else {
        panic!("expected table payload");
    };
    table.header_style.row_background_color = Some("header-row".to_owned());
    table.header_style.column_background_color = Some("header-column".to_owned());
    table.rows[0].cells[0].style.background_color = Some("cell-bg".to_owned());
    let mut runtime = DocumentRuntime::from_payloads(1, vec![payload], 720.0);
    runtime
        .set_table_column_width(10, 0, TableTrackSize::Px(180))
        .unwrap();
    runtime
        .set_table_row_height(10, 0, TableTrackSize::Px(64))
        .unwrap();
    runtime
        .set_table_cell_align(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
            cditor_core::rich_text::TableCellAlign::Center,
        )
        .unwrap();
    runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    let snapshot = runtime
        .table_clipboard_for_whole_table(10)
        .expect("whole merged table clipboard");
    assert_eq!(snapshot.table.columns[0].width, TableTrackSize::Px(180));
    assert_eq!(snapshot.table.rows[0].height, TableTrackSize::Px(64));
    assert_eq!(
        snapshot.table.rows[0].cells[0].merge,
        TableCellMerge::Origin {
            row_span: 2,
            col_span: 2,
        }
    );
    assert_eq!(
        snapshot.table.rows[1].cells[1].merge,
        TableCellMerge::Covered {
            origin_row: 0,
            origin_col: 0,
        }
    );
    assert_eq!(
        snapshot.table.rows[0].cells[0].align,
        TableCellAlign::Center
    );
    assert_eq!(
        snapshot.table.header_style.row_background_color.as_deref(),
        Some("header-row")
    );
    assert_eq!(
        snapshot
            .table
            .header_style
            .column_background_color
            .as_deref(),
        Some("header-column")
    );
    assert_eq!(
        snapshot.table.rows[0].cells[0]
            .style
            .background_color
            .as_deref(),
        Some("cell-bg")
    );
    assert!(snapshot.plain_text.contains("A\tB\nC\tD"));
    assert!(snapshot.markdown.contains("A\tB<br>C\tD"));

    let partial = runtime
        .table_clipboard_for_column(10, 1)
        .expect("partial merged column clipboard");
    assert_eq!(
        partial.table.rows[0].cells[0].merge,
        TableCellMerge::Unmerged,
        "partial selections must not keep dangling merge metadata"
    );
    assert_eq!(partial.table.rows[0].cells[0].align, TableCellAlign::Center);
}

#[test]
fn paste_table_clipboard_at_focused_cell_expands_table_and_supports_undo_redo() {
    let source_runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    let snapshot = source_runtime
        .table_clipboard_for_whole_table(10)
        .expect("source table clipboard");
    let mut target = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(20, RichBlockKind::Table, "")],
        720.0,
    );
    target.focus_table_cell_at_offset(20, 1, 1, 0).unwrap();

    assert!(
        target
            .paste_table_clipboard_at_focused_cell(&snapshot)
            .unwrap()
    );

    let payload = target.block_payload_record(20).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(payload.content_version, 2);
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some("A"));
    assert_eq!(table.cell_plain_text(1, 2).as_deref(), Some("B"));
    assert_eq!(table.cell_plain_text(2, 1).as_deref(), Some("C"));
    assert_eq!(target.focused_table_cell_offset(), Some((20, 1, 1, 0)));

    assert!(target.undo_focused_block().unwrap());
    let payload = target.block_payload_record(20).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.column_count(), 2);
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some(""));

    assert!(target.redo_focused_block().unwrap());
    let payload = target.block_payload_record(20).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.cell_plain_text(2, 2).as_deref(), Some("D"));
}

#[test]
fn paste_delimited_table_text_at_focused_cell_supports_tsv_csv_and_expansion() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(20, RichBlockKind::Table, "")],
        720.0,
    );
    runtime.focus_table_cell_at_offset(20, 0, 1, 0).unwrap();

    assert!(
        runtime
            .paste_delimited_table_text_at_focused_cell("A\tB\nC\tD")
            .unwrap()
    );

    let payload = runtime.block_payload_record(20).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some("A"));
    assert_eq!(table.cell_plain_text(1, 2).as_deref(), Some("D"));

    runtime.focus_table_cell_at_offset(20, 0, 0, 0).unwrap();
    assert!(
        runtime
            .paste_delimited_table_text_at_focused_cell("\"x,y\",z")
            .unwrap()
    );
    let payload = runtime.block_payload_record(20).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("x,y"));
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some("z"));
    assert!(
        !runtime
            .paste_delimited_table_text_at_focused_cell("plain")
            .unwrap()
    );
}

#[test]
fn table_track_resize_updates_payload_projection_and_content_version() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    assert!(
        runtime
            .set_table_column_width(10, 1, TableTrackSize::Px(180))
            .unwrap()
    );
    assert!(
        runtime
            .set_table_row_height(10, 1, TableTrackSize::Px(56))
            .unwrap()
    );
    assert!(
        !runtime
            .set_table_row_height(10, 1, TableTrackSize::Px(56))
            .unwrap()
    );

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(payload.content_version, 3);
    assert_eq!(table.columns[1].width, TableTrackSize::Px(180));
    assert_eq!(table.rows[1].height, TableTrackSize::Px(56));

    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    assert_eq!(table_view.width_px, 300.0);
    assert_eq!(table_view.height_px, 92.0);
    let resized_cell = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 1, col: 1 }))
        .expect("resized cell");
    assert_eq!(resized_cell.x_px, 120.0);
    assert_eq!(resized_cell.y_px, 36.0);
    assert_eq!(resized_cell.width_px, 180.0);
    assert_eq!(resized_cell.height_px, 56.0);
}

#[test]
fn table_resize_supports_undo_and_redo() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime.focus_block(10);

    assert!(
        runtime
            .set_table_column_width(10, 0, TableTrackSize::Px(180))
            .unwrap()
    );
    assert!(
        runtime
            .set_table_row_height(10, 0, TableTrackSize::Px(64))
            .unwrap()
    );

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.columns[0].width, TableTrackSize::Px(180));
    assert_eq!(table.rows[0].height, TableTrackSize::Px(64));
    assert_eq!(
        runtime.projection_for_window().blocks[0]
            .table_view
            .as_ref()
            .expect("table projection")
            .height_px,
        100.0
    );

    assert!(runtime.undo_focused_block().unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.columns[0].width, TableTrackSize::Px(180));
    assert_eq!(table.rows[0].height, TableTrackSize::Auto);
    assert_eq!(
        runtime.projection_for_window().blocks[0]
            .table_view
            .as_ref()
            .expect("table projection")
            .height_px,
        72.0
    );

    assert!(runtime.undo_focused_block().unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.columns[0].width, TableTrackSize::Auto);
    assert_eq!(table.rows[0].height, TableTrackSize::Auto);
    assert_eq!(
        runtime.projection_for_window().blocks[0]
            .table_view
            .as_ref()
            .expect("table projection")
            .height_px,
        72.0
    );

    assert!(runtime.redo_focused_block().unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.columns[0].width, TableTrackSize::Px(180));
    assert_eq!(table.rows[0].height, TableTrackSize::Auto);
}

#[test]
fn table_column_width_change_recomputes_auto_row_height_from_wrapping() {
    let mut payload = sample_table_payload();
    let BlockPayload::Table(table) = &mut payload.payload else {
        panic!("payload should be table");
    };
    table.set_cell_plain_text(0, 0, "abcdefghijklmnopqrstuvwxyz".repeat(8));
    let mut runtime = DocumentRuntime::from_payloads(1, vec![payload], 720.0);

    let before_projection = runtime.projection_for_window();
    let before_table = before_projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    let before_height = before_table
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 0, col: 0 }))
        .expect("first cell")
        .height_px;

    assert!(
        runtime
            .set_table_column_width(10, 0, TableTrackSize::Px(360))
            .unwrap()
    );

    let after_projection = runtime.projection_for_window();
    let after_table = after_projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    let after_height = after_table
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 0, col: 0 }))
        .expect("first cell")
        .height_px;

    assert!(
        after_height < before_height,
        "before_height={before_height} after_height={after_height}"
    );
    assert_eq!(
        after_projection.blocks[0].layout.effective_height() as f32,
        after_table.height_px
    );
}

#[test]
fn table_row_and_column_structure_edits_update_payload_projection_and_focus() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell_at_offset(10, 1, 1, 1).unwrap();
    assert!(runtime.insert_table_row(10, 1).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 2, 1, 1)));
    assert!(runtime.insert_table_column(10, 1).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 2, 2, 1)));

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(payload.content_version, 3);
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("A"));
    assert_eq!(table.cell_plain_text(1, 0).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(2, 2).as_deref(), Some("D"));

    assert!(runtime.move_table_row(10, 2, 0).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 2, 1)));
    assert!(runtime.move_table_column(10, 2, 0).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 0, 1)));
    assert!(runtime.delete_table_row(10, 1).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 0, 1)));
    assert!(runtime.delete_table_column(10, 1).unwrap());
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 0, 1)));

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(payload.content_version, 7);
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.column_count(), 2);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("D"));
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(1, 0).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some(""));

    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    assert_eq!(table_view.row_count, 2);
    assert_eq!(table_view.col_count, 2);
    assert_eq!(table_view.visible_cells.len(), 4);
}

#[test]
fn table_duplicate_row_and_column_update_payload_and_projection() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    assert!(runtime.duplicate_table_row(10, 0).unwrap());
    assert!(runtime.duplicate_table_column(10, 0).unwrap());

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(payload.content_version, 3);
    assert_eq!(table.row_count(), 3);
    assert_eq!(table.column_count(), 3);
    assert_eq!(table.cell_plain_text(1, 0).as_deref(), Some("A"));
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some("A"));

    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    assert_eq!(table_view.row_count, 3);
    assert_eq!(table_view.col_count, 3);
    assert_eq!(table_view.visible_cells.len(), 9);
}

#[test]
fn table_cell_background_color_is_projected_from_payload_style() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    assert!(
        runtime
            .set_table_cell_background_color(
                10,
                TableRange::normalized(0, 0, 0, 1),
                Some("action_background".to_owned()),
            )
            .unwrap()
    );

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(
        table.rows[0].cells[1].style.background_color.as_deref(),
        Some("action_background")
    );

    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    assert_eq!(
        table_view.visible_cells[0].background_color.as_deref(),
        Some("action_background")
    );
}

#[test]
fn table_structure_edits_reject_merged_tables_without_losing_runtime_payload() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    assert!(runtime.insert_table_row(10, 1).is_err());
    assert!(runtime.delete_table_row(10, 1).is_err());
    assert!(runtime.move_table_row(10, 0, 1).is_err());
    assert!(runtime.insert_table_column(10, 1).is_err());
    assert!(runtime.delete_table_column(10, 1).is_err());
    assert!(runtime.move_table_column(10, 0, 1).is_err());

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(payload.content_version, 2);
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.column_count(), 2);
    assert_eq!(table.visible_cells().count(), 1);
}

#[test]
fn table_row_and_column_reorder_support_undo_and_redo() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime.focus_block(10);

    assert!(runtime.move_table_row(10, 0, 1).unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("C"));

    assert!(runtime.undo_focused_block().unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("A"));

    assert!(runtime.redo_focused_block().unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("C"));

    assert!(runtime.move_table_column(10, 0, 1).unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("D"));

    assert!(runtime.undo_focused_block().unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("C"));
}

#[test]
fn table_auto_row_height_grows_for_multiline_cell_text() {
    let mut payload = sample_table_payload();
    let BlockPayload::Table(table) = &mut payload.payload else {
        panic!("payload should be table");
    };
    table.set_cell_plain_text(0, 0, "埃塞\n埃塞\n埃塞\n埃塞");

    let runtime = DocumentRuntime::from_payloads(1, vec![payload], 720.0);
    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    let first_cell = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 0, col: 0 }))
        .expect("first cell");
    let second_row_cell = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 1, col: 0 }))
        .expect("second row cell");

    assert!(
        first_cell.height_px > 72.0,
        "height={}",
        first_cell.height_px
    );
    assert_eq!(first_cell.y_px, 0.0);
    assert_eq!(second_row_cell.y_px, first_cell.height_px);
    assert_eq!(
        table_view.height_px,
        first_cell.height_px + second_row_cell.height_px
    );
}

#[test]
fn table_cell_enter_updates_block_height_and_pushes_following_blocks_down() {
    let table = sample_table_payload();
    let paragraph = BlockPayloadRecord::rich_text(11, RichBlockKind::Paragraph, "below");
    let mut runtime = DocumentRuntime::from_payloads(1, vec![table, paragraph], 720.0);
    let before_table_height = runtime.index.layout_meta[0].effective_height();
    let before_second_offset = runtime.height_index.offset_of_block(1).unwrap();

    runtime.focus_table_cell_at_offset(10, 0, 0, 1).unwrap();
    for _ in 0..6 {
        runtime.handle_enter().unwrap();
    }

    let projection = runtime.projection_for_window();
    let table_block = projection
        .blocks
        .iter()
        .find(|block| block.block_id == 10)
        .expect("table block");
    let table_view = table_block.table_view.as_ref().expect("table projection");
    let after_table_height = table_block.layout.effective_height();
    let after_second_offset = runtime.height_index.offset_of_block(1).unwrap();

    assert!(table_view.height_px > before_table_height as f32);
    assert!(after_table_height > before_table_height);
    assert_eq!(after_second_offset, after_table_height);
    assert!(after_second_offset > before_second_offset);
}

#[test]
fn table_height_change_above_viewport_restores_viewport_anchor() {
    let table = sample_table_payload();
    let mut payloads = vec![table];
    payloads.extend((11..160).map(|block_id| {
        BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, "below")
    }));
    let mut runtime = DocumentRuntime::from_payloads(1, payloads, 720.0);
    runtime
        .scroll
        .scroll_to_global_offset(1_200.0, cditor_editor::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let before_scroll_top = runtime.scroll.global_scroll_top;
    let before_table_height = runtime.height_index.heights[0];

    runtime.focus_table_cell_at_offset(10, 0, 0, 1).unwrap();
    for _ in 0..6 {
        runtime.handle_enter().unwrap();
    }

    let after_table_height = runtime.height_index.heights[0];
    let height_delta = after_table_height - before_table_height;
    assert!(height_delta > 0.0);
    assert_eq!(
        runtime.scroll.global_scroll_top,
        before_scroll_top + height_delta
    );
}

#[test]
fn table_height_change_during_scrollbar_drag_defers_displayed_total_update() {
    let table = sample_table_payload();
    let mut payloads = vec![table];
    payloads.extend((11..80).map(|block_id| {
        BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, "below")
    }));
    let mut runtime = DocumentRuntime::from_payloads(1, payloads, 720.0);
    let policy = ScrollbarPolicy {
        track_height: 720.0,
        ..ScrollbarPolicy::default()
    };
    let before_displayed_total = runtime.scroll.displayed_total_height;
    let before_model_total = runtime.scroll.model_total_height;

    let visual = runtime.begin_scrollbar_drag(policy);
    assert!(visual.enabled);
    runtime.focus_table_cell_at_offset(10, 0, 0, 1).unwrap();
    for _ in 0..6 {
        runtime.handle_enter().unwrap();
    }

    assert!(runtime.scroll.model_total_height > before_model_total);
    assert_eq!(
        runtime.scroll.displayed_total_height,
        before_displayed_total
    );
    let end = runtime.finish_scrollbar_drag().unwrap().unwrap();
    assert!(end.pending_layout_corrections > 0);
    assert_eq!(
        runtime.scroll.displayed_total_height,
        runtime.scroll.model_total_height
    );
}

#[test]
fn table_manual_row_height_is_not_overridden_by_multiline_text() {
    let mut payload = sample_table_payload();
    let BlockPayload::Table(table) = &mut payload.payload else {
        panic!("payload should be table");
    };
    table.rows[0].height = cditor_core::rich_text::TableTrackSize::Px(40);
    table.set_cell_plain_text(0, 0, "埃塞\n埃塞\n埃塞\n埃塞");

    let runtime = DocumentRuntime::from_payloads(1, vec![payload], 720.0);
    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    let first_cell = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 0, col: 0 }))
        .expect("first cell");

    assert_eq!(first_cell.height_px, 40.0);
}

#[test]
fn split_table_cell_restores_covered_cells_and_bumps_content_version() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    assert!(runtime.split_table_cell(10, 1, 1).unwrap());

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(payload.content_version, 3);
    assert_eq!(
        table.rows[0].cells[0].merge,
        cditor_core::rich_text::TableCellMerge::Unmerged
    );
    assert_eq!(
        table.rows[1].cells[1].merge,
        cditor_core::rich_text::TableCellMerge::Unmerged
    );
    assert_eq!(table.cell_plain_text(0, 0), Some("A\tB\nC\tD".to_owned()));
    assert_eq!(table.cell_plain_text(0, 1), Some(String::new()));
    assert_eq!(table.cell_plain_text(1, 0), Some(String::new()));
    assert_eq!(table.cell_plain_text(1, 1), Some(String::new()));
}

#[test]
fn merge_and_split_table_cells_support_undo_and_redo() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime.focus_block(10);

    assert!(
        runtime
            .merge_table_cells(
                10,
                cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1)
            )
            .unwrap()
    );
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.visible_cells().count(), 1);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("A\tB\nC\tD"));

    assert!(runtime.undo_focused_block().unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.visible_cells().count(), 4);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("A"));
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some("D"));

    assert!(runtime.redo_focused_block().unwrap());
    assert!(runtime.split_table_cell(10, 0, 0).unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.visible_cells().count(), 4);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("A\tB\nC\tD"));
    assert_eq!(table.cell_plain_text(1, 1).as_deref(), Some(""));

    assert!(runtime.undo_focused_block().unwrap());
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.visible_cells().count(), 1);
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("A\tB\nC\tD"));
}

#[test]
fn merged_table_cell_geometry_updates_after_row_and_column_resize() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    assert!(
        runtime
            .set_table_column_width(10, 0, TableTrackSize::Px(180))
            .unwrap()
    );
    assert!(
        runtime
            .set_table_row_height(10, 1, TableTrackSize::Px(64))
            .unwrap()
    );

    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    let merged = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 0, col: 0 }))
        .expect("merged cell");
    assert_eq!(table_view.visible_cells.len(), 1);
    assert_eq!(merged.width_px, 300.0);
    assert_eq!(merged.height_px, 100.0);
}

#[test]
fn merged_table_metadata_remaps_after_runtime_row_and_column_reorder() {
    let mut row_runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    assert!(row_runtime.insert_table_row(10, 2).unwrap());
    row_runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    assert!(row_runtime.move_table_row(10, 2, 0).unwrap());

    let payload = row_runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(
        table.rows[1].cells[0].merge,
        cditor_core::rich_text::TableCellMerge::Origin {
            row_span: 2,
            col_span: 2
        }
    );
    assert_eq!(
        table.rows[2].cells[1].merge,
        cditor_core::rich_text::TableCellMerge::Covered {
            origin_row: 1,
            origin_col: 0
        }
    );

    let mut column_runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    assert!(column_runtime.insert_table_column(10, 2).unwrap());
    column_runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    assert!(column_runtime.move_table_column(10, 2, 0).unwrap());

    let payload = column_runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(
        table.rows[0].cells[1].merge,
        cditor_core::rich_text::TableCellMerge::Origin {
            row_span: 2,
            col_span: 2
        }
    );
    assert_eq!(
        table.rows[1].cells[2].merge,
        cditor_core::rich_text::TableCellMerge::Covered {
            origin_row: 0,
            origin_col: 1
        }
    );
}

#[test]
fn runtime_rejects_reorder_that_would_split_merged_table_cell() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime
        .merge_table_cells(
            10,
            cditor_core::rich_text::TableRange::normalized(0, 0, 1, 1),
        )
        .unwrap();

    assert!(runtime.move_table_row(10, 0, 1).is_err());
    assert!(runtime.move_table_column(10, 0, 1).is_err());
}

#[test]
fn set_table_cell_align_updates_selected_range() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    assert!(
        runtime
            .set_table_cell_align(
                10,
                cditor_core::rich_text::TableRange::normalized(0, 1, 1, 1),
                cditor_core::rich_text::TableCellAlign::Right,
            )
            .unwrap()
    );

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(
        table.rows[0].cells[1].align,
        cditor_core::rich_text::TableCellAlign::Right
    );
    assert_eq!(
        table.rows[1].cells[1].align,
        cditor_core::rich_text::TableCellAlign::Right
    );
    assert_eq!(
        table.rows[1].cells[0].align,
        cditor_core::rich_text::TableCellAlign::Left
    );
    let projection = runtime.projection_for_window();
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table projection");
    let right_aligned = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 1, col: 1 }))
        .expect("right aligned cell");
    assert_eq!(
        right_aligned.align,
        cditor_core::rich_text::TableCellAlign::Right
    );
    let left_aligned = table_view
        .visible_cells
        .iter()
        .find(|cell| cell.position == (TableCellPosition { row: 1, col: 0 }))
        .expect("left aligned cell");
    assert_eq!(
        left_aligned.align,
        cditor_core::rich_text::TableCellAlign::Left
    );
}

#[test]
fn table_cell_align_supports_undo_and_redo() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    runtime.focus_block(10);
    runtime.focus_table_cell_at_offset(10, 1, 0, 1).unwrap();
    if let Some(cell) = runtime.focused_table_cell.as_mut() {
        *cell = cell.with_selected_range(0..1, true);
    }

    assert!(
        runtime
            .set_table_cell_align(
                10,
                runtime.table_row_selection_range(10, 1).expect("row range"),
                cditor_core::rich_text::TableCellAlign::Center,
            )
            .unwrap()
    );
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(
        table.rows[1].cells[0].align,
        cditor_core::rich_text::TableCellAlign::Center
    );
    assert_eq!(
        table.rows[1].cells[1].align,
        cditor_core::rich_text::TableCellAlign::Center
    );

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(
        runtime.focused_table_cell_selection_state(),
        Some((10, 1, 0, 0..1, true, None))
    );
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(
        table.rows[1].cells[0].align,
        cditor_core::rich_text::TableCellAlign::Left
    );

    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(
        runtime.focused_table_cell_selection_state(),
        Some((10, 1, 0, 0..1, true, None))
    );
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(
        table.rows[1].cells[0].align,
        cditor_core::rich_text::TableCellAlign::Center
    );
}

#[test]
fn table_kind_with_non_table_payload_is_normalized_on_load() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 10,
            content_version: 7,
            kind: RichBlockKind::Table,
            payload: BlockPayload::RichText {
                spans: vec![InlineSpan::plain("legacy")],
            },
        }],
        720.0,
    );

    assert!(!runtime.text_models.contains_key(&10));
    runtime.focus_block(10);
    runtime.focus_block(10);
    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("expected loaded payload");
    };
    let BlockPayload::Table(table) = &payload.payload else {
        panic!("expected normalized table payload");
    };
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells.len(), 2);
    assert_eq!(table.cell_plain_text(0, 0), Some("legacy".to_owned()));
}

#[test]
fn table_kind_with_empty_table_payload_gets_default_cells() {
    let runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord {
            block_id: 10,
            content_version: 1,
            kind: RichBlockKind::Table,
            payload: BlockPayload::Table(cditor_core::rich_text::TablePayload::default()),
        }],
        720.0,
    );

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0].cells.len(), 2);
}

#[test]
fn table_runtime_survives_stale_empty_payload_snapshot() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell(10, 0, 1).unwrap();
    runtime.insert_char('!').unwrap();
    let payload = runtime.payload_window.payloads.get_mut(&10).unwrap();
    payload.payload = BlockPayload::Table(cditor_core::rich_text::TablePayload::default());
    payload.content_version = payload.content_version.saturating_add(1);

    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload from runtime");
    };
    assert_eq!(table.cell_plain_text(0, 0), Some("A".to_owned()));
    assert_eq!(table.cell_plain_text(0, 1), Some("B!".to_owned()));

    let snapshot = runtime.loaded_payload_records_snapshot();
    let BlockPayload::Table(table) = &snapshot
        .iter()
        .find(|payload| payload.block_id == 10)
        .expect("table payload snapshot")
        .payload
    else {
        panic!("expected saved table payload from runtime");
    };
    assert_eq!(table.cell_plain_text(0, 0), Some("A".to_owned()));
    assert_eq!(table.cell_plain_text(0, 1), Some("B!".to_owned()));

    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("expected loaded payload");
    };
    let BlockPayload::Table(table) = &payload.payload else {
        panic!("expected projected table payload from runtime");
    };
    assert_eq!(table.cell_plain_text(0, 0), Some("A".to_owned()));
    assert_eq!(table.cell_plain_text(0, 1), Some("B!".to_owned()));
    let table_view = projection.blocks[0]
        .table_view
        .as_ref()
        .expect("table view state");
    assert_eq!(table_view.table.cell_plain_text(0, 0), Some("A".to_owned()));
    assert_eq!(
        table_view.table.cell_plain_text(0, 1),
        Some("B!".to_owned())
    );
}

#[test]
fn converting_to_table_removes_plain_text_model() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "/table")]);

    runtime.focus_block_at_offset(1, "/table".len()).unwrap();
    assert!(runtime.text_models.contains_key(&1));
    assert!(
        runtime
            .replace_text_in_focused_range(Some(0.."/table".len()), "")
            .unwrap()
    );
    assert!(
        runtime
            .convert_focused_block_kind(RichBlockKind::Table)
            .unwrap()
    );

    assert!(!runtime.text_models.contains_key(&1));
    assert_eq!(runtime.focused_text(), None);
    assert!(matches!(
        runtime.block_payload_record(1).unwrap().payload,
        BlockPayload::Table(_)
    ));
}

#[test]
fn editing_table_cell_does_not_create_block_text_model() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell(10, 0, 1).unwrap();
    runtime.insert_char('!').unwrap();

    assert!(!runtime.text_models.contains_key(&10));
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 1), Some("B!".to_owned()));
}

#[test]
fn enter_on_table_block_without_cell_focus_preserves_table_payload() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_block(10);
    runtime.handle_enter().unwrap();

    assert_eq!(runtime.index.block_ids, vec![10]);
    assert!(!runtime.text_models.contains_key(&10));
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0), Some("A".to_owned()));
    assert_eq!(table.cell_plain_text(0, 1), Some("B".to_owned()));
}

#[test]
fn splitting_table_payload_never_exports_cell_text_to_new_paragraph() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_block(10);
    runtime
        .split_focused_block_at_caret(EnterSplitMode::ForceParagraph)
        .unwrap();

    assert_eq!(runtime.index.block_ids, vec![10, 11]);
    let current = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = current.payload else {
        panic!("expected original table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0), Some("A".to_owned()));
    assert_eq!(table.cell_plain_text(0, 1), Some("B".to_owned()));
    assert_eq!(runtime.block_payload_record(11).unwrap().plain_text(), "");
}

#[test]
fn enter_in_focused_table_cell_inserts_newline_inside_cell() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_table_cell(10, 0, 1).unwrap();
    runtime.handle_enter().unwrap();

    assert_eq!(runtime.index.block_ids, vec![10]);
    assert!(!runtime.text_models.contains_key(&10));
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 1), Some("B\n".to_owned()));
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 1, 2)));
}

#[test]
fn backspace_and_delete_in_table_cell_do_not_delete_table_block() {
    let paragraph = BlockPayloadRecord::rich_text(11, RichBlockKind::Paragraph, "below");
    let mut runtime =
        DocumentRuntime::from_payloads(1, vec![sample_table_payload(), paragraph], 720.0);

    runtime.focus_table_cell_at_offset(10, 0, 0, 0).unwrap();
    assert!(!runtime.delete_backward().unwrap());
    assert_eq!(runtime.index.block_ids, vec![10, 11]);
    assert_eq!(runtime.focused_table_cell_offset(), Some((10, 0, 0, 0)));

    assert!(runtime.delete_forward().unwrap());
    assert_eq!(runtime.index.block_ids, vec![10, 11]);
    let payload = runtime.block_payload_record(10).unwrap();
    let BlockPayload::Table(table) = payload.payload else {
        panic!("expected table payload");
    };
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some(""));
    assert_eq!(
        runtime.block_payload_record(11).unwrap().plain_text(),
        "below"
    );
}

#[test]
fn table_composition_preview_without_cell_focus_keeps_projection_payload_table() {
    let mut runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);

    runtime.focus_block(10);
    runtime
        .editing
        .as_mut()
        .unwrap()
        .update_composition(CompositionState {
            block_id: 10,
            range_start: 0,
            range_end: 0,
            preview_text: "中".to_owned(),
            selected_range_start: None,
            selected_range_end: None,
        })
        .unwrap();

    let payload = runtime.block_payload_record(10).unwrap();
    assert!(matches!(payload.payload, BlockPayload::Table(_)));
    let projection = runtime.projection_for_window();
    let BlockPayloadView::Loaded(payload) = &projection.blocks[0].payload else {
        panic!("expected loaded payload");
    };
    assert!(matches!(payload.payload, BlockPayload::Table(_)));
}

async fn postgres_runtime_fixture(
    document_id: u64,
) -> (
    cditor_storage_postgres::PostgresDocumentStore,
    cditor_storage_postgres::PostgresPayloadStore,
    cditor_storage_postgres::PostgresLayoutCacheStore,
    cditor_storage_postgres::DocumentRow,
    BlockId,
) {
    use cditor_storage_postgres::{
        DocumentRow, PostgresDocumentStore, PostgresLayoutCacheStore, PostgresPayloadStore,
        PostgresPoolConfig, create_pg_pool, pg_document_id_from_runtime, run_migrations,
    };
    use sqlx::types::Uuid;

    let database_url = std::env::var("CDITOR_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://cditor:cditor@localhost:5433/cditor_test".to_owned());
    let pool = create_pg_pool(&PostgresPoolConfig::for_tests(database_url))
        .await
        .unwrap();
    run_migrations(&pool).await.unwrap();
    let document_store = PostgresDocumentStore::new(pool.clone());
    let payload_store = PostgresPayloadStore::new(pool.clone());
    let layout_store = PostgresLayoutCacheStore::new(pool);
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u64;
    let runtime_document_id = document_id + suffix;
    let document = DocumentRow {
        id: pg_document_id_from_runtime(runtime_document_id),
        workspace_id: Uuid::from_u128(
            0x9500_0000_0000_0000_0000_0000_0000_0000 | runtime_document_id as u128,
        ),
        title: format!("Runtime Store {runtime_document_id}"),
        structure_version: 1,
        content_version: 1,
        layout_version: 0,
        schema_version: 1,
    };
    document_store
        .save_document_metadata(&document)
        .await
        .unwrap();
    let base_block_id = runtime_document_id * 10;
    (
        document_store,
        payload_store,
        layout_store,
        document,
        base_block_id,
    )
}

fn sample_index_records(base_block_id: BlockId, count: usize) -> Vec<BlockIndexRecord> {
    (0..count)
        .map(|index| {
            BlockIndexRecord::new(
                base_block_id + index as u64,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
            .with_layout_meta(BlockLayoutMeta::new(base_block_id + index as u64, 32.0))
        })
        .collect()
}

fn sample_payloads(base_block_id: BlockId, count: usize) -> Vec<BlockPayloadRecord> {
    (0..count)
        .map(|index| {
            BlockPayloadRecord::rich_text(
                base_block_id + index as u64,
                RichBlockKind::Paragraph,
                format!("payload {index}"),
            )
        })
        .collect()
}

fn runtime_store_layout_key() -> LayoutCacheKey {
    LayoutCacheKey {
        width_bucket: 10,
        exact_width_px: 800,
        content_version: 1,
        attrs_version: 0,
        style_version: 0,
        font_version: 0,
        theme_version: 0,
        scale_factor_milli: 1000,
    }
}
