use super::*;

#[test]
fn large_markdown_paste_hydrates_only_focused_and_viewport_text_models() {
    let mut runtime = DocumentRuntime::empty();
    runtime.focus_block(1);
    let markdown = (0..2_000)
        .map(|index| format!("- item {index}"))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(runtime.insert_markdown_paste(&markdown).unwrap());
    let projection = runtime.projection_for_window_planned();

    assert_eq!(runtime.index.total_count(), 2_000);
    assert!(projection.blocks.len() <= 320);
    assert!(runtime.text_models.len() <= 322);
    assert!(runtime.text_models.len() < runtime.index.total_count());
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
fn markdown_paste_parses_inline_only_formatting() {
    let mut runtime =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "")]);
    runtime.focus_block_at_offset(1, 0).unwrap();

    assert!(
        runtime
            .insert_markdown_paste("**bold** and `code`")
            .unwrap()
    );

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
