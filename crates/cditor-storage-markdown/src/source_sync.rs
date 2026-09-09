use std::ops::Range;

use cditor_core::document::BlockIndexRecord;
use cditor_core::markdown::{MarkdownSyntax, MappingKind, SyntaxKind, equivalent_spans, render_inline_spans};
use cditor_core::rich_text::{BlockPayload, BlockPayloadRecord, RichBlockKind};
use cditor_storage::{StorageError, StorageResult};
use pulldown_cmark::TagEnd;

use crate::parse_markdown_to_blocks;

fn unsupported(message: &str) -> StorageError { StorageError::Serialization(message.to_owned()) }

/// Bridge for existing rich-text save batches. Only proven source mappings are
/// accepted. Structural editing needs a source-aware runtime transaction bridge.
pub(crate) fn sync_source(
    source: &str,
    before_index: &[BlockIndexRecord],
    before: &[BlockPayloadRecord],
    after_index: &[BlockIndexRecord],
    after: &[BlockPayloadRecord],
) -> StorageResult<String> {
    if before_index.len() != after_index.len() || before_index.iter().zip(after_index).any(|(a, b)| {
        a.id != b.id || a.parent_id != b.parent_id || a.depth != b.depth || a.kind_tag != b.kind_tag
    }) {
        return Err(unsupported("structural Markdown editing requires source transactions; original file was not changed"));
    }
    let mut changes = Vec::new();
    for record in after_index {
        let old = before.iter().find(|p| p.block_id == record.id).ok_or_else(|| unsupported("missing original payload"))?;
        let new = after.iter().find(|p| p.block_id == record.id).ok_or_else(|| unsupported("missing updated payload"))?;
        if old.kind != new.kind || old.payload != new.payload { changes.push((old, new)); }
    }
    if changes.is_empty() { return Ok(source.to_owned()); }
    let syntax = MarkdownSyntax::parse(source);
    let mut patches: Vec<(Range<usize>, String)> = vec![];
    for (old, new) in changes {
        if old.kind != new.kind { return Err(unsupported("block kind changes require source transactions")); }
        let (BlockPayload::RichText { spans: old_spans }, BlockPayload::RichText { spans: new_spans }) = (&old.payload, &new.payload) else {
            return Err(unsupported("this block payload requires source editing"));
        };
        if equivalent_spans(old_spans, new_spans) { continue; }
        let mut candidates = vec![];
        for (id, node) in syntax.nodes().iter().enumerate() {
            let compatible = match (&old.kind, &node.kind) {
                (RichBlockKind::Paragraph | RichBlockKind::Quote, SyntaxKind::Block(TagEnd::Paragraph)) => true,
                (RichBlockKind::Heading { level }, SyntaxKind::Block(TagEnd::Heading(parsed))) => *level == *parsed as u8,
                (RichBlockKind::BulletedList | RichBlockKind::NumberedList, SyntaxKind::Block(TagEnd::Item)) => {
                    !node.children.iter().any(|&id| matches!(syntax.nodes()[id].kind, SyntaxKind::Block(_)))
                },
                _ => false,
            };
            if compatible && let Ok(projection) = syntax.project_inline(id, source)
                && equivalent_spans(old_spans, &projection.spans)
            { candidates.push((id, projection)); }
        }
        if candidates.len() != 1 {
            return Err(unsupported("Markdown source block is ambiguous or unsupported; original file was not changed"));
        }
        let (id, projection) = candidates.pop().unwrap();
        let new_text: String = new_spans.iter().map(|span| span.text.as_str()).collect();
        let (range, inserted) = text_difference(&projection.text, &new_text);
        let mut precise = None;
        for segment in &projection.segments {
            if segment.mapping == MappingKind::Exact && segment.visible.start <= range.start && range.end <= segment.visible.end {
                let mut escaped = String::new();
                for ch in inserted.chars() {
                    if ch.is_ascii_punctuation() { escaped.push('\\'); }
                    escaped.push(ch);
                }
                let start = segment.source.start + range.start - segment.visible.start;
                let end = segment.source.start + range.end - segment.visible.start;
                let mut candidate = source.to_owned();
                candidate.replace_range(start..end, &escaped);
                // Verify this local change against the whole expected projection,
                // including surrounding styles; a style-only edit won't pass.
                let mut expected = before.to_vec();
                if let Some(payload) = expected.iter_mut().find(|p| p.block_id == new.block_id) { *payload = new.clone(); }
                if matches_projection(&candidate, before_index, &expected) {
                    precise = Some((start..end, escaped));
                    break;
                }
            }
        }
        if let Some(patch) = precise { patches.push(patch); continue; }
        let node = &syntax.nodes()[id];
        let first = node.children.first().ok_or_else(|| unsupported("empty inline source"))?;
        let last = node.children.last().unwrap();
        let range = syntax.nodes()[*first].source.start..syntax.nodes()[*last].source.end;
        // Multiline quote/list prefixes belong to the containing block, not inline.
        if source[range.clone()].contains(['\n', '\r']) {
            return Err(unsupported("multiline source transformation requires container-aware editing"));
        }
        let replacement = render_inline_spans(new_spans).map_err(|e| unsupported(&e.to_string()))?;
        patches.push((range, replacement));
    }
    patches.sort_by_key(|(range, _)| range.start);
    if patches.windows(2).any(|pair| pair[0].0.end > pair[1].0.start) { return Err(unsupported("overlapping Markdown source edits")); }
    let mut output = source.to_owned();
    for (range, replacement) in patches.into_iter().rev() { output.replace_range(range, &replacement); }
    if !matches_projection(&output, after_index, after) {
        return Err(unsupported("source patch changes document semantics; original file was not changed"));
    }
    Ok(output)
}

fn matches_projection(source: &str, index: &[BlockIndexRecord], expected: &[BlockPayloadRecord]) -> bool {
    let Ok((_, actual)) = parse_markdown_to_blocks(source, 1) else { return false; };
    actual.len() == index.len() && actual.iter().zip(index).all(|(actual, record)| {
        expected.iter().find(|p| p.block_id == record.id).is_some_and(|expected| {
            actual.kind == expected.kind && match (&actual.payload, &expected.payload) {
                (BlockPayload::RichText { spans: a }, BlockPayload::RichText { spans: b }) => equivalent_spans(a, b),
                (a, b) => a == b,
            }
        })
    })
}

fn text_difference<'a>(before: &str, after: &'a str) -> (Range<usize>, &'a str) {
    let prefix = before.chars().zip(after.chars()).take_while(|(a, b)| a == b).map(|(ch, _)| ch.len_utf8()).sum::<usize>();
    let suffix = before[prefix..].chars().rev().zip(after[prefix..].chars().rev())
        .take_while(|(a, b)| a == b).map(|(ch, _)| ch.len_utf8()).sum::<usize>();
    (prefix..before.len() - suffix, &after[prefix..after.len() - suffix])
}
