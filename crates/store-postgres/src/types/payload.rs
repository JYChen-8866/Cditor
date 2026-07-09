use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DbBlockPayload {
    RichText {
        spans: Vec<DbInlineSpan>,
    },
    Code {
        language: Option<String>,
        text: String,
    },
    Table {
        rows: Vec<DbTableRow>,
        #[serde(default)]
        columns: Vec<DbTableColumn>,
        header_rows: usize,
        header_cols: usize,
        #[serde(default)]
        header_style: DbTableHeaderStyle,
    },
    Image {
        source: String,
        alt: String,
        caption: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_width_ratio_milli: Option<u16>,
    },
    File {
        name: String,
        source: String,
        size_bytes: Option<u64>,
    },
    Whiteboard {
        scene_json: String,
    },
    Embed {
        url: String,
        title: String,
    },
    Html {
        html: String,
        sanitized: bool,
    },
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbInlineSpan {
    pub text: String,
    pub marks: Vec<DbInlineMark>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum DbInlineMark {
    Bold,
    Italic,
    Underline,
    Strike,
    Code,
    Link { href: String },
    Color(String),
    Background(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbTableRow {
    pub cells: Vec<DbTableCell>,
    #[serde(default)]
    pub height: DbTableTrackSize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DbTableColumn {
    #[serde(default)]
    pub width: DbTableTrackSize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbTableCell {
    pub spans: Vec<DbInlineSpan>,
    #[serde(default)]
    pub align: DbTableCellAlign,
    #[serde(default)]
    pub merge: DbTableCellMerge,
    #[serde(default)]
    pub style: DbTableCellStyle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DbTableCellStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DbTableHeaderStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_background_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_background_color: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum DbTableTrackSize {
    #[default]
    Auto,
    Px(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DbTableCellAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DbTableCellMerge {
    #[default]
    Unmerged,
    Origin {
        row_span: usize,
        col_span: usize,
    },
    Covered {
        origin_row: usize,
        origin_col: usize,
    },
}

impl From<&BlockPayload> for DbBlockPayload {
    fn from(payload: &BlockPayload) -> Self {
        match payload {
            BlockPayload::RichText { spans } => Self::RichText {
                spans: spans.iter().map(DbInlineSpan::from).collect(),
            },
            BlockPayload::Code { language, text } => Self::Code {
                language: language.clone(),
                text: text.clone(),
            },
            BlockPayload::Table(table) => Self::Table {
                rows: table.rows.iter().map(DbTableRow::from).collect(),
                columns: table.columns.iter().map(DbTableColumn::from).collect(),
                header_rows: table.header_rows,
                header_cols: table.header_cols,
                header_style: DbTableHeaderStyle::from(&table.header_style),
            },
            BlockPayload::Image(image) => Self::Image {
                source: image.source.clone(),
                alt: image.alt.clone(),
                caption: image.caption.clone(),
                display_width_ratio_milli: image.display_width_ratio_milli,
            },
            BlockPayload::File(file) => Self::File {
                name: file.name.clone(),
                source: file.source.clone(),
                size_bytes: file.size_bytes,
            },
            BlockPayload::Whiteboard(whiteboard) => Self::Whiteboard {
                scene_json: whiteboard.scene_json.clone(),
            },
            BlockPayload::Embed(embed) => Self::Embed {
                url: embed.url.clone(),
                title: embed.title.clone(),
            },
            BlockPayload::Html { html, sanitized } => Self::Html {
                html: html.clone(),
                sanitized: *sanitized,
            },
            BlockPayload::Empty => Self::Empty,
        }
    }
}

impl From<DbBlockPayload> for BlockPayload {
    fn from(payload: DbBlockPayload) -> Self {
        match payload {
            DbBlockPayload::RichText { spans } => Self::RichText {
                spans: spans.into_iter().map(InlineSpan::from).collect(),
            },
            DbBlockPayload::Code { language, text } => Self::Code { language, text },
            DbBlockPayload::Table {
                rows,
                columns,
                header_rows,
                header_cols,
                header_style,
            } => Self::Table(TablePayload {
                rows: rows.into_iter().map(TableRowPayload::from).collect(),
                columns: columns.into_iter().map(TableColumnPayload::from).collect(),
                header_rows,
                header_cols,
                header_style: TableHeaderStyle::from(header_style),
            }),
            DbBlockPayload::Image {
                source,
                alt,
                caption,
                display_width_ratio_milli,
            } => Self::Image(ImagePayload {
                source,
                alt,
                caption,
                display_width_ratio_milli,
            }),
            DbBlockPayload::File {
                name,
                source,
                size_bytes,
            } => Self::File(FilePayload {
                name,
                source,
                size_bytes,
            }),
            DbBlockPayload::Whiteboard { scene_json } => {
                Self::Whiteboard(WhiteboardPayload { scene_json })
            }
            DbBlockPayload::Embed { url, title } => Self::Embed(EmbedPayload { url, title }),
            DbBlockPayload::Html { html, sanitized } => Self::Html { html, sanitized },
            DbBlockPayload::Empty => Self::Empty,
        }
    }
}

impl From<&InlineSpan> for DbInlineSpan {
    fn from(span: &InlineSpan) -> Self {
        Self {
            text: span.text.clone(),
            marks: span.marks.iter().map(DbInlineMark::from).collect(),
        }
    }
}

impl From<DbInlineSpan> for InlineSpan {
    fn from(span: DbInlineSpan) -> Self {
        Self {
            text: span.text,
            marks: span.marks.into_iter().map(InlineMark::from).collect(),
        }
    }
}

impl From<&InlineMark> for DbInlineMark {
    fn from(mark: &InlineMark) -> Self {
        match mark {
            InlineMark::Bold => Self::Bold,
            InlineMark::Italic => Self::Italic,
            InlineMark::Underline => Self::Underline,
            InlineMark::Strike => Self::Strike,
            InlineMark::Code => Self::Code,
            InlineMark::Link { href } => Self::Link { href: href.clone() },
            InlineMark::Color(color) => Self::Color(color.clone()),
            InlineMark::Background(color) => Self::Background(color.clone()),
        }
    }
}

impl From<DbInlineMark> for InlineMark {
    fn from(mark: DbInlineMark) -> Self {
        match mark {
            DbInlineMark::Bold => Self::Bold,
            DbInlineMark::Italic => Self::Italic,
            DbInlineMark::Underline => Self::Underline,
            DbInlineMark::Strike => Self::Strike,
            DbInlineMark::Code => Self::Code,
            DbInlineMark::Link { href } => Self::Link { href },
            DbInlineMark::Color(color) => Self::Color(color),
            DbInlineMark::Background(color) => Self::Background(color),
        }
    }
}

impl From<&TableRowPayload> for DbTableRow {
    fn from(row: &TableRowPayload) -> Self {
        Self {
            cells: row.cells.iter().map(DbTableCell::from).collect(),
            height: DbTableTrackSize::from(row.height),
        }
    }
}

impl From<DbTableRow> for TableRowPayload {
    fn from(row: DbTableRow) -> Self {
        Self {
            cells: row.cells.into_iter().map(TableCellPayload::from).collect(),
            height: TableTrackSize::from(row.height),
        }
    }
}

impl From<&TableColumnPayload> for DbTableColumn {
    fn from(column: &TableColumnPayload) -> Self {
        Self {
            width: DbTableTrackSize::from(column.width),
        }
    }
}

impl From<DbTableColumn> for TableColumnPayload {
    fn from(column: DbTableColumn) -> Self {
        Self {
            width: TableTrackSize::from(column.width),
        }
    }
}

impl From<&TableCellPayload> for DbTableCell {
    fn from(cell: &TableCellPayload) -> Self {
        Self {
            spans: cell.spans.iter().map(DbInlineSpan::from).collect(),
            align: DbTableCellAlign::from(cell.align),
            merge: DbTableCellMerge::from(cell.merge),
            style: DbTableCellStyle::from(&cell.style),
        }
    }
}

impl From<DbTableCell> for TableCellPayload {
    fn from(cell: DbTableCell) -> Self {
        Self {
            spans: cell.spans.into_iter().map(InlineSpan::from).collect(),
            align: TableCellAlign::from(cell.align),
            merge: TableCellMerge::from(cell.merge),
            style: TableCellStyle::from(cell.style),
        }
    }
}

impl From<&TableCellStyle> for DbTableCellStyle {
    fn from(style: &TableCellStyle) -> Self {
        Self {
            background_color: style.background_color.clone(),
        }
    }
}

impl From<DbTableCellStyle> for TableCellStyle {
    fn from(style: DbTableCellStyle) -> Self {
        Self {
            background_color: style.background_color,
        }
    }
}

impl From<&TableHeaderStyle> for DbTableHeaderStyle {
    fn from(style: &TableHeaderStyle) -> Self {
        Self {
            row_background_color: style.row_background_color.clone(),
            column_background_color: style.column_background_color.clone(),
        }
    }
}

impl From<DbTableHeaderStyle> for TableHeaderStyle {
    fn from(style: DbTableHeaderStyle) -> Self {
        Self {
            row_background_color: style.row_background_color,
            column_background_color: style.column_background_color,
        }
    }
}

impl From<TableTrackSize> for DbTableTrackSize {
    fn from(size: TableTrackSize) -> Self {
        match size {
            TableTrackSize::Auto => Self::Auto,
            TableTrackSize::Px(px) => Self::Px(px),
        }
    }
}

impl From<DbTableTrackSize> for TableTrackSize {
    fn from(size: DbTableTrackSize) -> Self {
        match size {
            DbTableTrackSize::Auto => Self::Auto,
            DbTableTrackSize::Px(px) => Self::Px(px),
        }
    }
}

impl From<TableCellAlign> for DbTableCellAlign {
    fn from(align: TableCellAlign) -> Self {
        match align {
            TableCellAlign::Left => Self::Left,
            TableCellAlign::Center => Self::Center,
            TableCellAlign::Right => Self::Right,
        }
    }
}

impl From<DbTableCellAlign> for TableCellAlign {
    fn from(align: DbTableCellAlign) -> Self {
        match align {
            DbTableCellAlign::Left => Self::Left,
            DbTableCellAlign::Center => Self::Center,
            DbTableCellAlign::Right => Self::Right,
        }
    }
}

impl From<TableCellMerge> for DbTableCellMerge {
    fn from(merge: TableCellMerge) -> Self {
        match merge {
            TableCellMerge::Unmerged => Self::Unmerged,
            TableCellMerge::Origin { row_span, col_span } => Self::Origin { row_span, col_span },
            TableCellMerge::Covered {
                origin_row,
                origin_col,
            } => Self::Covered {
                origin_row,
                origin_col,
            },
        }
    }
}

impl From<DbTableCellMerge> for TableCellMerge {
    fn from(merge: DbTableCellMerge) -> Self {
        match merge {
            DbTableCellMerge::Unmerged => Self::Unmerged,
            DbTableCellMerge::Origin { row_span, col_span } => Self::Origin { row_span, col_span },
            DbTableCellMerge::Covered {
                origin_row,
                origin_col,
            } => Self::Covered {
                origin_row,
                origin_col,
            },
        }
    }
}

pub fn encode_block_payload(payload: &BlockPayload) -> serde_json::Result<serde_json::Value> {
    serde_json::to_value(DbBlockPayload::from(payload))
}

pub fn decode_block_payload(value: serde_json::Value) -> serde_json::Result<BlockPayload> {
    serde_json::from_value::<DbBlockPayload>(value).map(BlockPayload::from)
}
