use cditor_core::document::BlockIndexRecord;
use cditor_core::ids::BlockId;
use cditor_core::rich_text::{BlockPayload, BlockPayloadRecord, RichBlockKind};
use cditor_storage::{StorageError, StorageResult};
use std::collections::HashMap;
use tracing::debug;

/// 从 CDitor blocks 生成 Markdown。
///
/// Canonical export, not source-preserving. Inline syntax is validated by CDitor core.
pub fn blocks_to_markdown(
    index_records: &[BlockIndexRecord],
    payloads: &[BlockPayloadRecord],
) -> StorageResult<String> {
    let payload_map: HashMap<BlockId, &BlockPayloadRecord> =
        payloads.iter().map(|payload| (payload.block_id, payload)).collect();

    let mut markdown = String::new();
    // 有序列表的连续序号；遇到非有序列表块时归零。
    let mut ordered_index = 0usize;
    let mut prev_was_list = false;

    for record in index_records {
        let payload = payload_map
            .get(&record.id)
            .ok_or_else(|| StorageError::NotFound {
                entity: "block payload",
                id: format!("{}", record.id),
            })?;

        let is_list = matches!(
            payload.kind,
            RichBlockKind::BulletedList | RichBlockKind::NumberedList | RichBlockKind::Todo { .. }
        );

        // 列表结束后补一个空行，避免后续段落被吸进列表。
        if prev_was_list && !is_list {
            markdown.push('\n');
        }
        if !matches!(payload.kind, RichBlockKind::NumberedList) {
            ordered_index = 0;
        }

        match &payload.kind {
            RichBlockKind::Heading { level } => {
                markdown.push_str(&"#".repeat((*level).clamp(1, 6) as usize));
                markdown.push(' ');
                markdown.push_str(&render_inline(payload)?);
                markdown.push_str("\n\n");
            }

            RichBlockKind::Paragraph | RichBlockKind::DocumentTitle => {
                let text = render_inline(payload)?;
                if !text.is_empty() {
                    markdown.push_str(&text);
                    markdown.push_str("\n\n");
                }
            }

            RichBlockKind::Code { .. } => {
                let (language, text) = match &payload.payload {
                    BlockPayload::Code { language, text } => (language.clone(), text.clone()),
                    // kind 是 Code 但 payload 不是时退回纯文本，宁可少格式也不丢内容。
                    _ => (None, payload.plain_text()),
                };
                markdown.push_str("```");
                if let Some(lang) = language.as_deref().filter(|lang| !lang.is_empty()) {
                    markdown.push_str(lang);
                }
                markdown.push('\n');
                markdown.push_str(&text);
                if !text.ends_with('\n') {
                    markdown.push('\n');
                }
                markdown.push_str("```\n\n");
            }

            RichBlockKind::BulletedList => {
                markdown.push_str("- ");
                markdown.push_str(&render_inline(payload)?);
                markdown.push('\n');
            }

            RichBlockKind::NumberedList => {
                ordered_index += 1;
                markdown.push_str(&format!("{ordered_index}. "));
                markdown.push_str(&render_inline(payload)?);
                markdown.push('\n');
            }

            RichBlockKind::Todo { checked } => {
                markdown.push_str(if *checked { "- [x] " } else { "- [ ] " });
                markdown.push_str(&render_inline(payload)?);
                markdown.push('\n');
            }

            RichBlockKind::Quote => {
                // 多行引用的每一行都要带 `>`。
                let text = render_inline(payload)?;
                for line in text.lines() {
                    markdown.push_str("> ");
                    markdown.push_str(line);
                    markdown.push('\n');
                }
                markdown.push('\n');
            }

            RichBlockKind::Image => {
                markdown.push_str("![](");
                markdown.push_str(&payload.plain_text());
                markdown.push_str(")\n\n");
            }

            RichBlockKind::Divider => {
                markdown.push_str("---\n\n");
            }

            _ => {
                // 暂不支持的块类型：保留纯文本，不丢内容。
                let text = render_inline(payload)?;
                if !text.is_empty() {
                    markdown.push_str(&text);
                    markdown.push_str("\n\n");
                }
            }
        }

        prev_was_list = is_list;
    }

    let markdown = format!("{}\n", markdown.trim_end());
    debug!("Generated {} bytes of markdown", markdown.len());
    Ok(markdown)
}

/// 把一个 block 的 spans 渲染成带 inline 标记的 markdown。
fn render_inline(payload: &BlockPayloadRecord) -> StorageResult<String> {
    let BlockPayload::RichText { spans } = &payload.payload else {
        return Ok(payload.plain_text());
    };
    cditor_core::markdown::render_inline_spans(spans)
        .map_err(|error| StorageError::Serialization(error.to_string()))
}
