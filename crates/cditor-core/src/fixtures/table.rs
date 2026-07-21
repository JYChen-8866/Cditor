//! large table fixture：50k 行单表、500 列超宽表与 1k 简单表文档。
//!
//! 对应总设计 29.2 的 "1k simple tables、50k-row single table、超宽
//! 500-column table"。全部生成器确定性，是 Phase 10 表格内部虚拟化
//!（P10-015/P10-018）的验收语料。

use crate::ids::{BlockId, DocumentId};
use crate::rich_text::{
    RichBlockRecord, RichTextDocument, TableCellPayload, TablePayload, TableRowPayload,
};

/// table fixture 生成器版本；语义改动必须提升并更新 pinned checksum。
pub const TABLE_FIXTURE_VERSION: u32 = 1;

/// 完整 tall-table fixture 的行数。
pub const TALL_TABLE_FULL_ROWS: usize = 50_000;
/// tall-table 的列数。
pub const TALL_TABLE_COLUMNS: usize = 8;
/// 完整 wide-table fixture 的列数。
pub const WIDE_TABLE_FULL_COLUMNS: usize = 500;
/// wide-table 的行数。
pub const WIDE_TABLE_ROWS: usize = 40;
/// 完整 many-tables fixture 的表数量。
pub const MANY_TABLES_FULL_COUNT: usize = 1_000;

/// 单个 `rows x TALL_TABLE_COLUMNS` 高表的文档。
pub fn tall_table_document(document_id: DocumentId, rows: usize) -> RichTextDocument {
    let mut document = RichTextDocument::empty(document_id);
    document.metadata.title = Some(format!("Tall table fixture v{TABLE_FIXTURE_VERSION}"));
    document.metadata.tags = vec!["fixture".to_owned(), "tall-table".to_owned()];
    document.push_root_block(RichBlockRecord::table(
        1,
        grid_table(rows, TALL_TABLE_COLUMNS, 0),
    ));
    document
}

/// 单个 `WIDE_TABLE_ROWS x columns` 超宽表的文档。
pub fn wide_table_document(document_id: DocumentId, columns: usize) -> RichTextDocument {
    let mut document = RichTextDocument::empty(document_id);
    document.metadata.title = Some(format!("Wide table fixture v{TABLE_FIXTURE_VERSION}"));
    document.metadata.tags = vec!["fixture".to_owned(), "wide-table".to_owned()];
    document.push_root_block(RichBlockRecord::table(
        1,
        grid_table(WIDE_TABLE_ROWS, columns, 999_983),
    ));
    document
}

/// `table_count` 个 4x3 简单表交替普通段落的文档。
pub fn many_tables_document(document_id: DocumentId, table_count: usize) -> RichTextDocument {
    let mut document = RichTextDocument::empty(document_id);
    document.metadata.title = Some(format!("Many tables fixture v{TABLE_FIXTURE_VERSION}"));
    document.metadata.tags = vec!["fixture".to_owned(), "many-tables".to_owned()];
    let mut block_id: BlockId = 0;
    for index in 0..table_count {
        block_id += 1;
        document.push_root_block(RichBlockRecord::table(
            block_id,
            grid_table(4, 3, index * 100),
        ));
        block_id += 1;
        document.push_root_block(RichBlockRecord::paragraph(
            block_id,
            format!("表格 {index} 之后的分隔段落，保持文档结构混合。"),
        ));
    }
    document
}

/// 确定性网格：首行为 header，单元格内容带行列坐标与少量变长文本。
fn grid_table(rows: usize, columns: usize, seed: usize) -> TablePayload {
    let header = TableRowPayload {
        cells: (0..columns)
            .map(|column| TableCellPayload::plain(format!("C{column:03}")))
            .collect(),
        height: Default::default(),
    };
    let body = (1..rows).map(|row| TableRowPayload {
        cells: (0..columns)
            .map(|column| TableCellPayload::plain(cell_text(row, column, seed)))
            .collect(),
        height: Default::default(),
    });
    TablePayload {
        header_rows: 1,
        header_cols: 0,
        header_style: Default::default(),
        columns: Vec::new(),
        rows: std::iter::once(header).chain(body).collect(),
    }
}

fn cell_text(row: usize, column: usize, seed: usize) -> String {
    match (row + column + seed) % 5 {
        0 => format!("r{row}c{column}"),
        1 => format!("值 {}", (row * 31 + column * 7 + seed) % 10_000),
        2 => format!("multi word cell r{row} c{column}"),
        3 => format!(
            "状态-{}",
            ["就绪", "进行中", "阻塞", "完成"][(row + seed) % 4]
        ),
        _ => format!("note r{row}c{column}: 混合中英文 cell 内容用于换行测量"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::document_semantic_checksum;
    use crate::rich_text::BlockPayload;

    fn table_of(document: &RichTextDocument) -> TablePayload {
        match document.blocks[0].to_payload_record().payload {
            BlockPayload::Table(table) => table,
            other => panic!("expected table payload, got {other:?}"),
        }
    }

    #[test]
    fn tall_table_has_requested_dimensions() {
        let document = tall_table_document(21, 64);
        let table = table_of(&document);
        assert_eq!(table.rows.len(), 64);
        assert!(
            table
                .rows
                .iter()
                .all(|row| row.cells.len() == TALL_TABLE_COLUMNS)
        );
        assert_eq!(table.header_rows, 1);
    }

    #[test]
    fn wide_table_has_requested_dimensions() {
        let document = wide_table_document(22, 96);
        let table = table_of(&document);
        assert_eq!(table.rows.len(), WIDE_TABLE_ROWS);
        assert!(table.rows.iter().all(|row| row.cells.len() == 96));
    }

    #[test]
    fn many_tables_document_interleaves_paragraphs() {
        let document = many_tables_document(23, 16);
        assert_eq!(document.blocks.len(), 32);
        let payloads: Vec<_> = document
            .blocks
            .iter()
            .map(|block| block.to_payload_record().payload)
            .collect();
        assert!(matches!(payloads[0], BlockPayload::Table(_)));
        assert!(matches!(payloads[1], BlockPayload::RichText { .. }));
    }

    #[test]
    fn generators_are_deterministic_and_seed_sensitive() {
        let first = tall_table_document(21, 32);
        let second = tall_table_document(21, 32);
        assert_eq!(
            document_semantic_checksum(&first),
            document_semantic_checksum(&second)
        );

        let tall = table_of(&tall_table_document(21, 32));
        let wide = table_of(&wide_table_document(21, TALL_TABLE_COLUMNS));
        assert_ne!(
            tall.rows[1].cells[0], wide.rows[1].cells[0],
            "seed should differentiate tall/wide cell content"
        );
    }

    #[test]
    #[ignore = "builds the full 50k-row / 500-column / 1k-tables fixtures"]
    fn full_table_fixtures_reach_target_sizes() {
        let tall = table_of(&tall_table_document(21, TALL_TABLE_FULL_ROWS));
        assert_eq!(tall.rows.len(), TALL_TABLE_FULL_ROWS);

        let wide = table_of(&wide_table_document(22, WIDE_TABLE_FULL_COLUMNS));
        assert!(
            wide.rows
                .iter()
                .all(|row| row.cells.len() == WIDE_TABLE_FULL_COLUMNS)
        );

        let many = many_tables_document(23, MANY_TABLES_FULL_COUNT);
        assert_eq!(many.blocks.len(), MANY_TABLES_FULL_COUNT * 2);
    }
}
