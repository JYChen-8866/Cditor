use std::ops::Range;

use crate::document::BlockIndexRecord;
use crate::ids::{BlockId, DocumentId};
use crate::layout::{BlockLayoutMeta, estimate_kind_fallback_height};
use crate::rich_text::{
    BlockPayload, BlockPayloadRecord, CalloutVariant, EmbedPayload, InlineMark, InlineSpan,
    RichBlockKind, RichBlockRecord, RichTextDocument, TableCellPayload, TablePayload,
    TableRowPayload, WhiteboardPayload, kind_tag_for_rich_block_kind,
};

pub const LARGE_MIXED_DEMO_BLOCKS: usize = 100_000;
pub const LARGE_MIXED_DEMO_DOCUMENT_ID: DocumentId = 100_000;

pub fn large_mixed_demo_document() -> RichTextDocument {
    large_mixed_rich_text_document(LARGE_MIXED_DEMO_DOCUMENT_ID, LARGE_MIXED_DEMO_BLOCKS)
}

pub fn large_mixed_demo_index_records(count: usize) -> Vec<BlockIndexRecord> {
    (0..count)
        .map(|index| {
            let block_id = index as BlockId + 1;
            let kind = demo_kind(index);
            BlockIndexRecord::new(block_id, None, 0, kind_tag_for_rich_block_kind(&kind), 0)
                .with_layout_meta(BlockLayoutMeta::new(
                    block_id,
                    estimated_height_for_kind(&kind),
                ))
        })
        .collect()
}

pub fn large_mixed_demo_payload_records(
    range: Range<usize>,
    count: usize,
) -> Vec<BlockPayloadRecord> {
    let start = range.start.min(count);
    let end = range.end.min(count).max(start);
    (start..end)
        .map(|index| demo_payload_record(index as BlockId + 1, index))
        .collect()
}

pub fn demo_payload_record(block_id: BlockId, index: usize) -> BlockPayloadRecord {
    demo_block(block_id, index).to_payload_record()
}

pub fn large_mixed_rich_text_document(document_id: DocumentId, count: usize) -> RichTextDocument {
    let mut document = RichTextDocument::empty(document_id);
    document.metadata.title = Some(format!("CDitor V2 10w mixed syntax demo ({count} blocks)"));
    document.metadata.tags = vec![
        "large-document".to_owned(),
        "mixed-syntax".to_owned(),
        "scroll-acceptance".to_owned(),
    ];

    for index in 0..count {
        document.push_root_block(demo_block(index as BlockId + 1, index));
    }

    document
}

fn demo_kind(index: usize) -> RichBlockKind {
    match index % 36 {
        0 => RichBlockKind::Heading { level: 1 },
        1 => RichBlockKind::Heading { level: 2 },
        2 => RichBlockKind::Heading { level: 3 },
        3 => RichBlockKind::Heading { level: 4 },
        4 => RichBlockKind::Heading { level: 5 },
        5 => RichBlockKind::Heading { level: 6 },
        7 => RichBlockKind::Quote,
        8 => RichBlockKind::Callout {
            variant: callout_variant(index / 36),
        },
        9 => RichBlockKind::Todo {
            checked: index % 2 == 0,
        },
        10 => RichBlockKind::BulletedList,
        11 => RichBlockKind::NumberedList,
        12 => RichBlockKind::Toggle,
        13 => RichBlockKind::Code {
            language: Some("rust".to_owned()),
        },
        14 => RichBlockKind::Math,
        15 => RichBlockKind::Mermaid,
        16 => RichBlockKind::Html,
        17 => RichBlockKind::Table,
        18 => RichBlockKind::Image,
        19 => RichBlockKind::File,
        20 => RichBlockKind::Attachment,
        21 => RichBlockKind::Whiteboard,
        22 => RichBlockKind::MindMap,
        23 => RichBlockKind::Embed,
        24 => RichBlockKind::Divider,
        25 => RichBlockKind::Separator,
        26 => RichBlockKind::FootnoteDefinition,
        27 => RichBlockKind::Comment,
        28 => RichBlockKind::RawMarkdown,
        29 => RichBlockKind::Database,
        30 => RichBlockKind::Custom("kanban-card".to_owned()),
        34 => RichBlockKind::Code {
            language: Some("typescript".to_owned()),
        },
        _ => RichBlockKind::Paragraph,
    }
}

