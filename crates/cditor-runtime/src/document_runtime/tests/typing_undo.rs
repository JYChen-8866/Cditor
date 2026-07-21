use std::time::{Duration, Instant};

use cditor_core::ids::SurfaceId;
use cditor_core::rich_text::{
    BlockPayload, BlockPayloadRecord, ImagePayload, InlineMark, InlineSpan, RichBlockKind,
};
use cditor_import_export::clipboard::ClipboardSelection;

use super::*;

fn type_text(runtime: &mut DocumentRuntime, text: &str) {
    for ch in text.chars() {
        runtime
            .replace_text_from_platform(None, &ch.to_string())
            .unwrap();
    }
}

fn block_text(runtime: &DocumentRuntime, block_id: BlockId) -> String {
    runtime
        .block_payload_record(block_id)
        .expect("block payload")
        .plain_text()
}

fn first_span_marks(runtime: &DocumentRuntime, block_id: BlockId) -> Vec<InlineMark> {
    let payload = runtime
        .block_payload_record(block_id)
        .expect("block payload");
    let BlockPayload::RichText { spans } = payload.payload else {
        panic!("expected rich text payload");
    };
    spans
        .first()
        .map(|span| span.marks.clone())
        .unwrap_or_default()
}

fn surface_text(runtime: &DocumentRuntime, surface_id: SurfaceId) -> String {
    runtime
        .text_surface_snapshot(surface_id)
        .expect("text surface")
        .plain_text()
}

fn runtime_with_auxiliary_surfaces() -> DocumentRuntime {
    DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord {
                block_id: 10,
                content_version: 1,
                kind: RichBlockKind::Image,
                payload: BlockPayload::Image(ImagePayload {
                    source: "asset://hero".to_owned(),
                    alt: "hero".to_owned(),
                    caption: "caption".into(),
                    display_width_ratio_milli: None,
                }),
            },
            BlockPayloadRecord {
                block_id: 20,
                content_version: 1,
                kind: RichBlockKind::Database,
                payload: BlockPayload::Empty,
            },
        ],
        720.0,
    )
}

#[test]
fn ordinary_block_typing_coalesces_and_redo_restores_the_whole_run() {
    let mut runtime = runtime_with_paragraph_blocks(1);
    runtime.focus_block_at_offset(1, 0).unwrap();

    type_text(&mut runtime, "hello");
    assert_eq!(block_text(&runtime, 1), "hello");

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&runtime, 1), "");
    assert!(!runtime.undo_focused_block().unwrap());

    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(block_text(&runtime, 1), "hello");
    assert!(!runtime.redo_focused_block().unwrap());
}

#[test]
fn ordinary_spaces_coalesce_but_soft_line_breaks_are_independent() {
    let mut runtime = runtime_with_paragraph_blocks(1);
    runtime.focus_block_at_offset(1, 0).unwrap();
    type_text(&mut runtime, "hello");
    runtime.insert_space_or_markdown_shortcut().unwrap();
    type_text(&mut runtime, "world");

    assert_eq!(block_text(&runtime, 1), "hello world");
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&runtime, 1), "");

    type_text(&mut runtime, "ab");
    runtime.insert_soft_line_break().unwrap();
    type_text(&mut runtime, "cd");
    assert_eq!(block_text(&runtime, 1), "ab\ncd");
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&runtime, 1), "ab\n");
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&runtime, 1), "ab");
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&runtime, 1), "");
}

#[test]
fn every_persistent_text_surface_coalesces_typing_independently() {
    let mut table_runtime = DocumentRuntime::from_payloads(1, vec![sample_table_payload()], 720.0);
    let cell = SurfaceId::TableCell {
        block_id: 10,
        row: 0,
        column: 0,
    };
    table_runtime.focus_text_surface_at_offset(cell, 1).unwrap();
    type_text(&mut table_runtime, "xy");
    assert_eq!(surface_text(&table_runtime, cell), "Axy");
    assert!(table_runtime.undo_focused_block().unwrap());
    assert_eq!(surface_text(&table_runtime, cell), "A");
    assert!(table_runtime.redo_focused_block().unwrap());
    assert_eq!(surface_text(&table_runtime, cell), "Axy");

    for (surface_id, initial, typed) in [
        (SurfaceId::ImageCaption { block_id: 10 }, "caption", "12"),
        (SurfaceId::CollectionTitle { block_id: 20 }, "", "Projects"),
    ] {
        let mut runtime = runtime_with_auxiliary_surfaces();
        runtime
            .focus_text_surface_at_offset(surface_id, initial.len())
            .unwrap();
        type_text(&mut runtime, typed);
        assert_eq!(
            surface_text(&runtime, surface_id),
            format!("{initial}{typed}")
        );
        assert!(runtime.undo_focused_block().unwrap());
        assert_eq!(surface_text(&runtime, surface_id), initial);
        assert!(runtime.redo_focused_block().unwrap());
        assert_eq!(
            surface_text(&runtime, surface_id),
            format!("{initial}{typed}")
        );
    }
}

