use cditor_core::document::BlockIndexRecord;
use cditor_core::ids::{BlockId, DocumentId};
use cditor_core::layout::BlockLayoutMeta;
use cditor_core::rich_text::{
    BlockPayload, BlockPayloadRecord, InlineMark, InlineSpan, RichBlockKind,
    kind_tag_for_rich_block_kind,
};
use cditor_storage::StorageResult;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use tracing::debug;

/// Rendering projection only. The stack preserves repeated/nested marks.
#[derive(Default, Clone)]
struct InlineState {
    stack: Vec<InlineMark>,
}

impl InlineState {
    fn marks(&self) -> Vec<InlineMark> {
        self.stack.clone()
    }
}

/// 一个 block 的累积状态。
struct BlockBuilder {
    kind: RichBlockKind,
    spans: Vec<InlineSpan>,
    /// 代码块原文；代码块内不做 inline 解析。
    code_text: String,
}

impl BlockBuilder {
    fn new(kind: RichBlockKind) -> Self {
        Self {
            kind,
            spans: Vec::new(),
            code_text: String::new(),
        }
    }

    fn is_code(&self) -> bool {
        matches!(self.kind, RichBlockKind::Code { .. })
    }

    fn push_text(&mut self, text: &str, state: &InlineState) {
        if self.is_code() {
            self.code_text.push_str(text);
            return;
        }
        let marks = state.marks();
        // 相邻且标记相同的文本合并，避免每个 Text 事件都产生一个 span。
        if let Some(last) = self.spans.last_mut() {
            if last.marks == marks {
                last.text.push_str(text);
                return;
            }
        }
        self.spans.push(InlineSpan {
            text: text.to_owned(),
            marks,
        });
    }

    fn plain_len(&self) -> usize {
        if self.is_code() {
            self.code_text.trim().len()
        } else {
            self.spans.iter().map(|span| span.text.trim().len()).sum()
        }
    }


    fn into_payload(self, block_id: BlockId) -> BlockPayloadRecord {
        let payload = if self.is_code() {
            let language = match &self.kind {
                RichBlockKind::Code { language } => language.clone(),
                _ => None,
            };
            BlockPayload::Code {
                language,
                text: self.code_text.trim_end_matches('\n').to_owned(),
            }
        } else {
            BlockPayload::RichText { spans: self.spans }
        };
        BlockPayloadRecord {
            block_id,
            content_version: 1,
            kind: self.kind,
            payload,
        }
    }
}

