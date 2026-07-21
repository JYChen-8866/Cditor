use super::*;

#[test]
fn unified_selection_roundtrips_text_block_and_inner_endpoints() {
    let selections = [
        UnifiedDocumentSelection::text(DocumentSelection {
            anchor: TextPosition::downstream(1, 2),
            focus: TextPosition {
                block_id: 3,
                offset: 4,
                affinity: TextAffinity::Upstream,
            },
        }),
        UnifiedDocumentSelection::block(9),
        UnifiedDocumentSelection::inner(
            11,
            InnerSelectionAnchor::TableCell {
                row: 2,
                col: 3,
                offset: 7,
            },
        ),
    ];

    for selection in selections {
        let encoded = serde_json::to_vec(&selection).unwrap();
        let decoded: UnifiedDocumentSelection = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, selection);
    }
}

#[test]
fn text_projection_is_lossless_and_non_text_endpoints_do_not_fake_offsets() {
    let text = DocumentSelection {
        anchor: TextPosition::downstream(1, 2),
        focus: TextPosition::downstream(3, 4),
    };
    assert_eq!(text.unified().text_projection(), Some(text));
    assert_eq!(UnifiedDocumentSelection::block(1).text_projection(), None);
    assert_eq!(
        UnifiedDocumentSelection::inner(1, InnerSelectionAnchor::CanvasPoint { x: 4, y: 5 })
            .text_projection(),
        None
    );
}
