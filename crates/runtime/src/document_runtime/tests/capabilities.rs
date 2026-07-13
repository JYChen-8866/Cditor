use super::*;

#[test]
fn rich_text_menu_actions_require_non_empty_rich_text_payload() {
    let rich = runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "text")]);
    assert!(rich.supports_block_rich_text_actions(1));

    let empty = runtime_with_paragraph_blocks(1);
    assert!(!empty.supports_block_rich_text_actions(1));

    let table = runtime_with_single_payload(RichBlockKind::Table, sample_table_payload().payload);
    assert!(!table.supports_block_rich_text_actions(1));
}

#[test]
fn block_conversion_capability_rejects_asset_flattening_but_keeps_text_exports() {
    let rich = runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "text")]);
    assert!(rich.can_convert_block_kind(1, &RichBlockKind::Heading { level: 2 }));
    assert!(!rich.can_convert_block_kind(1, &RichBlockKind::Paragraph));

    let table = runtime_with_single_payload(RichBlockKind::Table, sample_table_payload().payload);
    assert!(table.can_convert_block_kind(1, &RichBlockKind::Paragraph));

    let image = runtime_with_single_payload(
        RichBlockKind::Image,
        BlockPayload::Image(cditor_core::rich_text::ImagePayload {
            source: "asset.png".to_owned(),
            alt: "asset".to_owned(),
            ..Default::default()
        }),
    );
    assert!(!image.can_convert_block_kind(1, &RichBlockKind::Paragraph));
}

#[test]
fn unsupported_asset_conversion_is_a_noop() {
    let mut image = runtime_with_single_payload(
        RichBlockKind::Image,
        BlockPayload::Image(cditor_core::rich_text::ImagePayload {
            source: "asset.png".to_owned(),
            ..Default::default()
        }),
    );
    image.focus_block(1);

    assert!(
        !image
            .convert_focused_block_kind(RichBlockKind::Paragraph)
            .unwrap()
    );
    assert_eq!(
        image.block_payload_record(1).unwrap().kind,
        RichBlockKind::Image
    );
}

#[test]
fn ai_menu_capability_tracks_text_selection_and_complex_blocks() {
    let mut rich =
        runtime_with_kind_depths_and_text(vec![(RichBlockKind::Paragraph, 0, None, "text")]);
    rich.focus_block_at_offset(1, 2).unwrap();
    assert!(rich.can_begin_ai_request());

    let mut table =
        runtime_with_single_payload(RichBlockKind::Table, sample_table_payload().payload);
    table.focus_block(1);
    assert!(!table.can_begin_ai_request());

    let mut selection = runtime_with_kind_depths_and_text(vec![
        (RichBlockKind::Paragraph, 0, None, "first"),
        (RichBlockKind::Paragraph, 0, None, "second"),
    ]);
    selection.set_document_text_selection(1, 1, 2, 3).unwrap();
    assert!(selection.can_begin_ai_request());
}

#[test]
fn delete_menu_capability_matches_subtree_delete_contract() {
    let leaf = runtime_with_paragraph_blocks(2);
    assert!(leaf.can_delete_block(1));
    assert!(leaf.can_delete_block(2));

    let subtree = runtime_with_kind_depths(vec![
        (RichBlockKind::Toggle, 0, None),
        (RichBlockKind::Paragraph, 1, Some(1)),
        (RichBlockKind::Paragraph, 0, None),
    ]);
    assert!(!subtree.can_delete_block(1));
    assert!(subtree.can_delete_block(2));

    let final_block = runtime_with_paragraph_blocks(1);
    assert!(final_block.can_delete_block(1));
    assert!(!final_block.can_delete_block(99));
}
