use cditor_core::clipboard::CditorClipboardEnvelope;
use cditor_core::ids::BlockId;
use cditor_core::import_plan::{
    ClipboardImportContent, ImportContent, ImportDiagnostic, ImportDiagnosticSeverity,
    ImportLimits, ImportPlan, ImportReport, ImportSource, ImportTarget, ImportedBlockDocument,
};
use cditor_core::rich_text::{
    BlockPayload, InlineMark, InlineSpan, TableCellPayload, TablePayload, TableRowPayload,
    TableTrackSize,
};

use crate::markdown::{MarkdownImportOptions, looks_like_markdown_paste, parse_markdown_document};

pub fn plan_clipboard_import(
    system_text: &str,
    metadata_json: Option<&str>,
    target: ImportTarget,
    first_block_id: BlockId,
    limits: ImportLimits,
) -> ImportPlan {
    let text = normalize_line_endings(system_text);
    let mut report = ImportReport {
        input_bytes: system_text.len() + metadata_json.map_or(0, str::len),
        ..ImportReport::default()
    };
    if report.input_bytes > limits.max_input_bytes {
        reject(
            &mut report,
            "input_bytes_exceeded",
            "clipboard input exceeds byte limit",
        );
    }

    let internal_selection = metadata_json.and_then(|json| {
        match CditorClipboardEnvelope::decode_metadata(json, system_text) {
            Ok(envelope) => Some(envelope.selection),
            Err(error) => {
                report.diagnostics.push(ImportDiagnostic {
                    code: "clipboard_metadata_rejected",
                    severity: ImportDiagnosticSeverity::Warning,
                    message: format!("clipboard metadata was ignored: {error:?}"),
                });
                None
            }
        }
    });
    let delimited_table = parse_delimited_table(&text);
    let markdown = looks_like_markdown_paste(&text).then(|| {
        let parsed = parse_markdown_document(
            &text,
            MarkdownImportOptions {
                document_id: target.document_id,
                first_block_id,
            },
        );
        ImportedBlockDocument {
            root_blocks: parsed.root_blocks,
            blocks: parsed.blocks,
        }
    });

    accumulate_content_counts(
        internal_selection.as_ref(),
        delimited_table.as_ref(),
        markdown.as_ref(),
        &mut report,
    );
    enforce_limits(&limits, &mut report);
    ImportPlan::new(
        ImportSource::Clipboard,
        target,
        limits,
        ImportContent::Clipboard(Box::new(ClipboardImportContent {
            internal_selection,
            delimited_table,
            markdown,
            plain_text: text,
        })),
        report,
    )
}

pub fn plan_markdown_import(
    markdown: &str,
    source: ImportSource,
    target: ImportTarget,
    first_block_id: BlockId,
    limits: ImportLimits,
) -> ImportPlan {
    let parsed = parse_markdown_document(
        markdown,
        MarkdownImportOptions {
            document_id: target.document_id,
            first_block_id,
        },
    );
    let document = ImportedBlockDocument {
        root_blocks: parsed.root_blocks,
        blocks: parsed.blocks,
    };
    let mut report = ImportReport {
        input_bytes: markdown.len(),
        block_count: document.blocks.len(),
        span_count: document
            .blocks
            .iter()
            .map(|block| payload_span_count(&block.payload))
            .sum(),
        max_depth: document
            .blocks
            .iter()
            .map(|block| block.depth)
            .max()
            .unwrap_or(0),
        table_cell_count: document
            .blocks
            .iter()
            .filter_map(|block| match &block.payload {
                BlockPayload::Table(table) => Some(table),
                _ => None,
            })
            .flat_map(|table| &table.rows)
            .map(|row| row.cells.len())
            .sum(),
        media_count: document
            .blocks
            .iter()
            .filter(|block| {
                matches!(
                    block.payload,
                    BlockPayload::Image(_) | BlockPayload::File(_)
                )
            })
            .count(),
        ..ImportReport::default()
    };
    if report.input_bytes > limits.max_input_bytes {
        reject(
            &mut report,
            "input_bytes_exceeded",
            "markdown input exceeds byte limit",
        );
    }
    reject_unsafe_document_resources(&document, &mut report);
    enforce_limits(&limits, &mut report);
    ImportPlan::new(
        source,
        target,
        limits,
        ImportContent::Markdown(document),
        report,
    )
}

