use cditor_core::edit::ChangeOrigin;

use super::markdown_paste::trace_markdown;
use super::*;

struct MarkdownApplyPlan {
    after_kind: RichBlockKind,
    after_payload: BlockPayload,
    inserted_records: Vec<BlockIndexRecord>,
    inserted_payloads: Vec<BlockPayloadRecord>,
    focus_block_id: BlockId,
    focus_offset: usize,
}

impl DocumentRuntime {
    pub(super) fn insert_ai_markdown_content(&mut self, markdown: &str) -> Result<bool, String> {
        self.insert_markdown_content_transaction(
            markdown,
            EditTransactionKind::AiApply,
            ChangeOrigin::Ai,
        )
    }

    pub(super) fn insert_markdown_content_transaction(
        &mut self,
        markdown: &str,
        kind: EditTransactionKind,
        origin: ChangeOrigin,
    ) -> Result<bool, String> {
        let before_selection = self.document_selection_snapshot();
        let before_selected_blocks = self.selected_block_ids_snapshot();
        let cross_plan = self
            .document_selection
            .filter(|selection| selection.anchor.block_id != selection.focus.block_id)
            .map(|selection| self.plan_cross_block_replacement(selection))
            .transpose()?;
        let current_block_id = cross_plan
            .as_ref()
            .map(|plan| plan.selection.start.block_id)
            .or_else(|| self.focused_block_id())
            .ok_or_else(|| "missing focused block".to_owned())?;
        let current_index = self
            .document
            .index
            .index_of(current_block_id)
            .ok_or_else(|| "focused block is missing from index".to_owned())?;
        let before_current = self
            .document
            .payload_window
            .get(current_block_id)
            .cloned()
            .ok_or_else(|| format!("missing payload for block {current_block_id}"))?;
        if !before_current.kind.supports_rich_text_title() {
            trace_markdown(
                "parse.blocked",
                format_args!("block={current_block_id} kind={:?}", before_current.kind),
            );
            return Ok(false);
        }

        let current_spans = editable_payload_spans(&before_current.payload)?;
        let current_text = plain_text_from_spans(&current_spans);
        let (prefix, suffix) = if let Some(plan) = &cross_plan {
            let end = self
                .document
                .payload_window
                .get(plan.selection.end.block_id)
                .ok_or_else(|| "selection end payload is not hydrated".to_owned())?;
            let end_spans = editable_payload_spans(&end.payload)?;
            let end_text = plain_text_from_spans(&end_spans);
            let start = safe_char_range(
                &current_text,
                plan.selection.start.offset..plan.selection.start.offset,
            )
            .start;
            let end = safe_char_range(
                &end_text,
                plan.selection.end.offset..plan.selection.end.offset,
            )
            .start;
            (
                slice_rich_text_spans(&current_spans, 0..start),
                slice_rich_text_spans(&end_spans, end..end_text.len()),
            )
        } else {
            let range = self
                .focused_text_selection_range()
                .map(|range| safe_char_range(&current_text, range))
                .unwrap_or_else(|| {
                    let caret = self
                        .caret_offset_for_block(current_block_id)
                        .unwrap_or(current_text.len());
                    safe_char_range(&current_text, caret..caret)
                });
            (
                slice_rich_text_spans(&current_spans, 0..range.start),
                slice_rich_text_spans(&current_spans, range.end..current_text.len()),
            )
        };

        let options = MarkdownImportOptions {
            document_id: self.document_id,
            first_block_id: self.next_available_block_id(),
        };
        let imported = import_markdown_block_incremental(markdown, options)
            .map(|block| ParsedMarkdownDocument {
                root_blocks: vec![block.id],
                blocks: vec![block],
            })
            .unwrap_or_else(|| parse_markdown_document(markdown, options));
        trace_markdown(
            "parse.result",
            format_args!(
                "block={current_block_id} input_bytes={} blocks={} roots={} kinds={:?}",
                markdown.len(),
                imported.blocks.len(),
                imported.root_blocks.len(),
                imported
                    .blocks
                    .iter()
                    .map(|block| &block.kind)
                    .collect::<Vec<_>>()
            ),
        );
        if imported.blocks.is_empty() {
            return Ok(false);
        }

        let insert_at = cross_plan
            .as_ref()
            .map(|plan| plan.replacement_index)
            .unwrap_or_else(|| self.subtree_end(current_index));
        let parent_id = self.document.index.parent_ids[current_index];
        let depth = self.document.index.depths[current_index];
        let plan = if imported
            .blocks
            .iter()
            .any(|block| matches!(block.kind, RichBlockKind::Table))
        {
            build_table_plan(
                self.document_id,
                current_block_id,
                parent_id,
                depth,
                insert_at,
                before_current.kind.clone(),
                prefix,
                suffix,
                imported,
            )
        } else {
            build_text_plan(
                self.document_id,
                current_block_id,
                parent_id,
                depth,
                insert_at,
                prefix,
                suffix,
                imported,
            )?
        };

        let mut ops = vec![payload_replace_operation(
            current_block_id,
            before_current.kind.clone(),
            before_current.payload.clone(),
            plan.after_kind.clone(),
            plan.after_payload.clone(),
        )];
        let (structure_ops, mut inverse_ops) = if let Some(cross_plan) = &cross_plan {
            cross_plan.structure_operations(
                plan.inserted_records.clone(),
                plan.inserted_payloads.clone(),
            )?
        } else if plan.inserted_records.is_empty() {
            (Vec::new(), Vec::new())
        } else {
            (
                vec![EditOperation::InsertBlocks {
                    index: insert_at,
                    blocks: plan.inserted_records.clone(),
                    payloads: plan.inserted_payloads.clone(),
                }],
                vec![EditOperation::DeleteBlockRange {
                    range: insert_at..insert_at + plan.inserted_records.len(),
                }],
            )
        };
        ops.extend(structure_ops);
        inverse_ops.push(payload_replace_operation(
            current_block_id,
            plan.after_kind.clone(),
            plan.after_payload.clone(),
            before_current.kind.clone(),
            before_current.payload.clone(),
        ));

        let after_selection = Some(DocumentSelection::caret(TextPosition::downstream(
            plan.focus_block_id,
            plan.focus_offset,
        )));
        let mut preconditions = vec![
            TransactionPrecondition::StructureVersion(self.structure_version()),
            TransactionPrecondition::BlockContentVersion {
                block_id: current_block_id,
                version: before_current.content_version,
            },
        ];
        preconditions.extend(
            plan.inserted_records
                .iter()
                .map(|record| TransactionPrecondition::BlockAbsent(record.id)),
        );
        self.cancel_composition();
        self.apply_local_structure_transaction(
            kind,
            origin,
            ops,
            inverse_ops,
            preconditions,
            before_selection,
            after_selection,
            before_selected_blocks,
            Vec::new(),
        )?;
        self.focus_block_at_offset(plan.focus_block_id, plan.focus_offset)?;
        trace_markdown(
            "apply.done",
            format_args!(
                "current_block={current_block_id} total_blocks={} focus={:?}",
                self.document.index.total_count(),
                self.focused_block_id()
            ),
        );
        Ok(true)
    }
}

