use super::InlineSpan;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TablePayload {
    pub rows: Vec<TableRowPayload>,
    pub header_rows: usize,
    pub header_cols: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableRowPayload {
    pub cells: Vec<TableCellPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableCellPayload {
    pub spans: Vec<InlineSpan>,
}