/// 解析 Markdown 为 CDitor blocks。
///
/// 块级映射：标题、段落、代码块、引用、分隔线各成一块；列表的每个 item 单独
/// 成块，类型为 `BulletedList` / `NumberedList` / `Todo`。inline 标记（粗体、
/// 斜体、删除线、行内代码、链接）保留在 `InlineSpan::marks` 上，因此往返不丢格式。
pub fn parse_markdown_to_blocks(
    markdown: &str,
    document_id: DocumentId,
) -> StorageResult<(Vec<BlockIndexRecord>, Vec<BlockPayloadRecord>)> {
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(markdown, options);

    let mut index_records = Vec::new();
    let mut payloads = Vec::new();
    let mut counter = 0usize;

    let mut current: Option<BlockBuilder> = None;
    let mut inline = InlineState::default();
    // 列表栈：每层记录是否有序，决定 item 的块类型。
    let mut list_stack: Vec<bool> = Vec::new();
    // 引用深度：引用内的段落保持 Quote 类型，而不是退回 Paragraph。
    let mut quote_depth = 0usize;
    // 图片的 alt 文本以 Text 事件到达，需要拦下来不混进正文。
    let mut pending_image: Option<String> = None;

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                flush_block(&mut current, &mut index_records, &mut payloads, &mut counter);
                let level_num = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                current = Some(BlockBuilder::new(RichBlockKind::Heading { level: level_num }));
            }
            Event::End(TagEnd::Heading(_)) => {
                flush_block(&mut current, &mut index_records, &mut payloads, &mut counter);
            }

            Event::Start(Tag::Paragraph) => {
                // 列表项内的段落沿用 item 已建立的 builder：紧凑列表没有内层段落，
                // 松散列表有，两者都不该新开一个块。
                if list_stack.is_empty() {
                    flush_block(&mut current, &mut index_records, &mut payloads, &mut counter);
                    let kind = if quote_depth > 0 {
                        RichBlockKind::Quote
                    } else {
                        RichBlockKind::Paragraph
                    };
                    current = Some(BlockBuilder::new(kind));
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if list_stack.is_empty() {
                    flush_block(&mut current, &mut index_records, &mut payloads, &mut counter);
                }
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                flush_block(&mut current, &mut index_records, &mut payloads, &mut counter);
                let language = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.to_string()),
                    _ => None,
                };
                current = Some(BlockBuilder::new(RichBlockKind::Code { language }));
            }
            Event::End(TagEnd::CodeBlock) => {
                flush_block(&mut current, &mut index_records, &mut payloads, &mut counter);
            }

            Event::Start(Tag::List(first_number)) => {
                flush_block(&mut current, &mut index_records, &mut payloads, &mut counter);
                list_stack.push(first_number.is_some());
            }
            Event::End(TagEnd::List(_)) => {
                flush_block(&mut current, &mut index_records, &mut payloads, &mut counter);
                list_stack.pop();
            }

            // item 的开始和结束都要 flush。开始时 flush 防止 item 被并进前一个块
            // （标题或上一个 item）——这正是旧实现把列表吞进标题的原因；结束时
            // flush 处理紧凑列表，那种列表的 item 内不会有 Paragraph 事件。
            Event::Start(Tag::Item) => {
                flush_block(&mut current, &mut index_records, &mut payloads, &mut counter);
                let ordered = list_stack.last().copied().unwrap_or(false);
                let kind = if ordered {
                    RichBlockKind::NumberedList
                } else {
                    RichBlockKind::BulletedList
                };
                current = Some(BlockBuilder::new(kind));
            }
            Event::End(TagEnd::Item) => {
                flush_block(&mut current, &mut index_records, &mut payloads, &mut counter);
            }

            Event::TaskListMarker(checked) => {
                // 任务列表项：把当前 item 升级为 Todo。
                if let Some(builder) = current.as_mut() {
                    builder.kind = RichBlockKind::Todo { checked };
                }
            }

            Event::Start(Tag::BlockQuote(_)) => {
                flush_block(&mut current, &mut index_records, &mut payloads, &mut counter);
                quote_depth += 1;
            }
            Event::End(TagEnd::BlockQuote) => {
                flush_block(&mut current, &mut index_records, &mut payloads, &mut counter);
                quote_depth = quote_depth.saturating_sub(1);
            }

            Event::Rule => {
                flush_block(&mut current, &mut index_records, &mut payloads, &mut counter);
                push_block(
                    &mut index_records,
                    &mut payloads,
                    &mut counter,
                    BlockBuilder::new(RichBlockKind::Divider),
                );
            }

            Event::Start(Tag::Image { dest_url, .. }) => {
                flush_block(&mut current, &mut index_records, &mut payloads, &mut counter);
                pending_image = Some(dest_url.to_string());
            }

            Event::Start(Tag::Strong) => inline.stack.push(InlineMark::Bold),
            Event::Start(Tag::Emphasis) => inline.stack.push(InlineMark::Italic),
            Event::Start(Tag::Strikethrough) => inline.stack.push(InlineMark::Strike),
            Event::End(TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough | TagEnd::Link) => {
                inline.stack.pop();
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                inline.stack.push(InlineMark::Link { href: dest_url.to_string() });
            }

            // 图片的 End 是独立的 TagEnd::Image，必须在这里把 pending_image 取走：
            // 漏掉它会让后面每个 Event::Text 都被 `pending_image.is_some()` 吞掉，
            // 整篇正文消失。
            Event::End(TagEnd::Image) => {
                if let Some(src) = pending_image.take() {
                    let mut builder = BlockBuilder::new(RichBlockKind::Image);
                    builder.push_text(&src, &InlineState::default());
                    push_block(&mut index_records, &mut payloads, &mut counter, builder);
                }
            }

            Event::Text(text) => {
                // 图片内的 Text 是 alt 文本，不能当正文。
                if pending_image.is_some() {
                    continue;
                }
                let builder = current.get_or_insert_with(|| {
                    BlockBuilder::new(if quote_depth > 0 {
                        RichBlockKind::Quote
                    } else {
                        RichBlockKind::Paragraph
                    })
                });
                builder.push_text(&text, &inline);
            }

            Event::Code(code) => {
                let mut state = inline.clone();
                state.stack.push(InlineMark::Code);
                let builder = current.get_or_insert_with(|| {
                    BlockBuilder::new(if quote_depth > 0 {
                        RichBlockKind::Quote
                    } else {
                        RichBlockKind::Paragraph
                    })
                });
                builder.push_text(&code, &state);
            }

            Event::SoftBreak | Event::HardBreak => {
                if let Some(builder) = current.as_mut() {
                    builder.push_text("\n", &inline);
                }
            }

            _ => {}
        }
    }

    flush_block(&mut current, &mut index_records, &mut payloads, &mut counter);

    // 空文档保留一个可编辑的空段落。
    if index_records.is_empty() {
        push_block(
            &mut index_records,
            &mut payloads,
            &mut counter,
            BlockBuilder::new(RichBlockKind::Paragraph),
        );
    }

    debug!(
        "Parsed {} blocks from markdown for document {}",
        index_records.len(),
        document_id
    );

    Ok((index_records, payloads))
}

