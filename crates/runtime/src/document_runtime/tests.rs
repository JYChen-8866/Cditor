use cditor_core::block::BlockPrefixSnapshot;

use super::*;

fn runtime_with_paragraph_blocks(count: usize) -> DocumentRuntime {
    let records = (1..=count as BlockId)
        .map(|block_id| {
            BlockIndexRecord::new(
                block_id,
                None,
                0,
                kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
                0,
            )
            .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(block_id, 32.0))
        })
        .collect::<Vec<_>>();
    let payloads = (1..=count as BlockId)
        .map(|block_id| BlockPayloadRecord::rich_text(block_id, RichBlockKind::Paragraph, ""))
        .collect::<Vec<_>>();
    DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0)
}

fn runtime_with_kind_depths(
    kinds_and_depths: Vec<(RichBlockKind, u16, Option<BlockId>)>,
) -> DocumentRuntime {
    runtime_with_kind_depths_and_text(
        kinds_and_depths
            .into_iter()
            .map(|(kind, depth, parent_id)| (kind, depth, parent_id, "item"))
            .collect(),
    )
}

fn runtime_with_kind_depths_and_text(
    blocks: Vec<(RichBlockKind, u16, Option<BlockId>, &str)>,
) -> DocumentRuntime {
    let records = blocks
        .iter()
        .enumerate()
        .map(|(index, (kind, depth, parent_id, _text))| {
            let block_id = (index + 1) as BlockId;
            BlockIndexRecord::new(
                block_id,
                *parent_id,
                *depth,
                kind_tag_for_rich_block_kind(kind),
                0,
            )
            .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(block_id, 32.0))
        })
        .collect::<Vec<_>>();
    let payloads = blocks
        .into_iter()
        .enumerate()
        .map(|(index, (kind, _, _, text))| {
            BlockPayloadRecord::rich_text((index + 1) as BlockId, kind, text)
        })
        .collect::<Vec<_>>();
    DocumentRuntime::from_index_records(1, records, payloads, 1, 720.0)
}

fn runtime_with_rich_spans(spans: Vec<InlineSpan>) -> DocumentRuntime {
    let record = BlockIndexRecord::new(
        1,
        None,
        0,
        kind_tag_for_rich_block_kind(&RichBlockKind::Paragraph),
        0,
    )
    .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(1, 32.0));
    let payload = BlockPayloadRecord {
        block_id: 1,
        content_version: 1,
        kind: RichBlockKind::Paragraph,
        payload: BlockPayload::RichText { spans },
    };
    DocumentRuntime::from_index_records(1, vec![record], vec![payload], 1, 720.0)
}

fn runtime_with_single_payload(kind: RichBlockKind, payload: BlockPayload) -> DocumentRuntime {
    let record = BlockIndexRecord::new(1, None, 0, kind_tag_for_rich_block_kind(&kind), 0)
        .with_layout_meta(cditor_core::layout::BlockLayoutMeta::new(1, 32.0));
    let payload = BlockPayloadRecord {
        block_id: 1,
        content_version: 1,
        kind,
        payload,
    };
    DocumentRuntime::from_index_records(1, vec![record], vec![payload], 1, 720.0)
}

fn sample_table_payload() -> BlockPayloadRecord {
    let mut table = cditor_core::rich_text::TablePayload {
        rows: vec![
            cditor_core::rich_text::TableRowPayload {
                cells: vec![
                    cditor_core::rich_text::TableCellPayload::plain("A"),
                    cditor_core::rich_text::TableCellPayload::plain("B"),
                ],
                height: cditor_core::rich_text::TableTrackSize::Auto,
            },
            cditor_core::rich_text::TableRowPayload {
                cells: vec![
                    cditor_core::rich_text::TableCellPayload::plain("C"),
                    cditor_core::rich_text::TableCellPayload::plain("D"),
                ],
                height: cditor_core::rich_text::TableTrackSize::Auto,
            },
        ],
        columns: Vec::new(),
        header_rows: 1,
        header_cols: 0,
        header_style: cditor_core::rich_text::TableHeaderStyle::default(),
    };
    table.normalize();
    BlockPayloadRecord {
        block_id: 10,
        content_version: 1,
        kind: RichBlockKind::Table,
        payload: BlockPayload::Table(table),
    }
}

#[path = "tests/ai.rs"]
mod ai;
#[path = "tests/clipboard.rs"]
mod clipboard;
#[path = "tests/complex_block_input.rs"]
mod complex_block_input;
#[path = "tests/composition_input.rs"]
mod composition_input;
#[path = "tests/conversion_clipboard_media.rs"]
mod conversion_clipboard_media;
#[path = "tests/delete_navigation_height.rs"]
mod delete_navigation_height;
#[path = "tests/inline_markdown_incremental.rs"]
mod inline_markdown_incremental;
#[path = "tests/large_window.rs"]
mod large_window;
#[path = "tests/list_structure.rs"]
mod list_structure;
#[path = "tests/markdown.rs"]
mod markdown;
#[path = "tests/multi_block_delete.rs"]
mod multi_block_delete;
#[path = "tests/payload_window_store.rs"]
mod payload_window_store;
#[path = "tests/rich_text_edit.rs"]
mod rich_text_edit;
#[path = "tests/runtime_shortcuts.rs"]
mod runtime_shortcuts;
#[path = "tests/selection_scroll.rs"]
mod selection_scroll;
#[path = "tests/table_clipboard_resize.rs"]
mod table_clipboard_resize;
#[path = "tests/table_core.rs"]
mod table_core;
#[path = "tests/table_structure_layout.rs"]
mod table_structure_layout;
#[path = "tests/table_style_input.rs"]
mod table_style_input;
#[path = "tests/window_projection.rs"]
mod window_projection;