fn accumulate_content_counts(
    selection: Option<&cditor_core::clipboard::ClipboardSelection>,
    table: Option<&TablePayload>,
    markdown: Option<&ImportedBlockDocument>,
    report: &mut ImportReport,
) {
    if let Some(document) = markdown {
        report.block_count = document.blocks.len();
        report.span_count = document
            .blocks
            .iter()
            .map(|block| payload_span_count(&block.payload))
            .sum();
        report.max_depth = document
            .blocks
            .iter()
            .map(|block| block.depth)
            .max()
            .unwrap_or(0);
        report.table_cell_count = document
            .blocks
            .iter()
            .filter_map(|block| match &block.payload {
                BlockPayload::Table(table) => Some(table),
                _ => None,
            })
            .flat_map(|table| &table.rows)
            .map(|row| row.cells.len())
            .sum();
        report.media_count = document
            .blocks
            .iter()
            .filter(|block| {
                matches!(
                    block.payload,
                    BlockPayload::Image(_) | BlockPayload::File(_)
                )
            })
            .count();
        reject_unsafe_document_resources(document, report);
    }
    if let Some(table) = table {
        report.table_cell_count = table.rows.iter().map(|row| row.cells.len()).sum::<usize>();
    }
    if let Some(selection) = selection {
        let text = selection.plain_text();
        report.span_count = report.span_count.max(text.lines().count());
    }
}

fn reject_unsafe_document_resources(document: &ImportedBlockDocument, report: &mut ImportReport) {
    let unsafe_count = document
        .blocks
        .iter()
        .map(|block| unsafe_payload_resource_count(&block.payload))
        .sum::<usize>();
    if unsafe_count > 0 {
        report.sanitized_resources = report.sanitized_resources.saturating_add(unsafe_count);
        reject(
            report,
            "unsafe_resource_rejected",
            "import contains an unsafe external resource",
        );
    }
}

fn unsafe_payload_resource_count(payload: &BlockPayload) -> usize {
    match payload {
        BlockPayload::RichText { spans } => unsafe_span_resource_count(spans),
        BlockPayload::Table(table) => table
            .rows
            .iter()
            .flat_map(|row| &row.cells)
            .map(|cell| unsafe_span_resource_count(&cell.spans))
            .sum(),
        BlockPayload::Image(image) => {
            usize::from(!safe_resource(&image.source))
                + unsafe_span_resource_count(&image.caption.spans)
        }
        BlockPayload::Collection(collection) => unsafe_span_resource_count(&collection.title.spans),
        BlockPayload::File(file) => usize::from(!safe_resource(&file.source)),
        BlockPayload::Embed(embed) => usize::from(!safe_resource(&embed.url)),
        _ => 0,
    }
}

fn unsafe_span_resource_count(spans: &[InlineSpan]) -> usize {
    spans
        .iter()
        .flat_map(|span| &span.marks)
        .filter(|mark| matches!(mark, InlineMark::Link { href } if !safe_resource(href)))
        .count()
}

fn safe_resource(value: &str) -> bool {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    value.is_empty()
        || (!value.contains('\0')
            && !lower.starts_with("javascript:")
            && !lower.starts_with("data:text/html")
            && !value.split(['/', '\\']).any(|part| part == ".."))
}

fn payload_span_count(payload: &BlockPayload) -> usize {
    match payload {
        BlockPayload::RichText { spans } => spans.len(),
        BlockPayload::Code { .. } => 1,
        BlockPayload::Table(table) => table
            .rows
            .iter()
            .flat_map(|row| &row.cells)
            .map(|cell| cell.spans.len())
            .sum(),
        _ => 0,
    }
}

fn enforce_limits(limits: &ImportLimits, report: &mut ImportReport) {
    for (exceeded, code, message) in [
        (
            report.block_count > limits.max_blocks,
            "block_limit_exceeded",
            "block limit exceeded",
        ),
        (
            report.span_count > limits.max_spans,
            "span_limit_exceeded",
            "span limit exceeded",
        ),
        (
            report.max_depth > limits.max_depth,
            "depth_limit_exceeded",
            "block depth limit exceeded",
        ),
        (
            report.table_cell_count > limits.max_table_cells,
            "table_cell_limit_exceeded",
            "table cell limit exceeded",
        ),
        (
            report.media_count > limits.max_media,
            "media_limit_exceeded",
            "media limit exceeded",
        ),
    ] {
        if exceeded {
            reject(report, code, message);
        }
    }
}

