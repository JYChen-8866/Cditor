use super::*;

#[test]
fn parent_drop_target_uses_previous_supported_block_outside_source_subtree() {
    let rects = vec![
        ProjectedBlockRect {
            block_id: 1,
            visible_index: 0,
            depth: 0,
            document_top: 0.0,
            document_bottom: 40.0,
            indent_px: 0.0,
            outer_padding_top_px: 2.0,
            text_origin_x_in_block_px: 0.0,
            text_origin_y_in_block_px: 0.0,
            text_width_px: 860.0,
            supports_children: true,
            ..ProjectedBlockRect::default()
        },
        ProjectedBlockRect {
            block_id: 2,
            visible_index: 1,
            depth: 1,
            document_top: 40.0,
            document_bottom: 80.0,
            indent_px: 24.0,
            outer_padding_top_px: 2.0,
            text_origin_x_in_block_px: 24.0,
            text_origin_y_in_block_px: 0.0,
            text_width_px: 836.0,
            supports_children: true,
            ..ProjectedBlockRect::default()
        },
        ProjectedBlockRect {
            block_id: 3,
            visible_index: 2,
            depth: 0,
            document_top: 80.0,
            document_bottom: 120.0,
            indent_px: 0.0,
            outer_padding_top_px: 2.0,
            text_origin_x_in_block_px: 0.0,
            text_origin_y_in_block_px: 0.0,
            text_width_px: 860.0,
            supports_children: false,
            ..ProjectedBlockRect::default()
        },
        ProjectedBlockRect {
            block_id: 4,
            visible_index: 3,
            depth: 0,
            document_top: 120.0,
            document_bottom: 160.0,
            indent_px: 0.0,
            outer_padding_top_px: 2.0,
            text_origin_x_in_block_px: 0.0,
            text_origin_y_in_block_px: 0.0,
            text_width_px: 860.0,
            supports_children: true,
            ..ProjectedBlockRect::default()
        },
    ];

    assert_eq!(
        parent_drop_target_from_rects(
            &rects,
            1,
            BlockDropTarget {
                insert_before_block_id: Some(4),
                target_visible_index: 3,
            },
        ),
        None
    );
    assert_eq!(
        parent_drop_target_from_rects(
            &rects,
            3,
            BlockDropTarget {
                insert_before_block_id: Some(4),
                target_visible_index: 3,
            },
        ),
        Some(ParentDropTarget {
            parent_id: 2,
            sibling_index: usize::MAX,
        })
    );
}