fn flush_block(
    current: &mut Option<BlockBuilder>,
    index_records: &mut Vec<BlockIndexRecord>,
    payloads: &mut Vec<BlockPayloadRecord>,
    counter: &mut usize,
) {
    let Some(builder) = current.take() else {
        return;
    };
    // 只有图片/分隔线这类本身无文本的块允许为空。
    let keep_empty = matches!(builder.kind, RichBlockKind::Image | RichBlockKind::Divider);
    if builder.plain_len() == 0 && !keep_empty {
        return;
    }
    push_block(index_records, payloads, counter, builder);
}

fn push_block(
    index_records: &mut Vec<BlockIndexRecord>,
    payloads: &mut Vec<BlockPayloadRecord>,
    counter: &mut usize,
    builder: BlockBuilder,
) {
    // BlockId 从 1 开始：0 在 runtime 里表示“无效/未设置”。
    let block_id = BlockId::from(*counter as u64 + 1);
    let kind = builder.kind.clone();
    let kind_tag = kind_tag_for_rich_block_kind(&kind);
    let payload = builder.into_payload(block_id);
    let estimated_height = estimate_height(&kind, &payload);

    index_records.push(BlockIndexRecord {
        id: block_id,
        parent_id: None,
        depth: 0,
        kind_tag,
        flags: 0,
        layout_meta: BlockLayoutMeta {
            block_id,
            estimated_height,
            measured_height: None,
            width_bucket: 10,
            layout_version: 0,
            dirty: true,
        },
    });
    payloads.push(payload);
    *counter += 1;
}

/// 首屏估高：只用于虚拟滚动的初始布局，真实高度由渲染层测量后回填。
fn estimate_height(kind: &RichBlockKind, payload: &BlockPayloadRecord) -> f64 {
    let text = payload.plain_text();
    match kind {
        RichBlockKind::Heading { level } => match level {
            1 => 48.0,
            2 => 40.0,
            3 => 32.0,
            _ => 28.0,
        },
        RichBlockKind::Code { .. } => {
            let lines = text.lines().count().max(1);
            lines as f64 * 20.0 + 40.0
        }
        RichBlockKind::Image => 400.0,
        RichBlockKind::Divider => 24.0,
        RichBlockKind::Paragraph | RichBlockKind::Quote => {
            let lines = (text.chars().count() as f64 / 80.0).ceil().max(1.0);
            lines * 24.0 + 16.0
        }
        _ => 32.0,
    }
}
