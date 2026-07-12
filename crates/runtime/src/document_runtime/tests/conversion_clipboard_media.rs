use super::*;

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
fn convert_focused_block_kind_to_table_creates_default_3_by_3_grid() {
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
    assert_eq!(table.rows.len(), 3);
    assert!(table.rows.iter().all(|row| row.cells.len() == 3));
    assert_eq!(table.cell_plain_text(0, 0).as_deref(), Some("hello"));
    assert_eq!(table.cell_plain_text(0, 1).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(0, 2).as_deref(), Some(""));
    assert_eq!(table.cell_plain_text(2, 2).as_deref(), Some(""));
    let projection = runtime.projection_for_window();
    // With Auto columns, the table width should equal available width
    // The exact width depends on layout calculation, but should be less than the old fixed 812
    let table_view = projection.blocks[0].table_view.as_ref().unwrap();
    assert_eq!(table_view.row_count, 3);
    assert_eq!(table_view.col_count, 3);
    assert!(
        table_view.width_px < 812.0,
        "Auto-width table should not exceed old fixed width, got: {}",
        table_view.width_px
    );
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
fn convert_focused_block_kind_to_whiteboard_creates_scene_payload() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "/whiteboard")]);
    runtime.focus_block(1);

    assert!(
        runtime
            .convert_focused_block_kind(RichBlockKind::Whiteboard)
            .unwrap()
    );

    let payload = runtime.block_payload_record(1).unwrap();
    assert_eq!(payload.kind, RichBlockKind::Whiteboard);
    assert!(matches!(
        payload.payload,
        BlockPayload::Whiteboard(cditor_core::rich_text::WhiteboardPayload { ref scene_json })
            if scene_json.is_empty()
    ));
    assert!(
        runtime.projection_for_window().blocks[0]
            .layout
            .effective_height()
            >= 240.0
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
fn selected_focused_rich_text_keeps_inline_marks_for_structured_clipboard() {
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
fn whiteboard_scene_updates_payload_version_without_changing_block_kind() {
    let mut runtime = runtime_with_single_payload(
        RichBlockKind::Whiteboard,
        BlockPayload::Whiteboard(cditor_core::rich_text::WhiteboardPayload {
            scene_json: "{}".to_owned(),
        }),
    );

    assert!(
        runtime
            .update_whiteboard_scene_json(1, r#"{"elements":[]}"#)
            .unwrap()
    );
    let payload = runtime.block_payload_record(1).unwrap();
    assert_eq!(payload.kind, RichBlockKind::Whiteboard);
    assert_eq!(payload.content_version, 2);
    assert!(matches!(
        payload.payload,
        BlockPayload::Whiteboard(cditor_core::rich_text::WhiteboardPayload { ref scene_json })
            if scene_json == r#"{"elements":[]}"#
    ));
    assert!(
        !runtime
            .update_whiteboard_scene_json(1, r#"{"elements":[]}"#)
            .unwrap()
    );
}
