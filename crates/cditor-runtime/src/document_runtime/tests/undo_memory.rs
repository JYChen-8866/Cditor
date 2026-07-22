use super::*;

#[test]
fn text_undo_history_obeys_byte_budget_and_keeps_events_in_sync() {
    let mut runtime = runtime_with_paragraph_blocks(1);
    let text_bytes = 1024 * 1024;

    for revision in 1..=40 {
        let marker = char::from(b'a' + (revision % 26) as u8);
        runtime.payload_window.insert(BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Paragraph,
            marker.to_string().repeat(text_bytes),
        ));
        runtime.push_undo_snapshot(1).unwrap();
    }

    let snapshot_count = runtime.undo_stacks.values().map(Vec::len).sum::<usize>();
    let event_count = runtime
        .undo_events
        .iter()
        .filter(|event| matches!(event, RuntimeUndoEvent::Text(_)))
        .count();
    assert_eq!(snapshot_count, event_count);
    assert!(snapshot_count > 1);
    assert!(runtime.estimated_text_undo_memory_bytes() <= runtime.text_undo_memory_budget_bytes());
    assert!(runtime.undo_focused_block().unwrap());
}

#[test]
fn new_text_edit_releases_every_redo_snapshot() {
    let mut runtime = runtime_with_paragraph_blocks(2);
    runtime.focus_block_at_offset(1, 0).unwrap();
    runtime.insert_char('a').unwrap();
    runtime.break_typing_coalescing();
    runtime.focus_block_at_offset(2, 0).unwrap();
    runtime.insert_char('b').unwrap();
    assert!(runtime.undo_focused_block().unwrap());
    assert!(!runtime.redo_stacks.is_empty());

    runtime.focus_block_at_offset(1, 1).unwrap();
    runtime.insert_char('c').unwrap();

    assert!(runtime.redo_stacks.is_empty());
    assert!(runtime.redo_events.is_empty());
    assert!(!runtime.can_redo());
}