#[allow(clippy::too_many_arguments)]
fn build_text_plan(
    document_id: DocumentId,
    current_block_id: BlockId,
    parent_id: Option<BlockId>,
    depth: u16,
    insert_at: usize,
    prefix: Vec<InlineSpan>,
    suffix: Vec<InlineSpan>,
    mut imported: ParsedMarkdownDocument,
) -> Result<MarkdownApplyPlan, String> {
    let imported_first_id = imported.blocks[0].id;
    let mut remap = HashMap::new();
    remap.insert(imported_first_id, current_block_id);
    for block in imported.blocks.iter().skip(1) {
        remap.insert(block.id, block.id);
    }
    for block in &mut imported.blocks {
        block.id = remap.get(&block.id).copied().unwrap_or(block.id);
        block.document_id = document_id;
        block.parent_id = block
            .parent_id
            .and_then(|id| remap.get(&id).copied())
            .or(parent_id);
        if block.parent_id == parent_id {
            block.depth = depth;
        } else if block.parent_id.is_some() {
            block.depth = block.depth.saturating_add(depth);
        }
        for child in &mut block.children {
            *child = remap.get(child).copied().unwrap_or(*child);
        }
    }

    let mut first = imported.blocks.remove(0);
    first.parent_id = parent_id;
    first.depth = depth;
    first.payload = prepend_spans_to_payload(prefix, first.payload);
    let (focus_block_id, focus_offset) = if let Some(last) = imported.blocks.last_mut() {
        let offset = last.payload.plain_text().len();
        last.payload = append_spans_to_payload(last.payload.clone(), suffix);
        (last.id, offset)
    } else {
        let offset = first.payload.plain_text().len();
        first.payload = append_spans_to_payload(first.payload, suffix);
        (current_block_id, offset)
    };
    let (inserted_records, inserted_payloads) = block_operation_records(imported.blocks, insert_at);
    Ok(MarkdownApplyPlan {
        after_kind: first.kind,
        after_payload: first.payload,
        inserted_records,
        inserted_payloads,
        focus_block_id,
        focus_offset,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_table_plan(
    document_id: DocumentId,
    current_block_id: BlockId,
    parent_id: Option<BlockId>,
    depth: u16,
    insert_at: usize,
    current_kind: RichBlockKind,
    prefix: Vec<InlineSpan>,
    suffix: Vec<InlineSpan>,
    imported: ParsedMarkdownDocument,
) -> MarkdownApplyPlan {
    let mut blocks = imported.blocks;
    let prefix_empty = plain_text_from_spans(&prefix).is_empty();
    let (after_kind, after_payload) = if prefix_empty {
        let first = blocks.remove(0);
        (first.kind, first.payload)
    } else {
        (
            current_kind.clone(),
            payload_for_kind_from_spans(&current_kind, prefix),
        )
    };
    for block in &mut blocks {
        block.document_id = document_id;
        block.parent_id = parent_id;
        block.depth = depth;
        block.children.clear();
    }
    if !suffix.is_empty()
        || blocks
            .last()
            .is_some_and(|block| !block.kind.supports_rich_text_title())
    {
        let trailing_id = blocks
            .iter()
            .map(|block| block.id)
            .max()
            .unwrap_or(current_block_id)
            .saturating_add(1);
        let mut trailing = RichBlockRecord::new(
            trailing_id,
            RichBlockKind::Paragraph,
            BlockPayload::RichText {
                spans: normalized_spans(suffix.clone()),
            },
        );
        trailing.document_id = document_id;
        trailing.parent_id = parent_id;
        trailing.depth = depth;
        blocks.push(trailing);
    }
    let focus_block_id = blocks
        .iter()
        .rev()
        .find(|block| block.kind.supports_rich_text_title())
        .map(|block| block.id)
        .unwrap_or(current_block_id);
    let focus_offset = blocks
        .last()
        .filter(|block| block.id == focus_block_id)
        .map(|_| plain_text_from_spans(&suffix).len())
        .unwrap_or(0);
    let (inserted_records, inserted_payloads) = block_operation_records(blocks, insert_at);
    MarkdownApplyPlan {
        after_kind,
        after_payload,
        inserted_records,
        inserted_payloads,
        focus_block_id,
        focus_offset,
    }
}

fn block_operation_records(
    blocks: Vec<RichBlockRecord>,
    insert_at: usize,
) -> (Vec<BlockIndexRecord>, Vec<BlockPayloadRecord>) {
    blocks
        .into_iter()
        .enumerate()
        .map(|(offset, block)| {
            let payload = block.to_payload_record();
            let record = block.to_index_record().with_layout_meta(
                cditor_core::layout::BlockLayoutMeta::new(
                    block.id,
                    estimate_payload_height(&payload, insert_at + offset),
                ),
            );
            (record, payload)
        })
        .unzip()
}

pub(super) fn editable_payload_spans(payload: &BlockPayload) -> Result<Vec<InlineSpan>, String> {
    match payload {
        BlockPayload::RichText { spans } => Ok(spans.clone()),
        BlockPayload::Code { text, .. } => Ok(vec![InlineSpan::plain(text.clone())]),
        BlockPayload::Html { html, .. } => Ok(vec![InlineSpan::plain(html.clone())]),
        _ => Err("markdown paste target is not an editable text payload".to_owned()),
    }
}

fn prepend_spans_to_payload(mut prefix: Vec<InlineSpan>, payload: BlockPayload) -> BlockPayload {
    match payload {
        BlockPayload::RichText { spans } => {
            prefix.extend(spans);
            BlockPayload::RichText {
                spans: normalized_spans(prefix),
            }
        }
        other => prepend_plain_text_to_payload(plain_text_from_spans(&prefix), other),
    }
}

pub(super) fn append_spans_to_payload(
    payload: BlockPayload,
    suffix: Vec<InlineSpan>,
) -> BlockPayload {
    match payload {
        BlockPayload::RichText { mut spans } => {
            spans.extend(suffix);
            BlockPayload::RichText {
                spans: normalized_spans(spans),
            }
        }
        other => append_plain_text_to_payload(other, plain_text_from_spans(&suffix)),
    }
}

pub(super) fn payload_for_kind_from_spans(
    kind: &RichBlockKind,
    spans: Vec<InlineSpan>,
) -> BlockPayload {
    if matches!(
        kind,
        RichBlockKind::Paragraph
            | RichBlockKind::Heading { .. }
            | RichBlockKind::Quote
            | RichBlockKind::Callout { .. }
            | RichBlockKind::BulletedList
            | RichBlockKind::NumberedList
            | RichBlockKind::Todo { .. }
            | RichBlockKind::Toggle
    ) {
        BlockPayload::RichText {
            spans: normalized_spans(spans),
        }
    } else {
        payload_for_kind_from_plain_text(kind, plain_text_from_spans(&spans))
    }
}

fn normalized_spans(mut spans: Vec<InlineSpan>) -> Vec<InlineSpan> {
    merge_inline_spans(&mut spans);
    spans
}

pub(super) fn payload_replace_operation(
    block_id: BlockId,
    before_kind: RichBlockKind,
    before_payload: BlockPayload,
    after_kind: RichBlockKind,
    after_payload: BlockPayload,
) -> EditOperation {
    EditOperation::Block(cditor_core::edit::BlockEditOperation::ReplacePayload {
        block_id,
        before_kind,
        before_payload,
        after_kind,
        after_payload,
    })
}
