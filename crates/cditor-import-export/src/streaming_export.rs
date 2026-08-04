use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cditor_core::rich_text::{
    BlockAttrs, BlockPayload, InlineMark, InlineSpan, RichBlockKind, RichBlockRecord,
    RichTextDocument,
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamingExportFormat {
    Markdown,
    Html,
    NativeJsonLines,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamingExportOptions {
    pub format: StreamingExportFormat,
    pub max_output_bytes: u64,
}

impl StreamingExportOptions {
    pub const fn new(format: StreamingExportFormat) -> Self {
        Self {
            format,
            max_output_bytes: 512 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportProgress {
    pub completed_blocks: usize,
    pub total_blocks: usize,
    pub bytes_written: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportWarning {
    pub block_id: Option<u64>,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamingExportReport {
    pub format: StreamingExportFormat,
    pub blocks: usize,
    pub bytes: u64,
    pub warnings: Vec<ExportWarning>,
}

impl StreamingExportReport {
    fn empty(format: StreamingExportFormat) -> Self {
        Self {
            format,
            blocks: 0,
            bytes: 0,
            warnings: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum StreamingExportError {
    Cancelled(StreamingExportReport),
    LimitExceeded(StreamingExportReport),
    Io(std::io::Error),
    Serialization(serde_json::Error),
}

impl std::fmt::Display for StreamingExportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled(_) => formatter.write_str("export cancelled"),
            Self::LimitExceeded(_) => formatter.write_str("export output limit exceeded"),
            Self::Io(error) => write!(formatter, "export I/O failed: {error}"),
            Self::Serialization(error) => write!(formatter, "native export failed: {error}"),
        }
    }
}

impl std::error::Error for StreamingExportError {}

#[derive(Debug, Clone, Default)]
pub struct ExportCancellation(Arc<AtomicBool>);

impl ExportCancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

pub fn export_document_streaming(
    document: &RichTextDocument,
    options: StreamingExportOptions,
    cancellation: &ExportCancellation,
    mut output: impl Write,
    mut progress: impl FnMut(ExportProgress),
) -> Result<StreamingExportReport, StreamingExportError> {
    let mut report = StreamingExportReport::empty(options.format);
    if options.format == StreamingExportFormat::NativeJsonLines {
        let header = NativeHeader {
            format: "cditor-native-jsonl",
            version: 1,
            document_id: document.id,
            document_version: document.version,
            structure_version: document.structure_version,
        };
        write_json_line(&mut output, &header, options, &mut report)?;
    }

    for (index, block) in document.blocks.iter().enumerate() {
        if cancellation.is_cancelled() {
            return Err(StreamingExportError::Cancelled(report));
        }
        if index > 0 && options.format != StreamingExportFormat::NativeJsonLines {
            write_bytes(&mut output, b"\n", options, &mut report)?;
        }
        append_capability_warning(block, options.format, &mut report.warnings);
        match options.format {
            StreamingExportFormat::Markdown => {
                append_markdown_warnings(block, &mut report.warnings);
                let markdown = crate::markdown::export::block_to_plain_markdown(block);
                write_bytes(&mut output, markdown.as_bytes(), options, &mut report)?;
            }
            StreamingExportFormat::Html => {
                let html = block_html(block, &mut report.warnings);
                write_bytes(&mut output, html.as_bytes(), options, &mut report)?;
            }
            StreamingExportFormat::NativeJsonLines => {
                write_json_line(&mut output, &NativeBlock::from(block), options, &mut report)?;
            }
        }
        report.blocks += 1;
        progress(ExportProgress {
            completed_blocks: report.blocks,
            total_blocks: document.blocks.len(),
            bytes_written: report.bytes,
        });
    }
    output.flush().map_err(StreamingExportError::Io)?;
    Ok(report)
}

fn write_json_line(
    output: &mut impl Write,
    value: &impl Serialize,
    options: StreamingExportOptions,
    report: &mut StreamingExportReport,
) -> Result<(), StreamingExportError> {
    let mut bytes = serde_json::to_vec(value).map_err(StreamingExportError::Serialization)?;
    bytes.push(b'\n');
    write_bytes(output, &bytes, options, report)
}

fn write_bytes(
    output: &mut impl Write,
    bytes: &[u8],
    options: StreamingExportOptions,
    report: &mut StreamingExportReport,
) -> Result<(), StreamingExportError> {
    let next = report.bytes.saturating_add(bytes.len() as u64);
    if next > options.max_output_bytes {
        return Err(StreamingExportError::LimitExceeded(report.clone()));
    }
    output.write_all(bytes).map_err(StreamingExportError::Io)?;
    report.bytes = next;
    Ok(())
}

fn append_capability_warning(
    block: &RichBlockRecord,
    format: StreamingExportFormat,
    warnings: &mut Vec<ExportWarning>,
) {
    if format == StreamingExportFormat::NativeJsonLines {
        return;
    }
    let capabilities = cditor_core::schema::builtin_block_registry()
        .descriptor_for_kind(&block.kind)
        .capabilities;
    let supported = match format {
        StreamingExportFormat::Markdown => capabilities.export_markdown,
        StreamingExportFormat::Html => capabilities.export_html,
        StreamingExportFormat::NativeJsonLines => true,
    };
    if !supported {
        warnings.push(ExportWarning {
            block_id: Some(block.id),
            code: "lossy_block_fallback",
            message: format!("{:?} used a plain-text export fallback", block.kind),
        });
    }
}

fn append_markdown_warnings(block: &RichBlockRecord, warnings: &mut Vec<ExportWarning>) {
    let unsupported_marks = match &block.payload {
        BlockPayload::RichText { spans } => spans.iter().any(|span| {
            span.marks.iter().any(|mark| {
                matches!(
                    mark,
                    InlineMark::Underline | InlineMark::Color(_) | InlineMark::Background(_)
                )
            })
        }),
        _ => false,
    };
    if unsupported_marks {
        warnings.push(ExportWarning {
            block_id: Some(block.id),
            code: "markdown_mark_fallback",
            message: "underline or color marks are not representable in portable Markdown"
                .to_owned(),
        });
    }
}

fn block_html(block: &RichBlockRecord, warnings: &mut Vec<ExportWarning>) -> String {
    let text = html_payload(&block.payload);
    match &block.kind {
        RichBlockKind::Heading { level } => format!("<h{level}>{text}</h{level}>"),
        RichBlockKind::BulletedList => format!("<ul><li>{text}</li></ul>"),
        RichBlockKind::NumberedList => format!("<ol><li>{text}</li></ol>"),
        RichBlockKind::Todo { checked } => format!(
            "<div data-cditor-todo=\"{}\">{text}</div>",
            if *checked { "checked" } else { "unchecked" }
        ),
        RichBlockKind::Quote | RichBlockKind::Callout { .. } => {
            format!("<blockquote>{text}</blockquote>")
        }
        RichBlockKind::Code { language } => format!(
            "<pre><code data-language=\"{}\">{}</code></pre>",
            escape_html(language.as_deref().unwrap_or_default()),
            escape_html(&block.payload.plain_text())
        ),
        RichBlockKind::Separator | RichBlockKind::Divider => "<hr>".to_owned(),
        RichBlockKind::Table => table_html(&block.payload),
        RichBlockKind::Html => match &block.payload {
            BlockPayload::Html {
                html,
                sanitized: true,
            } => html.clone(),
            BlockPayload::Html { html, .. } => {
                warnings.push(ExportWarning {
                    block_id: Some(block.id),
                    code: "unsanitized_html_escaped",
                    message: "unsanitized HTML was exported as escaped source".to_owned(),
                });
                format!("<pre data-cditor-html-source>{}</pre>", escape_html(html))
            }
            _ => format!("<p>{text}</p>"),
        },
        RichBlockKind::RawMarkdown => format!(
            "<pre data-cditor-raw-markdown>{}</pre>",
            escape_html(block.raw_fallback.as_deref().unwrap_or_default())
        ),
        _ => format!("<p>{text}</p>"),
    }
}

fn html_payload(payload: &BlockPayload) -> String {
    match payload {
        BlockPayload::RichText { spans } => spans_html(spans),
        _ => escape_html(&payload.plain_text()),
    }
}

fn spans_html(spans: &[InlineSpan]) -> String {
    spans
        .iter()
        .map(|span| {
            let mut text = escape_html(&span.text);
            for mark in span.marks.iter().rev() {
                text = match mark {
                    InlineMark::Bold => format!("<strong>{text}</strong>"),
                    InlineMark::Italic => format!("<em>{text}</em>"),
                    InlineMark::Underline => format!("<u>{text}</u>"),
                    InlineMark::Strike => format!("<s>{text}</s>"),
                    InlineMark::Code => format!("<code>{text}</code>"),
                    InlineMark::Link { href } | InlineMark::DocumentLink { href } => {
                        format!("<a href=\"{}\">{text}</a>", escape_html(href))
                    }
                    InlineMark::Color(color) => format!(
                        "<span data-cditor-color=\"{}\">{text}</span>",
                        escape_html(color)
                    ),
                    InlineMark::Background(color) => format!(
                        "<span data-cditor-background=\"{}\">{text}</span>",
                        escape_html(color)
                    ),
                };
            }
            text
        })
        .collect()
}

fn table_html(payload: &BlockPayload) -> String {
    let BlockPayload::Table(table) = payload else {
        return String::new();
    };
    let mut output = String::from("<table>");
    for (row_index, row) in table.rows.iter().enumerate() {
        output.push_str("<tr>");
        for cell in &row.cells {
            let tag = if row_index < table.header_rows {
                "th"
            } else {
                "td"
            };
            output.push_str(&format!("<{tag}>{}</{tag}>", spans_html(&cell.spans)));
        }
        output.push_str("</tr>");
    }
    output.push_str("</table>");
    output
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[derive(Serialize)]
struct NativeHeader {
    format: &'static str,
    version: u32,
    document_id: u64,
    document_version: u32,
    structure_version: u64,
}

#[derive(Serialize)]
struct NativeBlock<'a> {
    record: &'static str,
    id: u64,
    parent_id: Option<u64>,
    depth: u16,
    kind: &'a RichBlockKind,
    attrs: &'a BlockAttrs,
    payload: &'a BlockPayload,
    content_version: u64,
    structure_version: u64,
    raw_fallback: &'a Option<String>,
}

impl<'a> From<&'a RichBlockRecord> for NativeBlock<'a> {
    fn from(block: &'a RichBlockRecord) -> Self {
        Self {
            record: "block",
            id: block.id,
            parent_id: block.parent_id,
            depth: block.depth,
            kind: &block.kind,
            attrs: &block.attrs,
            payload: &block.payload,
            content_version: block.content_version,
            structure_version: block.structure_version,
            raw_fallback: &block.raw_fallback,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::rich_text::RichBlockRecord;

    fn document() -> RichTextDocument {
        let mut document = RichTextDocument::empty(7);
        document.push_root_block(RichBlockRecord::heading(1, 2, "Title <one>"));
        document.push_root_block(RichBlockRecord::raw_markdown(2, "<custom raw>"));
        document.push_root_block(RichBlockRecord::new(
            3,
            RichBlockKind::Html,
            BlockPayload::Html {
                html: "<script>unsafe</script>".to_owned(),
                sanitized: false,
            },
        ));
        document
    }

    #[test]
    fn markdown_html_and_native_stream_without_whole_document_buffers() {
        for format in [
            StreamingExportFormat::Markdown,
            StreamingExportFormat::Html,
            StreamingExportFormat::NativeJsonLines,
        ] {
            let mut bytes = Vec::new();
            let mut progress = Vec::new();
            let report = export_document_streaming(
                &document(),
                StreamingExportOptions::new(format),
                &ExportCancellation::default(),
                &mut bytes,
                |item| progress.push(item),
            )
            .unwrap();
            assert_eq!(report.blocks, 3);
            assert_eq!(report.bytes, bytes.len() as u64);
            assert_eq!(progress.len(), 3);
            assert_eq!(progress.last().unwrap().completed_blocks, 3);
            let text = String::from_utf8(bytes).unwrap();
            match format {
                StreamingExportFormat::Markdown => assert!(text.contains("## Title <one>")),
                StreamingExportFormat::Html => {
                    assert!(text.contains("<h2>Title &lt;one&gt;</h2>"));
                    assert!(text.contains("&lt;script&gt;unsafe&lt;/script&gt;"));
                    assert!(
                        report
                            .warnings
                            .iter()
                            .any(|warning| { warning.code == "unsanitized_html_escaped" })
                    );
                }
                StreamingExportFormat::NativeJsonLines => {
                    let lines = text.lines().collect::<Vec<_>>();
                    assert_eq!(lines.len(), 4);
                    assert_eq!(
                        serde_json::from_str::<serde_json::Value>(lines[0]).unwrap()["format"],
                        "cditor-native-jsonl"
                    );
                    assert!(lines[2].contains("<custom raw>"));
                }
            }
        }
    }

    #[test]
    fn cancellation_and_output_limit_return_partial_typed_reports() {
        let cancellation = ExportCancellation::default();
        let callback_cancellation = cancellation.clone();
        let error = export_document_streaming(
            &document(),
            StreamingExportOptions::new(StreamingExportFormat::Markdown),
            &cancellation,
            Vec::new(),
            move |progress| {
                if progress.completed_blocks == 2 {
                    callback_cancellation.cancel();
                }
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            StreamingExportError::Cancelled(StreamingExportReport { blocks: 2, .. })
        ));

        let error = export_document_streaming(
            &document(),
            StreamingExportOptions {
                format: StreamingExportFormat::NativeJsonLines,
                max_output_bytes: 8,
            },
            &ExportCancellation::default(),
            Vec::new(),
            |_| {},
        )
        .unwrap_err();
        assert!(matches!(
            error,
            StreamingExportError::LimitExceeded(StreamingExportReport { blocks: 0, .. })
        ));
    }

    #[test]
    fn markdown_preserves_portable_marks_and_warns_for_nonportable_marks() {
        let mut document = RichTextDocument::empty(9);
        document.push_root_block(RichBlockRecord::new(
            1,
            RichBlockKind::Paragraph,
            BlockPayload::RichText {
                spans: vec![
                    InlineSpan {
                        text: "bold".to_owned(),
                        marks: vec![InlineMark::Bold],
                    },
                    InlineSpan {
                        text: " link".to_owned(),
                        marks: vec![
                            InlineMark::Italic,
                            InlineMark::Link {
                                href: "https://example.com/a_(b)".to_owned(),
                            },
                        ],
                    },
                    InlineSpan {
                        text: " color".to_owned(),
                        marks: vec![InlineMark::Color("red".to_owned())],
                    },
                ],
            },
        ));
        let mut output = Vec::new();
        let report = export_document_streaming(
            &document,
            StreamingExportOptions::new(StreamingExportFormat::Markdown),
            &ExportCancellation::default(),
            &mut output,
            |_| {},
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "**bold**[_ link_](https://example.com/a_\\(b\\)) color"
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.code == "markdown_mark_fallback")
        );
    }
}
