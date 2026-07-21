use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TableCellStyle {
    pub background_color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TableHeaderStyle {
    pub row_background_color: Option<String>,
    pub column_background_color: Option<String>,
}
