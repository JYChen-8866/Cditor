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
        header_rows: usize,
        header_cols: usize,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbTableCell {
    pub spans: Vec<DbInlineSpan>,
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
                header_rows: table.header_rows,
                header_cols: table.header_cols,
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
                header_rows,
                header_cols,
            } => Self::Table(TablePayload {
                rows: rows.into_iter().map(TableRowPayload::from).collect(),
                header_rows,
                header_cols,
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
        }
    }
}

impl From<DbTableRow> for TableRowPayload {
    fn from(row: DbTableRow) -> Self {
        Self {
            cells: row.cells.into_iter().map(TableCellPayload::from).collect(),
        }
    }
}

impl From<&TableCellPayload> for DbTableCell {
    fn from(cell: &TableCellPayload) -> Self {
        Self {
            spans: cell.spans.iter().map(DbInlineSpan::from).collect(),
        }
    }
}

impl From<DbTableCell> for TableCellPayload {
    fn from(cell: DbTableCell) -> Self {
        Self {
            spans: cell.spans.into_iter().map(InlineSpan::from).collect(),
        }
    }
}

pub fn encode_block_payload(payload: &BlockPayload) -> serde_json::Result<serde_json::Value> {
    serde_json::to_value(DbBlockPayload::from(payload))
}

pub fn decode_block_payload(value: serde_json::Value) -> serde_json::Result<BlockPayload> {
    serde_json::from_value::<DbBlockPayload>(value).map(BlockPayload::from)
}
