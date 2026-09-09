use super::*;
use cditor_core::markdown::{ByteOffset, MarkdownSource, SourceEdit, SourceSelection, SourceTransaction};
use cditor_core::edit::MarkdownBlockChange;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DocumentRuntimeMode {
    #[default]
    Sqlite,
    Markdown,
}

/// Source anchors are created once, in syntax order. Offset shifts are indexed
/// by a Fenwick tree: an edit never walks all following blocks.
#[derive(Debug)]
pub(super) struct MarkdownRuntimeState {
    source: MarkdownSource,
    positions: HashMap<BlockId, usize>,
    ranges: Vec<Range<usize>>,
    lengths: Vec<usize>,
    shifts: Vec<isize>,
    pub(super) references: HashMap<String, (String, String)>,
}

impl MarkdownRuntimeState {
    pub(super) fn snapshot(&self) -> (String, cditor_core::markdown::SourceSelection) {
        (self.source.text(), self.source.selection().clone())
    }

    pub(super) fn apply(&mut self, transaction: SourceTransaction) -> Result<(), cditor_core::markdown::MarkdownError> {
        self.source.apply(transaction)
    }
    pub(super) fn undo(&mut self) -> Result<bool, cditor_core::markdown::MarkdownError> { self.source.undo() }
    pub(super) fn redo(&mut self) -> Result<bool, cditor_core::markdown::MarkdownError> { self.source.redo() }

    pub(super) fn can_map_plain_text_edit(&self, before: &str, range: Range<usize>) -> bool {
        let source = self.source.text();
        source.match_indices(before).take(2).count() == 1
            && range.end <= before.len()
            && before.is_char_boundary(range.start)
            && before.is_char_boundary(range.end)
    }

    pub(super) fn apply_plain_text_edit(&mut self, before: &str, range: Range<usize>, replacement: &str) -> Result<(), cditor_core::markdown::MarkdownError> {
        if !self.can_map_plain_text_edit(before, range.clone()) {
            return Err(cditor_core::markdown::MarkdownError::Unsupported("ambiguous Markdown source anchor".into()));
        }
        let source = self.source.text();
        let start = source.match_indices(before).next().unwrap().0;
        let source_range = start + range.start..start + range.end;
        let caret = source_range.start + replacement.len();
        self.source.apply_without_history(SourceTransaction {
            revision: self.source.revision(),
            edits: vec![SourceEdit { range: ByteOffset(source_range.start)..ByteOffset(source_range.end), replacement: replacement.to_owned() }],
            after_selection: SourceSelection { anchor: ByteOffset(caret), focus: ByteOffset(caret) },
        })
    }

