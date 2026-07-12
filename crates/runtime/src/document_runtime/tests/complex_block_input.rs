/// Tests for complex block input capability boundaries
///
/// Ensures that complex blocks (whiteboard, image, file, etc.) are not
/// accidentally degraded to text when receiving text editing commands like Enter.
use cditor_core::rich_text::{FilePayload, ImagePayload, WhiteboardPayload};

use super::*;

fn create_runtime_with_whiteboard() -> (DocumentRuntime, BlockId) {
    let scene_json = r#"{"elements":[{"id":"elem1","type":"rect"}],"version":1}"#.to_string();
    let runtime = DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "First paragraph"),
            BlockPayloadRecord {
                block_id: 2,
                content_version: 1,
                kind: RichBlockKind::Whiteboard,
                payload: BlockPayload::Whiteboard(WhiteboardPayload {
                    scene_json: scene_json.clone(),
                }),
            },
        ],
        720.0,
    );
    (runtime, 2)
}

#[test]
fn test_whiteboard_focus_creates_complex_input_target() {
    let (mut runtime, wb_id) = create_runtime_with_whiteboard();

    // Focus the whiteboard
    runtime.focus_block(wb_id);

    // Check that input target is ComplexBlock, not BlockText
    let input_target = runtime.editing.as_ref().map(|editing| editing.input_target);
    assert!(
        matches!(
            input_target,
            Some(crate::editing::session::InputTarget::ComplexBlock { .. })
        ),
        "Expected ComplexBlock input target, got {:?}",
        input_target
    );
}

#[test]
fn test_whiteboard_enter_inserts_paragraph_after_not_split() {
    let (mut runtime, wb_id) = create_runtime_with_whiteboard();

    // Get the original scene JSON
    let original_scene = runtime
        .payload_window
        .get(wb_id)
        .and_then(|p| match &p.payload {
            BlockPayload::Whiteboard(wb) => Some(wb.scene_json.clone()),
            _ => None,
        })
        .expect("whiteboard payload exists");

    assert!(
        original_scene.contains("elem1"),
        "Original scene should have elements"
    );

    // Focus the whiteboard
    runtime.focus_block(wb_id);

    // Press Enter
    let result = runtime.handle_enter();
    assert!(result.is_ok(), "handle_enter should succeed: {:?}", result);

    // Check that the whiteboard block still exists with its kind and payload intact
    let wb_payload = runtime.payload_window.get(wb_id).expect("wb still exists");
    assert_eq!(
        wb_payload.kind,
        RichBlockKind::Whiteboard,
        "Whiteboard kind should not change"
    );

    let scene_after = match &wb_payload.payload {
        BlockPayload::Whiteboard(wb) => wb.scene_json.clone(),
        other => panic!("Payload was corrupted to {:?}", other),
    };

    assert_eq!(
        scene_after, original_scene,
        "Whiteboard scene JSON should not be modified or degraded to 'whiteboard' text"
    );

    // Check that a new paragraph was inserted after the whiteboard
    let wb_index = runtime.index.index_of(wb_id).expect("wb in index");
    let next_id = runtime.index.block_ids.get(wb_index + 1).copied();
    assert!(next_id.is_some(), "A block should be inserted after wb");

    if let Some(next_id) = next_id {
        let next_payload = runtime.payload_window.get(next_id).expect("next exists");
        assert!(
            matches!(next_payload.kind, RichBlockKind::Paragraph),
            "New block should be paragraph, got {:?}",
            next_payload.kind
        );

        // Check that focus moved to the new paragraph
        let focused = runtime.focused_block_id();
        assert_eq!(focused, Some(next_id), "Focus should move to new paragraph");
    }
}

#[test]
fn test_mermaid_source_focuses_as_text_and_enter_inserts_newline() {
    let source = "flowchart TD\n  A --> B";
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![BlockPayloadRecord::rich_text(
            1,
            RichBlockKind::Mermaid,
            source,
        )],
        720.0,
    );

    runtime
        .focus_block_at_offset(1, source.len())
        .expect("mermaid source should accept a caret");
    assert!(matches!(
        runtime.editing.as_ref().map(|editing| editing.input_target),
        Some(crate::editing::session::InputTarget::BlockText { block_id: 1 })
    ));

    runtime
        .handle_enter()
        .expect("enter should insert a newline");

    assert_eq!(runtime.index.block_ids, vec![1]);
    assert_eq!(
        runtime
            .block_payload_record(1)
            .expect("mermaid payload")
            .plain_text(),
        format!("{source}\n")
    );
}

#[test]
fn test_image_block_enter_does_not_split() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "First"),
            BlockPayloadRecord {
                block_id: 2,
                content_version: 1,
                kind: RichBlockKind::Image,
                payload: BlockPayload::Image(ImagePayload {
                    source: "https://example.com/image.png".to_string(),
                    alt: "Test Image".to_string(),
                    caption: "".to_string(),
                    display_width_ratio_milli: None,
                }),
            },
        ],
        720.0,
    );

    let img_id = 2;
    runtime.focus_block(img_id);
    let result = runtime.handle_enter();
    assert!(result.is_ok());

    // Image should remain intact
    let img_payload = runtime.payload_window.get(img_id).unwrap();
    assert_eq!(img_payload.kind, RichBlockKind::Image);
    assert!(matches!(img_payload.payload, BlockPayload::Image(_)));

    // New paragraph should be inserted after
    let img_index = runtime.index.index_of(img_id).unwrap();
    let next_id = runtime.index.block_ids.get(img_index + 1).copied();
    assert!(next_id.is_some());
}

#[test]
fn test_file_block_enter_does_not_split() {
    let mut runtime = DocumentRuntime::from_payloads(
        1,
        vec![
            BlockPayloadRecord::rich_text(1, RichBlockKind::Paragraph, "First"),
            BlockPayloadRecord {
                block_id: 2,
                content_version: 1,
                kind: RichBlockKind::File,
                payload: BlockPayload::File(FilePayload {
                    name: "document.pdf".to_string(),
                    source: "file:///path/to/document.pdf".to_string(),
                    size_bytes: Some(1024),
                }),
            },
        ],
        720.0,
    );

    let file_id = 2;
    runtime.focus_block(file_id);
    let result = runtime.handle_enter();
    assert!(result.is_ok());

    // File should remain intact
    let file_payload = runtime.payload_window.get(file_id).unwrap();
    assert_eq!(file_payload.kind, RichBlockKind::File);
    assert!(matches!(file_payload.payload, BlockPayload::File(_)));
}