#[test]
fn caret_navigation_and_selection_changes_break_typing_groups() {
    let mut runtime = runtime_with_paragraph_blocks(1);
    runtime.focus_block_at_offset(1, 0).unwrap();
    type_text(&mut runtime, "ab");

    assert!(runtime.move_caret_left(false).unwrap());
    type_text(&mut runtime, "X");
    assert_eq!(block_text(&runtime, 1), "aXb");
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&runtime, 1), "ab");

    runtime.set_document_text_selection(1, 0, 1, 2).unwrap();
    type_text(&mut runtime, "Y");
    assert_eq!(block_text(&runtime, 1), "Y");
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&runtime, 1), "ab");
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&runtime, 1), "");
}

#[test]
fn focus_and_surface_switches_break_typing_groups() {
    let mut runtime = runtime_with_paragraph_blocks(2);
    runtime.focus_block_at_offset(1, 0).unwrap();
    type_text(&mut runtime, "ab");
    runtime.focus_block_at_offset(2, 0).unwrap();
    type_text(&mut runtime, "cd");
    runtime.focus_block_at_offset(1, 2).unwrap();
    type_text(&mut runtime, "X");

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&runtime, 1), "ab");
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&runtime, 2), "");
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&runtime, 1), "");
}

#[test]
fn typing_time_gap_starts_a_new_undo_step() {
    let mut runtime = runtime_with_paragraph_blocks(1);
    runtime.focus_block_at_offset(1, 0).unwrap();
    type_text(&mut runtime, "ab");
    runtime
        .typing_undo_group
        .as_mut()
        .expect("active typing group")
        .last_input_at = Instant::now() - Duration::from_secs(2);

    type_text(&mut runtime, "c");
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&runtime, 1), "ab");
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&runtime, 1), "");
}

#[test]
fn text_undo_redo_restore_semantic_viewport_anchors() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor_core::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let anchor_block = runtime
        .target_for_global_offset(runtime.scroll.global_scroll_top)
        .unwrap()
        .block_id;
    runtime.focus_block_at_offset(500, 0).unwrap();
    type_text(&mut runtime, "undo");

    runtime
        .scroll
        .scroll_to_global_offset(6_400.0, cditor_editor_core::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    assert!(runtime.undo_focused_block().unwrap());
    let anchor_index = runtime
        .visible_index
        .visible_index_of(anchor_block)
        .unwrap();
    assert_eq!(
        runtime.scroll.global_scroll_top,
        runtime.height_index.offset_of_block(anchor_index).unwrap()
    );

    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(runtime.scroll.global_scroll_top, 6_400.0);
}

#[test]
fn text_undo_anchor_survives_height_changes_above_the_viewport() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor_core::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let anchor = runtime
        .target_for_global_offset(runtime.scroll.global_scroll_top)
        .unwrap();
    runtime.focus_block_at_offset(500, 0).unwrap();
    type_text(&mut runtime, "x");

    assert!(runtime.queue_measured_height(1, 1, 64.0).unwrap());
    assert!(runtime.flush_pending_height_corrections().unwrap());
    runtime
        .scroll
        .scroll_to_global_offset(100.0, cditor_editor_core::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    assert!(runtime.undo_focused_block().unwrap());

    let anchor_index = runtime
        .visible_index
        .visible_index_of(anchor.block_id)
        .unwrap();
    assert_eq!(
        runtime.scroll.global_scroll_top,
        runtime.height_index.offset_of_block(anchor_index).unwrap() + anchor.offset_in_block
    );
}

#[test]
fn structure_undo_redo_restore_their_semantic_viewport_anchors() {
    let mut runtime = runtime_with_paragraph_blocks(1_000);
    runtime.focus_block_at_offset(500, 0).unwrap();
    runtime
        .scroll
        .scroll_to_global_offset(3_200.0, cditor_editor_core::scroll::ScrollOrigin::UserWheel)
        .unwrap();
    let before_anchor = runtime
        .target_for_global_offset(runtime.scroll.global_scroll_top)
        .unwrap();

    runtime.insert_paragraph_after_block(1).unwrap();
    let after_anchor = runtime
        .target_for_global_offset(runtime.scroll.global_scroll_top)
        .unwrap();
    assert_ne!(before_anchor.block_id, after_anchor.block_id);
    runtime
        .scroll
        .scroll_to_global_offset(6_400.0, cditor_editor_core::scroll::ScrollOrigin::UserWheel)
        .unwrap();

    assert!(runtime.undo_focused_block().unwrap());
    let before_index = runtime
        .visible_index
        .visible_index_of(before_anchor.block_id)
        .unwrap();
    assert_eq!(
        runtime.scroll.global_scroll_top,
        runtime.height_index.offset_of_block(before_index).unwrap() + before_anchor.offset_in_block
    );

    assert!(runtime.redo_focused_block().unwrap());
    let after_index = runtime
        .visible_index
        .visible_index_of(after_anchor.block_id)
        .unwrap();
    assert_eq!(
        runtime.scroll.global_scroll_top,
        runtime.height_index.offset_of_block(after_index).unwrap() + after_anchor.offset_in_block
    );
}

