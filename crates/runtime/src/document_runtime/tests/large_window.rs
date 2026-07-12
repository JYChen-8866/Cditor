use super::*;

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