    pub(super) fn new(
        source: MarkdownSource,
        records: &[BlockIndexRecord],
        payloads: &[BlockPayloadRecord],
    ) -> Result<Self, String> {
        use cditor_core::markdown::{MarkdownSyntax, SyntaxKind, equivalent_spans};
        use pulldown_cmark::{Parser, TagEnd};
        let text = source.text();
        let syntax = MarkdownSyntax::parse(&text);
        let references = Parser::new_ext(&text, cditor_core::markdown::markdown_options())
            .reference_definitions().iter()
            .map(|(label, def)| (label.to_lowercase(), (def.dest.to_string(), def.title.as_ref().map(ToString::to_string).unwrap_or_default())))
            .collect();
        let mut candidates = Vec::new();
        for (id, node) in syntax.nodes().iter().enumerate() {
            let kind = match node.kind {
                SyntaxKind::Block(TagEnd::Paragraph) => RichBlockKind::Paragraph,
                SyntaxKind::Block(TagEnd::Heading(level)) => RichBlockKind::Heading { level: level as u8 },
                SyntaxKind::Block(TagEnd::Item) if !node.children.iter().any(|&child| matches!(syntax.nodes()[child].kind, SyntaxKind::Block(_))) => RichBlockKind::BulletedList,
                _ => continue,
            };
            let projection = syntax.project_inline(id, &text).map_err(|e| e.to_string())?;
            let range = match (node.children.first(), node.children.last()) {
                (Some(first), Some(last)) => syntax.nodes()[*first].source.start..syntax.nodes()[*last].source.end,
                _ => node.source.start..node.source.start,
            };
            candidates.push((kind, range, projection));
        }
        let body: Vec<_> = records.iter().filter(|r| r.kind_tag != kind_tag_for_rich_block_kind(&RichBlockKind::DocumentTitle)).collect();
        if candidates.is_empty() && text.trim().is_empty() && body.len() == 1 {
            candidates.push((RichBlockKind::Paragraph, 0..0, Default::default()));
        }
        if candidates.len() != body.len() {
            return Err("Markdown projection requires one supported inline block per source anchor; complex blocks need a container-aware bridge".into());
        }
        let payloads: HashMap<_, _> = payloads.iter().map(|p| (p.block_id, p)).collect();
        let mut positions = HashMap::new();
        let mut ranges = Vec::new();
        for (record, (kind, range, projection)) in body.iter().zip(candidates) {
            let payload = payloads.get(&record.id).ok_or("Markdown initialization requires each block projection")?;
            let BlockPayload::RichText { spans } = &payload.payload else { return Err("Markdown anchor is not a text payload".into()); };
            let compatible = payload.kind == kind
                || matches!((&payload.kind, &kind), (RichBlockKind::Quote, RichBlockKind::Paragraph) | (RichBlockKind::NumberedList, RichBlockKind::BulletedList));
            if !compatible || !equivalent_spans(spans, &projection.spans) {
                return Err(format!("Markdown source and block {} projection disagree", record.id));
            }
            // Validate fragment context at cold start, not on the first key.
            if ranges.last().is_some_and(|last: &Range<usize>| last.end > range.start) {
                return Err("overlapping Markdown block anchors".into());
            }
            positions.insert(record.id, ranges.len());
            ranges.push(range);
        }
        let lengths = ranges.iter().map(Range::len).collect();
        let shifts = vec![0; ranges.len() + 1];
        Ok(Self { source, positions, ranges, lengths, shifts, references })
    }

    pub(super) fn source(&self) -> &MarkdownSource { &self.source }

    fn range(&self, block_id: BlockId) -> Result<Range<usize>, String> {
        let &position = self.positions.get(&block_id).ok_or("block has no Markdown source anchor")?;
        let mut index = position;
        let mut shift = 0;
        while index > 0 {
            shift += self.shifts[index];
            index &= index - 1;
        }
        let start = self.ranges[position].start.saturating_add_signed(shift);
        Ok(start..start + self.lengths[position])
    }

    pub(super) fn block_source(&self, block_id: BlockId) -> Result<String, String> {
        self.source.slice(self.range(block_id)?).map_err(|e| e.to_string())
    }

    /// Validate every patch against the same source epoch before applying any.
    /// Source histories are carried by runtime text snapshots / EditTransaction.
    pub(super) fn commit(&mut self, changes: &[MarkdownBlockChange], reverse: bool) -> Result<(), String> {
        let mut edits = Vec::new();
        let mut seen = HashSet::new();
        for change in changes {
            if !seen.insert(change.block_id) { return Err("duplicate Markdown patch".into()); }
            let range = self.range(change.block_id)?;
            let (before, after) = if reverse { (&change.after, &change.before) } else { (&change.before, &change.after) };
            if self.source.slice(range.clone()).map_err(|e| e.to_string())? != *before {
                return Err("stale Markdown source patch".into());
            }
            edits.push(SourceEdit { range: ByteOffset(range.start)..ByteOffset(range.end), replacement: after.clone() });
        }
        self.source.apply_without_history(SourceTransaction {
            revision: self.source.revision(),
            edits,
            // Visible selection is owned by runtime; source-local selection is
            // not exposed as a second cursor truth.
            after_selection: SourceSelection { anchor: ByteOffset(0), focus: ByteOffset(0) },
        }).map_err(|e| e.to_string())?;
        for change in changes {
            let position = self.positions[&change.block_id];
            let new_length = if reverse { change.before.len() } else { change.after.len() };
            let delta = new_length as isize - self.lengths[position] as isize;
            self.lengths[position] = new_length;
            let mut index = position + 1;
            while index < self.shifts.len() {
                self.shifts[index] += delta;
                index += index & index.wrapping_neg();
            }
        }
        Ok(())
    }
}
