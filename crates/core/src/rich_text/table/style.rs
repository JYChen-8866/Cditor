#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableCellStyle {
    pub background_color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableHeaderStyle {
    pub row_background_color: Option<String>,
    pub column_background_color: Option<String>,
}
