#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    CditorJson,
    PlainText,
    Html,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentExportMode {
    Reference,
    Copy,
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownImportOptions {
    pub replace_document: bool,
    pub preserve_unknown_blocks: bool,
}

impl Default for MarkdownImportOptions {
    fn default() -> Self {
        Self {
            replace_document: false,
            preserve_unknown_blocks: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownExportOptions {
    pub include_frontmatter: bool,
    pub attachment_mode: AttachmentExportMode,
}

impl Default for MarkdownExportOptions {
    fn default() -> Self {
        Self {
            include_frontmatter: false,
            attachment_mode: AttachmentExportMode::Reference,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportWarning {
    pub block_index: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportWarning {
    pub block_id: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub inserted_blocks: usize,
    pub warnings: Vec<ImportWarning>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportReport {
    pub blocks: usize,
    pub bytes: u64,
    pub warnings: Vec<ExportWarning>,
}