fn reject(report: &mut ImportReport, code: &'static str, message: &str) {
    report.diagnostics.push(ImportDiagnostic {
        code,
        severity: ImportDiagnosticSeverity::Error,
        message: message.to_owned(),
    });
}

fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

pub fn parse_delimited_table(text: &str) -> Option<TablePayload> {
    let text = text.trim_end_matches(['\r', '\n']);
    if text.is_empty() || !(text.contains('\t') || text.contains(',') || text.contains('\n')) {
        return None;
    }
    let rows = if text.contains('\t') {
        text.lines()
            .map(|line| line.trim_end_matches('\r'))
            .map(|line| line.split('\t').map(str::to_owned).collect())
            .collect::<Vec<Vec<String>>>()
    } else {
        parse_csv_rows(text)
    };
    let rows = rows
        .into_iter()
        .filter(|row| !row.is_empty())
        .map(|row| TableRowPayload {
            cells: row.into_iter().map(TableCellPayload::plain).collect(),
            height: TableTrackSize::Auto,
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return None;
    }
    let mut table = TablePayload {
        rows,
        ..TablePayload::default()
    };
    table.normalize();
    Some(table)
}

fn parse_csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => row.push(std::mem::take(&mut cell)),
            '\n' if !quoted => {
                row.push(cell.trim_end_matches('\r').to_owned());
                cell.clear();
                rows.push(std::mem::take(&mut row));
            }
            _ => cell.push(ch),
        }
    }
    row.push(cell.trim_end_matches('\r').to_owned());
    rows.push(row);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delimited_table_parser_handles_tsv_csv_quotes_and_plain_text() {
        let tsv = parse_delimited_table("A\tB\nC\tD").unwrap();
        assert_eq!(tsv.cell_plain_text(1, 1).as_deref(), Some("D"));
        let csv = parse_delimited_table("\"x,y\",z").unwrap();
        assert_eq!(csv.cell_plain_text(0, 0).as_deref(), Some("x,y"));
        assert_eq!(csv.cell_plain_text(0, 1).as_deref(), Some("z"));
        assert!(parse_delimited_table("plain").is_none());
    }

    #[test]
    fn planning_rejects_input_before_dispatch_when_limits_are_exceeded() {
        let plan = plan_markdown_import(
            "# title",
            ImportSource::Markdown,
            ImportTarget {
                document_id: 1,
                expected_revision: 0,
                focused_block_id: Some(1),
            },
            2,
            ImportLimits {
                max_input_bytes: 2,
                ..ImportLimits::default()
            },
        );
        assert!(plan.report().rejected());
        assert!(
            plan.report()
                .diagnostics
                .iter()
                .any(|item| item.code == "input_bytes_exceeded")
        );
    }

    #[test]
    fn planning_rejects_unsafe_markdown_links_before_runtime_dispatch() {
        let plan = plan_markdown_import(
            "[unsafe](javascript:alert)",
            ImportSource::Markdown,
            ImportTarget {
                document_id: 1,
                expected_revision: 0,
                focused_block_id: Some(1),
            },
            2,
            ImportLimits::default(),
        );
        assert!(plan.report().rejected());
        assert!(
            plan.report()
                .diagnostics
                .iter()
                .any(|item| item.code == "unsafe_resource_rejected")
        );
    }

    #[test]
    fn planning_accepts_ten_thousand_typed_markdown_blocks_within_default_limits() {
        let markdown = (0..10_000)
            .map(|index| format!("- item {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let plan = plan_markdown_import(
            &markdown,
            ImportSource::Markdown,
            ImportTarget {
                document_id: 1,
                expected_revision: 0,
                focused_block_id: Some(1),
            },
            2,
            ImportLimits::default(),
        );
        assert!(!plan.report().rejected());
        assert_eq!(plan.report().block_count, 10_000);
        assert!(matches!(
            plan.content(),
            ImportContent::Markdown(document) if document.blocks.len() == 10_000
        ));
    }
}
