use std::ops::Range;

use ropey::Rope;
use super::{MarkdownError, MarkdownResult, MarkdownSyntax};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ByteOffset(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Affinity { Before, After }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSelection {
    pub anchor: ByteOffset,
    pub focus: ByteOffset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEdit {
    /// All ranges refer to the transaction's base revision, not preceding edits.
    pub range: Range<ByteOffset>,
    pub replacement: String,
}

#[derive(Debug, Clone)]
pub struct SourceTransaction {
    pub revision: u64,
    pub edits: Vec<SourceEdit>,
    pub after_selection: SourceSelection,
}

#[derive(Debug, Clone)]
struct HistoryEntry {
    forward: Vec<SourceEdit>,
    inverse: Vec<SourceEdit>,
    before: SourceSelection,
    after: SourceSelection,
}

/// Source transaction foundation. The host owns grouping/composition and must
/// not mix this history with an independently mutating rich-text undo stack.
#[derive(Debug, Clone)]
pub struct MarkdownSource {
    rope: Rope,
    revision: u64,
    selection: SourceSelection,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

impl MarkdownSource {
    pub fn new(source: &str) -> Self {
        Self {
            rope: Rope::from_str(source), revision: 0,
            selection: SourceSelection { anchor: ByteOffset(0), focus: ByteOffset(0) },
            undo: vec![], redo: vec![],
        }
    }

    pub fn text(&self) -> String { self.rope.to_string() }
    pub fn slice(&self, range: Range<usize>) -> MarkdownResult<String> {
        if range.start > range.end { return Err(MarkdownError::InvalidRange); }
        let start = checked_char(&self.rope, range.start)?;
        let end = checked_char(&self.rope, range.end)?;
        Ok(self.rope.slice(start..end).to_string())
    }
    pub fn revision(&self) -> u64 { self.revision }
    pub fn selection(&self) -> &SourceSelection { &self.selection }
    /// Explicit full parse; never implicit in `apply`, undo or redo.
    pub fn parse(&self) -> MarkdownSyntax { MarkdownSyntax::parse(&self.text()) }

    pub fn set_selection(&mut self, selection: SourceSelection) -> MarkdownResult<()> {
        validate_selection(&self.rope, &selection)?;
        self.selection = selection;
        Ok(())
    }

    pub fn apply(&mut self, mut transaction: SourceTransaction) -> MarkdownResult<()> {
        if transaction.revision != self.revision { return Err(MarkdownError::StaleRevision); }
        transaction.edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
        let (next, inverse) = apply_edits(&self.rope, &transaction.edits)?;
        validate_selection(&next, &transaction.after_selection)?;
        if next == self.rope {
            self.selection = transaction.after_selection;
            return Ok(());
        }
        self.undo.push(HistoryEntry {
            forward: transaction.edits, inverse,
            before: self.selection.clone(), after: transaction.after_selection.clone(),
        });
        self.redo.clear();
        self.rope = next;
        self.selection = transaction.after_selection;
        self.revision += 1;
        Ok(())
    }

    /// Applies an already validated host transaction without changing this
    /// object's own undo timeline. A document runtime with a richer selection
    /// and layout history uses this to keep one user-visible undo stack.
    pub fn apply_without_history(&mut self, mut transaction: SourceTransaction) -> MarkdownResult<()> {
        if transaction.revision != self.revision { return Err(MarkdownError::StaleRevision); }
        transaction.edits.sort_by_key(|edit| (edit.range.start, edit.range.end));
        let (next, _) = apply_edits(&self.rope, &transaction.edits)?;
        validate_selection(&next, &transaction.after_selection)?;
        if next != self.rope {
            self.rope = next;
            self.revision += 1;
        }
        self.selection = transaction.after_selection;
        Ok(())
    }

    /// Restores source bytes from a runtime-owned history snapshot. This is
    /// intentionally not a user-visible source undo operation.
    pub fn restore_without_history(
        &mut self,
        source: &str,
        selection: SourceSelection,
    ) -> MarkdownResult<()> {
        let rope = Rope::from_str(source);
        validate_selection(&rope, &selection)?;
        self.rope = rope;
        self.selection = selection;
        self.revision += 1;
        Ok(())
    }

    pub fn undo(&mut self) -> MarkdownResult<bool> {
        let Some(entry) = self.undo.last() else { return Ok(false); };
        let (next, _) = apply_edits(&self.rope, &entry.inverse)?;
        let entry = self.undo.pop().unwrap();
        self.rope = next;
        self.selection = entry.before.clone();
        self.redo.push(entry);
        self.revision += 1;
        Ok(true)
    }

    pub fn redo(&mut self) -> MarkdownResult<bool> {
        let Some(entry) = self.redo.last() else { return Ok(false); };
        let (next, _) = apply_edits(&self.rope, &entry.forward)?;
        let entry = self.redo.pop().unwrap();
        self.rope = next;
        self.selection = entry.after.clone();
        self.undo.push(entry);
        self.revision += 1;
        Ok(true)
    }

    pub fn byte_to_utf16(&self, offset: ByteOffset) -> MarkdownResult<usize> {
        let char_index = checked_char(&self.rope, offset.0)?;
        Ok(self.rope.char_to_utf16_cu(char_index))
    }

    pub fn utf16_to_byte(&self, offset: usize) -> MarkdownResult<ByteOffset> {
        if offset > self.rope.len_utf16_cu() { return Err(MarkdownError::InvalidRange); }
        let ch = self.rope.utf16_cu_to_char(offset);
        if self.rope.char_to_utf16_cu(ch) != offset { return Err(MarkdownError::InvalidRange); }
        Ok(ByteOffset(self.rope.char_to_byte(ch)))
    }
}

fn checked_char(rope: &Rope, byte: usize) -> MarkdownResult<usize> {
    if byte > rope.len_bytes() { return Err(MarkdownError::InvalidRange); }
    let ch = rope.byte_to_char(byte);
    if rope.char_to_byte(ch) != byte { return Err(MarkdownError::InvalidRange); }
    Ok(ch)
}

fn validate_selection(rope: &Rope, selection: &SourceSelection) -> MarkdownResult<()> {
    checked_char(rope, selection.anchor.0)?;
    checked_char(rope, selection.focus.0)?;
    Ok(())
}

fn apply_edits(rope: &Rope, edits: &[SourceEdit]) -> MarkdownResult<(Rope, Vec<SourceEdit>)> {
    let mut inverse = Vec::new();
    let mut removed = 0;
    let mut inserted = 0;
    let mut previous: Option<&SourceEdit> = None;
    for edit in edits {
        let start = edit.range.start.0;
        let end = edit.range.end.0;
        if start > end { return Err(MarkdownError::InvalidRange); }
        if previous.is_some_and(|p| p.range.end.0 > start || p.range.start.0 == start) {
            return Err(MarkdownError::OverlappingEdits);
        }
        let from = checked_char(rope, start)?;
        let to = checked_char(rope, end)?;
        let new_start = start - removed + inserted;
        inverse.push(SourceEdit {
            range: ByteOffset(new_start)..ByteOffset(new_start + edit.replacement.len()),
            replacement: rope.slice(from..to).to_string(),
        });
        removed += end - start;
        inserted += edit.replacement.len();
        previous = Some(edit);
    }
    // Adjacent deletions produce inverse insertions at the same position: merge
    // these in source order, so inverse validation and replay remain unambiguous.
    let mut merged: Vec<SourceEdit> = vec![];
    for edit in inverse {
        if let Some(last) = merged.last_mut().filter(|last| last.range.end == edit.range.start) {
            last.range.end = edit.range.end;
            last.replacement.push_str(&edit.replacement);
        } else { merged.push(edit); }
    }
    let mut next = rope.clone();
    for edit in edits.iter().rev() {
        let from = next.byte_to_char(edit.range.start.0);
        let to = next.byte_to_char(edit.range.end.0);
        next.remove(from..to);
        next.insert(from, &edit.replacement);
    }
    Ok((next, merged))
}
