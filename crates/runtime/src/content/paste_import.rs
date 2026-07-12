use crate::{MediaResourceId, MediaStableBox};
use cditor_core::document::BlockIndexRecord;
use cditor_core::edit::EditTransaction;
use cditor_core::ids::BlockId;
use cditor_core::layout::{BlockLayoutMeta, HeightConfidence, HeightEstimate};

const KIND_PARAGRAPH: u16 = 1;
const KIND_IMAGE: u16 = 13;
const PARAGRAPH_ESTIMATED_HEIGHT: f64 = 32.0;
const IMAGE_PLACEHOLDER_HEIGHT: f64 = 240.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardInput {
    PlainText(String),
    Markdown(String),
    Html(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasteImportConfig {
    pub visible_hydration_limit: usize,
    pub persist_batch_blocks: usize,
}

impl Default for PasteImportConfig {
    fn default() -> Self {
        Self {
            visible_hydration_limit: 64,
            persist_batch_blocks: 512,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PastePipelinePhase {
    ClipboardInput,
    Parsed,
    Sanitized,
    NormalizedBlocks,
    AllocatedBlockIds,
    EstimatedLayoutHeight,
    BatchInsertedDocumentIndex,
    VisibleBlocksHydrated,
    AsyncPersistScheduled,
    AsyncMediaMetadataScheduled,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasteRunOptions {
    pub cancel_after_phase: Option<PastePipelinePhase>,
}

impl PasteRunOptions {
    pub const fn normal() -> Self {
        Self {
            cancel_after_phase: None,
        }
    }

    pub const fn cancel_after(phase: PastePipelinePhase) -> Self {
        Self {
            cancel_after_phase: Some(phase),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteProgress {
    pub phase: PastePipelinePhase,
    pub parsed_blocks: usize,
    pub normalized_blocks: usize,
    pub allocated_blocks: usize,
    pub inserted_blocks: usize,
    pub hydrated_visible_blocks: usize,
    pub persisted_payloads: usize,
    pub media_metadata_tasks: usize,
    pub cancelled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedPasteBlock {
    pub text: String,
    pub kind_tag: u16,
    pub estimated_height: HeightEstimate,
    pub media: Option<PendingMediaResource>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingMediaResource {
    pub resource_id: MediaResourceId,
    pub source: String,
    pub stable_box: MediaStableBox,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadPersistTask {
    pub block_ids: Vec<BlockId>,
    pub async_only: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaMetadataTask {
    pub resource_id: MediaResourceId,
    pub block_id: BlockId,
    pub source: String,
    pub stable_box: MediaStableBox,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PasteImportResult {
    pub transaction: Option<EditTransaction>,
    pub records: Vec<BlockIndexRecord>,
    pub visible_hydrated_blocks: Vec<BlockId>,
    pub persist_tasks: Vec<PayloadPersistTask>,
    pub media_metadata_tasks: Vec<MediaMetadataTask>,
    pub progress: PasteProgress,
    pub progress_log: Vec<PasteProgress>,
}

impl PasteImportResult {
    pub fn cancelled(&self) -> bool {
        self.progress.cancelled
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PasteImportPipeline {
    config: PasteImportConfig,
    next_block_id: BlockId,
    next_media_id: u64,
}

impl PasteImportPipeline {
    pub fn new(config: PasteImportConfig, next_block_id: BlockId) -> Self {
        Self {
            config,
            next_block_id,
            next_media_id: 1,
        }
    }

    pub fn run(
        &mut self,
        input: ClipboardInput,
        insert_index: usize,
        transaction_id: u64,
        timestamp: u64,
        options: PasteRunOptions,
    ) -> PasteImportResult {
        let mut progress = PasteProgress::new(PastePipelinePhase::ClipboardInput);
        let mut log = vec![progress.clone()];
        if should_cancel(options, progress.phase) {
            return cancelled_result(progress, log);
        }

        let parsed = parse_input(input);
        progress.phase = PastePipelinePhase::Parsed;
        progress.parsed_blocks = parsed.text_blocks.len() + parsed.image_sources.len();
        log.push(progress.clone());
        if should_cancel(options, progress.phase) {
            return cancelled_result(progress, log);
        }

        let sanitized = sanitize_parsed(parsed);
        progress.phase = PastePipelinePhase::Sanitized;
        progress.parsed_blocks = sanitized.text_blocks.len() + sanitized.image_sources.len();
        log.push(progress.clone());
        if should_cancel(options, progress.phase) {
            return cancelled_result(progress, log);
        }

        let normalized = self.normalize_blocks(sanitized);
        progress.phase = PastePipelinePhase::NormalizedBlocks;
        progress.normalized_blocks = normalized.len();
        log.push(progress.clone());
        if should_cancel(options, progress.phase) {
            return cancelled_result(progress, log);
        }

        let records = self.allocate_records(&normalized);
        progress.phase = PastePipelinePhase::AllocatedBlockIds;
        progress.allocated_blocks = records.len();
        log.push(progress.clone());
        if should_cancel(options, progress.phase) {
            return cancelled_result_with_records(progress, log, records);
        }

        progress.phase = PastePipelinePhase::EstimatedLayoutHeight;
        log.push(progress.clone());
        if should_cancel(options, progress.phase) {
            return cancelled_result_with_records(progress, log, records);
        }

        let transaction =
            EditTransaction::paste_blocks(transaction_id, timestamp, insert_index, records.clone());
        progress.phase = PastePipelinePhase::BatchInsertedDocumentIndex;
        progress.inserted_blocks = records.len();
        log.push(progress.clone());
        if should_cancel(options, progress.phase) {
            return PasteImportResult {
                transaction: None,
                records,
                visible_hydrated_blocks: Vec::new(),
                persist_tasks: Vec::new(),
                media_metadata_tasks: Vec::new(),
                progress: progress.cancelled(),
                progress_log: log,
            };
        }

        let visible_hydrated_blocks = records
            .iter()
            .take(self.config.visible_hydration_limit)
            .map(|record| record.id)
            .collect::<Vec<_>>();
        progress.phase = PastePipelinePhase::VisibleBlocksHydrated;
        progress.hydrated_visible_blocks = visible_hydrated_blocks.len();
        log.push(progress.clone());
        if should_cancel(options, progress.phase) {
            return PasteImportResult {
                transaction: Some(transaction),
                records,
                visible_hydrated_blocks,
                persist_tasks: Vec::new(),
                media_metadata_tasks: Vec::new(),
                progress: progress.cancelled(),
                progress_log: log,
            };
        }

        let persist_tasks = build_persist_tasks(&records, self.config.persist_batch_blocks);
        progress.phase = PastePipelinePhase::AsyncPersistScheduled;
        progress.persisted_payloads = records.len();
        log.push(progress.clone());
        if should_cancel(options, progress.phase) {
            return PasteImportResult {
                transaction: Some(transaction),
                records,
                visible_hydrated_blocks,
                persist_tasks,
                media_metadata_tasks: Vec::new(),
                progress: progress.cancelled(),
                progress_log: log,
            };
        }

        let media_metadata_tasks = normalized
            .iter()
            .zip(records.iter())
            .filter_map(|(block, record)| {
                block.media.as_ref().map(|media| MediaMetadataTask {
                    resource_id: media.resource_id,
                    block_id: record.id,
                    source: media.source.clone(),
                    stable_box: media.stable_box,
                })
            })
            .collect::<Vec<_>>();
        progress.phase = PastePipelinePhase::AsyncMediaMetadataScheduled;
        progress.media_metadata_tasks = media_metadata_tasks.len();
        log.push(progress.clone());
        if should_cancel(options, progress.phase) {
            return PasteImportResult {
                transaction: Some(transaction),
                records,
                visible_hydrated_blocks,
                persist_tasks,
                media_metadata_tasks,
                progress: progress.cancelled(),
                progress_log: log,
            };
        }

        progress.phase = PastePipelinePhase::Completed;
        log.push(progress.clone());
        PasteImportResult {
            transaction: Some(transaction),
            records,
            visible_hydrated_blocks,
            persist_tasks,
            media_metadata_tasks,
            progress,
            progress_log: log,
        }
    }

    fn normalize_blocks(&mut self, parsed: ParsedPasteInput) -> Vec<NormalizedPasteBlock> {
        let mut blocks = Vec::with_capacity(parsed.text_blocks.len() + parsed.image_sources.len());
        for text in parsed.text_blocks {
            blocks.push(NormalizedPasteBlock {
                text,
                kind_tag: KIND_PARAGRAPH,
                estimated_height: HeightEstimate::new(
                    PARAGRAPH_ESTIMATED_HEIGHT,
                    HeightConfidence::Predictive,
                    8.0,
                ),
                media: None,
            });
        }
        for source in parsed.image_sources {
            let resource_id = MediaResourceId(self.next_media_id);
            self.next_media_id = self.next_media_id.saturating_add(1);
            let stable_box = MediaStableBox {
                estimated_height: IMAGE_PLACEHOLDER_HEIGHT,
                min_height: 120.0,
                max_height: 480.0,
            };
            blocks.push(NormalizedPasteBlock {
                text: String::new(),
                kind_tag: KIND_IMAGE,
                estimated_height: HeightEstimate::new(
                    stable_box.estimated_height,
                    HeightConfidence::Predictive,
                    stable_box.max_height - stable_box.min_height,
                ),
                media: Some(PendingMediaResource {
                    resource_id,
                    source,
                    stable_box,
                }),
            });
        }
        blocks
    }

    fn allocate_records(&mut self, blocks: &[NormalizedPasteBlock]) -> Vec<BlockIndexRecord> {
        let mut records = Vec::with_capacity(blocks.len());
        for block in blocks {
            let block_id = self.next_block_id;
            self.next_block_id = self.next_block_id.saturating_add(1);
            let layout_meta = BlockLayoutMeta::new(block_id, block.estimated_height.height);
            records.push(
                BlockIndexRecord::new(block_id, None, 0, block.kind_tag, 0)
                    .with_layout_meta(layout_meta),
            );
        }
        records
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedPasteInput {
    text_blocks: Vec<String>,
    image_sources: Vec<String>,
}

impl PasteProgress {
    fn new(phase: PastePipelinePhase) -> Self {
        Self {
            phase,
            parsed_blocks: 0,
            normalized_blocks: 0,
            allocated_blocks: 0,
            inserted_blocks: 0,
            hydrated_visible_blocks: 0,
            persisted_payloads: 0,
            media_metadata_tasks: 0,
            cancelled: false,
        }
    }

    fn cancelled(mut self) -> Self {
        self.phase = PastePipelinePhase::Cancelled;
        self.cancelled = true;
        self
    }
}

fn should_cancel(options: PasteRunOptions, phase: PastePipelinePhase) -> bool {
    options.cancel_after_phase == Some(phase)
}

fn cancelled_result(mut progress: PasteProgress, mut log: Vec<PasteProgress>) -> PasteImportResult {
    progress = progress.cancelled();
    log.push(progress.clone());
    PasteImportResult {
        transaction: None,
        records: Vec::new(),
        visible_hydrated_blocks: Vec::new(),
        persist_tasks: Vec::new(),
        media_metadata_tasks: Vec::new(),
        progress,
        progress_log: log,
    }
}

fn cancelled_result_with_records(
    mut progress: PasteProgress,
    mut log: Vec<PasteProgress>,
    records: Vec<BlockIndexRecord>,
) -> PasteImportResult {
    progress = progress.cancelled();
    log.push(progress.clone());
    PasteImportResult {
        transaction: None,
        records,
        visible_hydrated_blocks: Vec::new(),
        persist_tasks: Vec::new(),
        media_metadata_tasks: Vec::new(),
        progress,
        progress_log: log,
    }
}

fn parse_input(input: ClipboardInput) -> ParsedPasteInput {
    match input {
        ClipboardInput::PlainText(text) | ClipboardInput::Markdown(text) => ParsedPasteInput {
            text_blocks: text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
            image_sources: Vec::new(),
        },
        ClipboardInput::Html(html) => parse_html(html),
    }
}

fn parse_html(html: String) -> ParsedPasteInput {
    let without_scripts = strip_script_blocks(&html);
    let image_sources = extract_img_sources(&without_scripts);
    let text = strip_tags(&without_scripts);
    let text_blocks = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    ParsedPasteInput {
        text_blocks,
        image_sources,
    }
}

fn sanitize_parsed(parsed: ParsedPasteInput) -> ParsedPasteInput {
    ParsedPasteInput {
        text_blocks: parsed
            .text_blocks
            .into_iter()
            .map(|text| text.replace("\u{0000}", ""))
            .collect(),
        image_sources: parsed
            .image_sources
            .into_iter()
            .filter(|source| !source.trim().is_empty())
            .collect(),
    }
}

fn build_persist_tasks(records: &[BlockIndexRecord], batch_size: usize) -> Vec<PayloadPersistTask> {
    let batch_size = batch_size.max(1);
    records
        .chunks(batch_size)
        .map(|chunk| PayloadPersistTask {
            block_ids: chunk.iter().map(|record| record.id).collect(),
            async_only: true,
        })
        .collect()
}

fn strip_script_blocks(html: &str) -> String {
    let lower = html.to_lowercase();
    let mut result = String::new();
    let mut cursor = 0;
    while let Some(start_rel) = lower[cursor..].find("<script") {
        let start = cursor + start_rel;
        result.push_str(&html[cursor..start]);
        let Some(end_rel) = lower[start..].find("</script>") else {
            cursor = html.len();
            break;
        };
        cursor = start + end_rel + "</script>".len();
    }
    result.push_str(&html[cursor..]);
    result
}

fn extract_img_sources(html: &str) -> Vec<String> {
    let mut sources = Vec::new();
    let lower = html.to_lowercase();
    let mut cursor = 0;
    while let Some(img_rel) = lower[cursor..].find("<img") {
        let img_start = cursor + img_rel;
        let tag_end = lower[img_start..]
            .find('>')
            .map(|end| img_start + end)
            .unwrap_or(html.len());
        let tag = &html[img_start..tag_end];
        if let Some(source) = extract_attr(tag, "src") {
            sources.push(source);
        }
        cursor = tag_end.saturating_add(1);
    }
    sources
}

fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let attr_pattern = format!("{attr}=");
    let start = lower.find(&attr_pattern)? + attr_pattern.len();
    let value = &tag[start..];
    let mut chars = value.chars();
    let quote = chars.next()?;
    if quote == '"' || quote == '\'' {
        let rest = &value[quote.len_utf8()..];
        let end = rest.find(quote).unwrap_or(rest.len());
        Some(rest[..end].to_owned())
    } else {
        let end = value
            .find(|ch: char| ch.is_whitespace() || ch == '>')
            .unwrap_or(value.len());
        Some(value[..end].to_owned())
    }
}

fn strip_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                result.push('\n');
            }
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use cditor_core::edit::{EditOperation, EditTransactionKind};

    #[test]
    fn paste_10k_markdown_blocks_batches_insert_and_hydrates_visible_first() {
        let markdown = (0..10_000)
            .map(|index| format!("paragraph {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut pipeline = PasteImportPipeline::new(
            PasteImportConfig {
                visible_hydration_limit: 48,
                persist_batch_blocks: 1_000,
            },
            1,
        );

        let result = pipeline.run(
            ClipboardInput::Markdown(markdown),
            0,
            7,
            100,
            PasteRunOptions::normal(),
        );

        assert!(!result.cancelled());
        assert_eq!(result.records.len(), 10_000);
        assert_eq!(result.visible_hydrated_blocks.len(), 48);
        assert_eq!(result.persist_tasks.len(), 10);
        assert!(result.persist_tasks.iter().all(|task| task.async_only));

        let transaction = result.transaction.as_ref().unwrap();
        assert_eq!(transaction.kind, EditTransactionKind::Paste);
        assert!(matches!(
            &transaction.ops[0],
            EditOperation::InsertBlocks { index: 0, blocks } if blocks.len() == 10_000
        ));
        assert_eq!(result.progress.phase, PastePipelinePhase::Completed);
    }

    #[test]
    fn paste_html_with_images_sanitizes_and_schedules_metadata_tasks() {
        let html = r#"
            <div>Hello</div>
            <script>alert('x')</script>
            <img src="file://local/image.png" />
            <img src='https://example.com/a.png'>
        "#;
        let mut pipeline = PasteImportPipeline::new(PasteImportConfig::default(), 10);

        let result = pipeline.run(
            ClipboardInput::Html(html.to_owned()),
            0,
            1,
            0,
            PasteRunOptions::normal(),
        );

        assert!(!result.cancelled());
        assert_eq!(result.media_metadata_tasks.len(), 2);
        assert!(
            result
                .records
                .iter()
                .any(|record| record.kind_tag == KIND_IMAGE)
        );
        assert!(
            result
                .records
                .iter()
                .any(|record| record.layout_meta.estimated_height == IMAGE_PLACEHOLDER_HEIGHT)
        );
        assert!(
            result
                .progress_log
                .iter()
                .any(|progress| progress.phase == PastePipelinePhase::Sanitized)
        );
    }

    #[test]
    fn paste_cancel_stops_before_batch_insert() {
        let markdown = (0..10_000)
            .map(|index| format!("paragraph {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut pipeline = PasteImportPipeline::new(PasteImportConfig::default(), 1);

        let result = pipeline.run(
            ClipboardInput::Markdown(markdown),
            0,
            1,
            0,
            PasteRunOptions::cancel_after(PastePipelinePhase::NormalizedBlocks),
        );

        assert!(result.cancelled());
        assert!(result.transaction.is_none());
        assert!(result.visible_hydrated_blocks.is_empty());
        assert!(result.persist_tasks.is_empty());
        assert_eq!(result.progress.phase, PastePipelinePhase::Cancelled);
    }
}
