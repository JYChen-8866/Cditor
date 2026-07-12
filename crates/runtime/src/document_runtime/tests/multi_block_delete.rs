/// Tests for multi-block selection deletion
use super::*;

#[test]
fn test_delete_selected_blocks_with_backspace() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "First"),
            BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "Second"),
            BlockPayloadRecord::rich_text(3, RichBlockKind::Paragraph, "Third"),
        ],
        720.0,
    );

    // Simulate selecting blocks 1 and 2
    runtime.selected_block_ids.insert(1);
    runtime.selected_block_ids.insert(2);

    // Delete with Backspace
    let result = runtime.delete_backward();
    assert!(
        result.is_ok() && result.unwrap(),
        "delete_backward should succeed"
    );

    // Verify blocks 1 and 2 are deleted
    assert!(
        runtime.index.index_of(1).is_none(),
        "Block 1 should be deleted"
    );
    assert!(
        runtime.index.index_of(2).is_none(),
        "Block 2 should be deleted"
    );
    assert!(runtime.index.index_of(3).is_some(), "Block 3 should remain");

    // Verify selection is cleared
    assert!(
        runtime.selected_block_ids.is_empty(),
        "Selection should be cleared"
    );

    // Verify focus moved to remaining block
    assert_eq!(runtime.focused_block_id(), Some(3));
}

#[test]
fn test_delete_selected_blocks_with_delete_key() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "First"),
            BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "Second"),
            BlockPayloadRecord::rich_text(3, RichBlockKind::Paragraph, "Third"),
        ],
        720.0,
    );

    // Simulate selecting block 2
    runtime.selected_block_ids.insert(2);

    // Delete with Delete key (forward delete)
    let result = runtime.delete_forward();
    assert!(
        result.is_ok() && result.unwrap(),
        "delete_forward should succeed"
    );

    // Verify block 2 is deleted
    assert!(runtime.index.index_of(1).is_some(), "Block 1 should remain");
    assert!(
        runtime.index.index_of(2).is_none(),
        "Block 2 should be deleted"
    );
    assert!(runtime.index.index_of(3).is_some(), "Block 3 should remain");

    // Verify selection is cleared
    assert!(runtime.selected_block_ids.is_empty());
}

#[test]
fn test_delete_all_blocks_leaves_one_paragraph() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            "Only block",
        )],
        720.0,
    );

    // Select the only block
    runtime.selected_block_ids.insert(1);

    // Delete it
    let result = runtime.delete_backward();
    assert!(result.is_ok() && result.unwrap());

    // Verify we still have at least one block (empty paragraph)
    assert!(
        !runtime.index.block_ids.is_empty(),
        "Should have at least one block"
    );

    // Verify we have a valid focused block
    assert!(
        runtime.focused_block_id().is_some(),
        "Should have a focused block"
    );
}

#[test]
fn test_delete_selected_blocks_with_mixed_types() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "Paragraph"),
            BlockPayloadRecord::rich_text(2, RichBlockKind::Heading { level: 1 }, "Heading"),
            BlockPayloadRecord::rich_text(3, RichBlockKind::Quote, "Quote"),
            BlockPayloadRecord::rich_text(4, RichBlockKind::Paragraph, "End"),
        ],
        720.0,
    );

    // Select blocks of different types
    runtime.selected_block_ids.insert(2);
    runtime.selected_block_ids.insert(3);

    let result = runtime.delete_backward();
    assert!(result.is_ok() && result.unwrap());

    // Verify correct blocks were deleted
    assert!(runtime.index.index_of(1).is_some());
    assert!(runtime.index.index_of(2).is_none());
    assert!(runtime.index.index_of(3).is_none());
    assert!(runtime.index.index_of(4).is_some());
}