#[test]
fn structure_undo_redo_restore_cross_block_text_selection() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "first"),
            BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "middle"),
            BlockPayloadRecord::rich_text(3, RichBlockKind::Paragraph, "last"),
        ],
        720.0,
    );
    let before = DocumentSelection {
        anchor: TextPosition::downstream(1, 2),
        focus: TextPosition::downstream(3, 2),
    };
    runtime.set_document_selection(before).unwrap();

    assert!(runtime.delete_document_selection().unwrap());
    let after = runtime.document_selection_snapshot().unwrap();
    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.document_selection_snapshot(), Some(before));
    assert_eq!(block_text(&runtime, 1), "first");
    assert_eq!(block_text(&runtime, 2), "middle");
    assert_eq!(block_text(&runtime, 3), "last");

    assert!(runtime.redo_focused_block().unwrap());
    assert_eq!(runtime.document_selection_snapshot(), Some(after));
}

#[test]
fn structure_undo_redo_restore_whole_block_selection() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "first"),
            BlockPayloadRecord::rich_text(2, RichBlockKind::Paragraph, "middle"),
            BlockPayloadRecord::rich_text(3, RichBlockKind::Paragraph, "last"),
        ],
        720.0,
    );
    assert!(runtime.select_visible_block_range(2, 3));
    let before = runtime.selected_block_ids.clone();
    assert!(
        runtime
            .paste_clipboard_selection(&ClipboardSelection::Blocks {
                blocks: vec![cditor_import_export::clipboard::ClipboardBlock {
                    source_id: 99,
                    parent_source_id: None,
                    depth: 0,
                    kind: RichBlockKind::Paragraph,
                    payload: BlockPayload::RichText {
                        spans: vec![InlineSpan::plain("pasted")],
                    },
                }],
            })
            .unwrap()
    );
    assert!(runtime.selected_block_ids.is_empty());

    assert!(runtime.undo_focused_block().unwrap());
    assert_eq!(runtime.selected_block_ids, before);
    assert!(runtime.redo_focused_block().unwrap());
    assert!(runtime.selected_block_ids.is_empty());
}

#[test]
fn paste_composition_and_format_are_independent_undo_steps() {
    let mut paste_runtime = runtime_with_paragraph_blocks(1);
    paste_runtime.focus_block_at_offset(1, 0).unwrap();
    type_text(&mut paste_runtime, "ab");
    assert!(
        paste_runtime
            .paste_clipboard_selection(&ClipboardSelection::Inline {
                spans: vec![InlineSpan::plain("PASTE")],
            })
            .unwrap()
    );
    type_text(&mut paste_runtime, "c");
    assert!(paste_runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&paste_runtime, 1), "abPASTE");
    assert!(paste_runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&paste_runtime, 1), "ab");
    assert!(paste_runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&paste_runtime, 1), "");

    let mut ime_runtime = runtime_with_paragraph_blocks(1);
    ime_runtime.focus_block_at_offset(1, 0).unwrap();
    type_text(&mut ime_runtime, "ab");
    ime_runtime
        .begin_or_update_composition(1, 2..2, "中")
        .unwrap();
    assert!(ime_runtime.commit_composition().unwrap());
    assert!(ime_runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&ime_runtime, 1), "ab");
    assert!(ime_runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&ime_runtime, 1), "");

    let mut format_runtime = runtime_with_paragraph_blocks(1);
    format_runtime.focus_block_at_offset(1, 0).unwrap();
    type_text(&mut format_runtime, "ab");
    format_runtime
        .set_document_text_selection(1, 0, 1, 2)
        .unwrap();
    assert!(
        format_runtime
            .toggle_inline_mark_on_selection(InlineMark::Bold)
            .unwrap()
    );
    format_runtime.focus_block_at_offset(1, 2).unwrap();
    type_text(&mut format_runtime, "c");
    assert!(format_runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&format_runtime, 1), "ab");
    assert_eq!(first_span_marks(&format_runtime, 1), vec![InlineMark::Bold]);
    assert!(format_runtime.undo_focused_block().unwrap());
    assert!(first_span_marks(&format_runtime, 1).is_empty());
    assert!(format_runtime.undo_focused_block().unwrap());
    assert_eq!(block_text(&format_runtime, 1), "");
}
