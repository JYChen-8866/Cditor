use crate::clipboard::ClipboardSelection;
use crate::ids::{BlockId, DocumentId};
use crate::rich_text::{RichBlockRecord, TablePayload};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportSource {
    Clipboard,
    Ai,
    Markdown,
    Html,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportTarget {
    pub document_id: DocumentId,
    pub expected_revision: u64,
    pub focused_block_id: Option<BlockId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportLimits {
    pub max_input_bytes: usize,
    pub max_blocks: usize,
    pub max_spans: usize,
    pub max_depth: u16,
    pub max_table_cells: usize,
    pub max_media: usize,
}

impl Default for ImportLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 8 * 1024 * 1024,
            max_blocks: 100_000,
            max_spans: 1_000_000,
            max_depth: 64,
            max_table_cells: 1_000_000,
            max_media: 10_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDiagnostic {
    pub code: &'static str,
    pub severity: ImportDiagnosticSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub input_bytes: usize,
    pub block_count: usize,
    pub span_count: usize,
    pub max_depth: u16,
    pub table_cell_count: usize,
    pub media_count: usize,
    pub sanitized_resources: usize,
    pub diagnostics: Vec<ImportDiagnostic>,
}

impl ImportReport {
    pub fn rejected(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|item| item.severity == ImportDiagnosticSeverity::Error)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportedBlockDocument {
    pub root_blocks: Vec<BlockId>,
    pub blocks: Vec<RichBlockRecord>,
}

impl ImportedBlockDocument {
    pub fn push_root_block(&mut self, block: RichBlockRecord) -> BlockId {
        let id = block.id;
        self.root_blocks.push(id);
        self.blocks.push(block);
        id
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClipboardImportContent {
    pub internal_selection: Option<ClipboardSelection>,
    pub delimited_table: Option<TablePayload>,
    pub markdown: Option<ImportedBlockDocument>,
    pub plain_text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportContent {
    Clipboard(Box<ClipboardImportContent>),
    Markdown(ImportedBlockDocument),
    PlainText(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportPlan {
    source: ImportSource,
    target: ImportTarget,
    limits: ImportLimits,
    content: ImportContent,
    report: ImportReport,
}

impl ImportPlan {
    pub fn new(
        source: ImportSource,
        target: ImportTarget,
        limits: ImportLimits,
        content: ImportContent,
        report: ImportReport,
    ) -> Self {
        Self {
            source,
            target,
            limits,
            content,
            report,
        }
    }

    pub fn source(&self) -> ImportSource {
        self.source
    }

    pub fn target(&self) -> ImportTarget {
        self.target
    }

    pub fn limits(&self) -> ImportLimits {
        self.limits
    }

    pub fn content(&self) -> &ImportContent {
        &self.content
    }

    pub fn report(&self) -> &ImportReport {
        &self.report
    }
}