fn estimated_height_for_kind(kind: &RichBlockKind) -> f64 {
    estimate_kind_fallback_height(kind).height
}

fn demo_block(block_id: BlockId, index: usize) -> RichBlockRecord {
    match index % 36 {
        0 => RichBlockRecord::heading(
            block_id,
            1,
            format!("# 大文档章节 {} · Heading 1", index / 36 + 1),
        ),
        1 => RichBlockRecord::heading(block_id, 2, format!("## 二级标题 block {block_id}")),
        2 => RichBlockRecord::heading(block_id, 3, format!("### 三级标题 block {block_id}")),
        3 => RichBlockRecord::heading(block_id, 4, format!("#### 四级标题 block {block_id}")),
        4 => RichBlockRecord::heading(block_id, 5, format!("##### 五级标题 block {block_id}")),
        5 => RichBlockRecord::heading(block_id, 6, format!("###### 六级标题 block {block_id}")),
        6 => rich_text_block(
            block_id,
            RichBlockKind::Paragraph,
            vec![
                InlineSpan::plain(format!("段落 {block_id}: 支持 ")),
                marked("bold", InlineMark::Bold),
                InlineSpan::plain(" / "),
                marked("italic", InlineMark::Italic),
                InlineSpan::plain(" / "),
                marked("underline", InlineMark::Underline),
                InlineSpan::plain(" / "),
                marked("strike", InlineMark::Strike),
                InlineSpan::plain(" / "),
                marked("inline_code()", InlineMark::Code),
                InlineSpan::plain(" / "),
                marked(
                    "link",
                    InlineMark::Link {
                        href: "https://example.com/cditor".to_owned(),
                    },
                ),
                InlineSpan::plain("，并混合中文、emoji 🚀、English。"),
            ],
        ),
        7 => RichBlockRecord::quote(
            block_id,
            format!("> 引用块 {block_id}: UI 只是投影，runtime 才是真相。"),
        ),
        8 => {
            let variant = callout_variant(index / 36);
            RichBlockRecord::callout(
                block_id,
                variant,
                format!("Callout {variant:?}: 大文档滚动必须准确、连续、不抖动。"),
            )
        }
        9 => RichBlockRecord::todo(
            block_id,
            index % 2 == 0,
            format!("Todo item {block_id}: 检查 IME / markdown shortcut / caret anchor"),
        ),
        10 => RichBlockRecord::bulleted_list(
            block_id,
            format!("- bullet item {block_id} with nested-ready payload"),
        ),
        11 => RichBlockRecord::numbered_list(
            block_id,
            format!("{}. ordered item with stable height", index + 1),
        ),
        12 => RichBlockRecord::rich_text(
            block_id,
            RichBlockKind::Toggle,
            format!("Toggle {block_id}: folded children placeholder"),
        ),
        13 => RichBlockRecord::code_block(
            block_id,
            Some("rust".to_owned()),
            format!(
                "fn block_{block_id}() -> usize {{\n    let value = {index};\n    value + 1\n}}"
            ),
        ),
        14 => RichBlockRecord::rich_text(
            block_id,
            RichBlockKind::Math,
            format!("$$ E = mc^2 + {index} $$"),
        ),
        15 => RichBlockRecord::rich_text(
            block_id,
            RichBlockKind::Mermaid,
            format!("graph TD; A[{block_id}] --> B[virtual scroll]; B --> C[anchor restore];"),
        ),
        16 => RichBlockRecord::new(
            block_id,
            RichBlockKind::Html,
            BlockPayload::Html {
                html: format!(
                    "<section><strong>HTML block {block_id}</strong><p>sanitized preview</p></section>"
                ),
                sanitized: true,
            },
        ),
        17 => RichBlockRecord::table(block_id, demo_table(block_id)),
        18 => RichBlockRecord::image(
            block_id,
            format!("asset://demo/image-{block_id}.png"),
            format!("image alt {block_id}"),
            "稳定盒图片 caption",
        ),
        19 => RichBlockRecord::file(
            block_id,
            format!("asset://demo/file-{block_id}.pdf"),
            format!("design-note-{block_id}.pdf"),
            Some(128 * 1024 + index as u64),
        ),
        20 => RichBlockRecord::attachment(
            block_id,
            format!("asset://demo/attachment-{block_id}.zip"),
            format!("attachment-{block_id}.zip"),
            Some(512 * 1024 + index as u64),
        ),
        21 => RichBlockRecord::whiteboard(
            block_id,
            format!(r#"{{"type":"whiteboard","id":{block_id},"shapes":3}}"#),
        ),
        22 => RichBlockRecord::new(
            block_id,
            RichBlockKind::MindMap,
            BlockPayload::Whiteboard(WhiteboardPayload {
                scene_json: format!(r#"{{"type":"mindmap","root":"block-{block_id}"}}"#),
            }),
        ),
        23 => RichBlockRecord::new(
            block_id,
            RichBlockKind::Embed,
            BlockPayload::Embed(EmbedPayload {
                url: format!("https://example.com/embed/{block_id}"),
                title: format!("Embed preview {block_id}"),
            }),
        ),
        24 => RichBlockRecord::divider(block_id),
        25 => RichBlockRecord::separator(block_id),
        26 => RichBlockRecord::footnote_definition(
            block_id,
            format!("[^note-{block_id}]: footnote definition for long document."),
        ),
        27 => RichBlockRecord::comment(block_id, format!("Comment {block_id}: 这里是批注内容。")),
        28 => RichBlockRecord::raw_markdown(
            block_id,
            format!(
                "**Raw markdown {block_id}** with `code`, [link](https://example.com), and ~~strike~~."
            ),
        ),
        29 => RichBlockRecord::new(
            block_id,
            RichBlockKind::Database,
            BlockPayload::Table(demo_database_table(block_id)),
        ),
        30 => RichBlockRecord::new(
            block_id,
            RichBlockKind::Custom("kanban-card".to_owned()),
            BlockPayload::RichText {
                spans: vec![InlineSpan::plain(format!(
                    "Custom block {block_id}: kanban-card payload"
                ))],
            },
        ),
        31 => rich_text_block(
            block_id,
            RichBlockKind::Paragraph,
            vec![
                marked("红色文本", InlineMark::Color("#cf222e".to_owned())),
                InlineSpan::plain(" + "),
                marked("黄色背景", InlineMark::Background("#fff8c5".to_owned())),
                InlineSpan::plain(format!(" · color/background block {block_id}")),
            ],
        ),
        32 => RichBlockRecord::paragraph(
            block_id,
            format!("Markdown shortcuts sample {block_id}: # / ## / - / 1. / [ ] / ```rust"),
        ),
        33 => RichBlockRecord::paragraph(
            block_id,
            format!("中文 IME 压测行 {block_id}: 埃塞俄比亚咖啡、输入法候选、caret 末尾稳定。"),
        ),
        34 => RichBlockRecord::code_block(
            block_id,
            Some("typescript".to_owned()),
            format!("export const block{block_id} = {{ kind: 'mixed', index: {index} }};"),
        ),
        _ => RichBlockRecord::paragraph(
            block_id,
            format!("普通段落 {block_id}: 用于填充 10w block，保持滚动连续。"),
        ),
    }
}

fn rich_text_block(
    block_id: BlockId,
    kind: RichBlockKind,
    spans: Vec<InlineSpan>,
) -> RichBlockRecord {
    RichBlockRecord::new(block_id, kind, BlockPayload::RichText { spans })
}

fn marked(text: impl Into<String>, mark: InlineMark) -> InlineSpan {
    InlineSpan {
        text: text.into(),
        marks: vec![mark],
    }
}

fn callout_variant(index: usize) -> CalloutVariant {
    match index % 8 {
        0 => CalloutVariant::Note,
        1 => CalloutVariant::Tip,
        2 => CalloutVariant::Important,
        3 => CalloutVariant::Warning,
        4 => CalloutVariant::Caution,
        5 => CalloutVariant::Info,
        6 => CalloutVariant::Success,
        _ => CalloutVariant::Danger,
    }
}

fn demo_table(block_id: BlockId) -> TablePayload {
    TablePayload {
        header_rows: 1,
        header_cols: 1,
        header_style: Default::default(),
        columns: Vec::new(),
        rows: vec![
            table_row(["Name", "Kind", "Status"]),
            table_row([
                format!("block-{block_id}"),
                "table".to_owned(),
                "loaded".to_owned(),
            ]),
            table_row([
                "height".to_owned(),
                "stable".to_owned(),
                "measured later".to_owned(),
            ]),
        ],
    }
}

fn demo_database_table(block_id: BlockId) -> TablePayload {
    TablePayload {
        header_rows: 1,
        header_cols: 0,
        header_style: Default::default(),
        columns: Vec::new(),
        rows: vec![
            table_row(["Property", "Value", "Type"]),
            table_row([
                "title".to_owned(),
                format!("Row {block_id}"),
                "text".to_owned(),
            ]),
            table_row([
                "status".to_owned(),
                "In Progress".to_owned(),
                "select".to_owned(),
            ]),
        ],
    }
}

fn table_row<const N: usize>(cells: [impl Into<String>; N]) -> TableRowPayload {
    TableRowPayload {
        cells: cells.into_iter().map(TableCellPayload::plain).collect(),
        height: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_demo_has_requested_block_count() {
        let document = large_mixed_rich_text_document(7, 10_000);

        assert_eq!(document.blocks.len(), 10_000);
        assert_eq!(document.payload_records().len(), 10_000);
        assert_eq!(document.index_records().len(), 10_000);
    }

    #[test]
    #[ignore = "builds the full 100k block demo used by cargo run"]
    fn large_mixed_demo_has_100k_blocks() {
        let document = large_mixed_demo_document();

        assert_eq!(document.blocks.len(), LARGE_MIXED_DEMO_BLOCKS);
        assert_eq!(document.payload_records().len(), LARGE_MIXED_DEMO_BLOCKS);
        assert_eq!(document.index_records().len(), LARGE_MIXED_DEMO_BLOCKS);
    }

    #[test]
    fn mixed_demo_sample_covers_all_primary_block_kinds() {
        let document = large_mixed_rich_text_document(7, 72);
        let kinds = document
            .payload_records()
            .into_iter()
            .map(|payload| payload.kind)
            .collect::<Vec<_>>();

        assert!(kinds.contains(&RichBlockKind::Paragraph));
        assert!(kinds.contains(&RichBlockKind::Heading { level: 1 }));
        assert!(kinds.contains(&RichBlockKind::Heading { level: 6 }));
        assert!(kinds.contains(&RichBlockKind::Quote));
        assert!(
            kinds
                .iter()
                .any(|kind| matches!(kind, RichBlockKind::Callout { .. }))
        );
        assert!(
            kinds
                .iter()
                .any(|kind| matches!(kind, RichBlockKind::Todo { .. }))
        );
        assert!(kinds.contains(&RichBlockKind::BulletedList));
        assert!(kinds.contains(&RichBlockKind::NumberedList));
        assert!(kinds.contains(&RichBlockKind::Toggle));
        assert!(
            kinds
                .iter()
                .any(|kind| matches!(kind, RichBlockKind::Code { .. }))
        );
        assert!(kinds.contains(&RichBlockKind::Math));
        assert!(kinds.contains(&RichBlockKind::Mermaid));
        assert!(kinds.contains(&RichBlockKind::Html));
        assert!(kinds.contains(&RichBlockKind::Table));
        assert!(kinds.contains(&RichBlockKind::Image));
        assert!(kinds.contains(&RichBlockKind::File));
        assert!(kinds.contains(&RichBlockKind::Attachment));
        assert!(kinds.contains(&RichBlockKind::Whiteboard));
        assert!(kinds.contains(&RichBlockKind::MindMap));
        assert!(kinds.contains(&RichBlockKind::Embed));
        assert!(kinds.contains(&RichBlockKind::Divider));
        assert!(kinds.contains(&RichBlockKind::Separator));
        assert!(kinds.contains(&RichBlockKind::FootnoteDefinition));
        assert!(kinds.contains(&RichBlockKind::Comment));
        assert!(kinds.contains(&RichBlockKind::RawMarkdown));
        assert!(kinds.contains(&RichBlockKind::Database));
        assert!(
            kinds
                .iter()
                .any(|kind| matches!(kind, RichBlockKind::Custom(_)))
        );
    }
}
