use std::ops::Range;

use cditor_core::edit::TextAffinity;
use parley::{Affinity, Cursor, Selection};

use super::TextLayoutSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextLayoutPosition {
    pub offset: usize,
    pub affinity: TextAffinity,
}

impl TextLayoutPosition {
    pub const fn downstream(offset: usize) -> Self {
        Self {
            offset,
            affinity: TextAffinity::Downstream,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextLayoutRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextLayoutSelection {
    pub anchor: TextLayoutPosition,
    pub focus: TextLayoutPosition,
}

impl TextLayoutSelection {
    pub fn text_range(self) -> std::ops::Range<usize> {
        self.anchor.offset.min(self.focus.offset)..self.anchor.offset.max(self.focus.offset)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextLayoutSelectionKind {
    Cluster,
    Word,
    Line,
    HardLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextLayoutMoveCommand {
    PreviousVisual,
    NextVisual,
    PreviousVisualWord,
    NextVisualWord,
    PreviousLogicalWord,
    NextLogicalWord,
    PreviousLine,
    NextLine,
    LineStart,
    LineEnd,
    HardLineStart,
    HardLineEnd,
}

impl TextLayoutSnapshot {
    pub fn position_for_point(&self, x: f32, y: f32) -> TextLayoutPosition {
        self.geometry().position_for_point(x, y)
    }

    pub fn caret_rect(&self, position: TextLayoutPosition, width: f32) -> TextLayoutRect {
        self.geometry().caret_rect(self.text(), position, width)
    }

    pub fn selection_rects(&self, selection: TextLayoutSelection) -> Vec<TextLayoutRect> {
        self.geometry().selection_rects(self.text(), selection)
    }

    pub fn range_rects(&self, range: Range<usize>) -> Vec<TextLayoutRect> {
        let start = range.start.min(self.text().len());
        let end = range.end.min(self.text().len()).max(start);
        if start == end {
            return Vec::new();
        }
        self.selection_rects(TextLayoutSelection {
            anchor: TextLayoutPosition::downstream(start),
            focus: TextLayoutPosition {
                offset: end,
                affinity: TextAffinity::Upstream,
            },
        })
    }

    pub fn selection_at_point(
        &self,
        x: f32,
        y: f32,
        kind: TextLayoutSelectionKind,
    ) -> TextLayoutSelection {
        if kind == TextLayoutSelectionKind::Cluster {
            let position = self.position_for_point(x, y);
            return TextLayoutSelection {
                anchor: position,
                focus: position,
            };
        }
        let scale = self.display_scale();
        let x = x * scale;
        let y = y * scale;
        let selection = match kind {
            TextLayoutSelectionKind::Cluster => unreachable!("cluster hit returned above"),
            TextLayoutSelectionKind::Word => Selection::word_from_point(self.layout(), x, y),
            TextLayoutSelectionKind::Line => Selection::line_from_point(self.layout(), x, y),
            TextLayoutSelectionKind::HardLine => {
                Selection::hard_line_from_point(self.layout(), x, y)
            }
        };
        selection_to_positions(self.text(), selection)
    }

    pub fn move_selection(
        &self,
        selection: TextLayoutSelection,
        command: TextLayoutMoveCommand,
        extend: bool,
    ) -> TextLayoutSelection {
        let selection = positions_to_selection(self.layout(), self.text(), selection);
        let moved = match command {
            TextLayoutMoveCommand::PreviousVisual => {
                selection.previous_visual(self.layout(), extend)
            }
            TextLayoutMoveCommand::NextVisual => selection.next_visual(self.layout(), extend),
            TextLayoutMoveCommand::PreviousVisualWord => {
                selection.previous_visual_word(self.layout(), extend)
            }
            TextLayoutMoveCommand::NextVisualWord => {
                selection.next_visual_word(self.layout(), extend)
            }
            TextLayoutMoveCommand::PreviousLogicalWord => {
                let focus = selection.focus().previous_logical_word(self.layout());
                if extend {
                    selection.extend(focus)
                } else {
                    focus.into()
                }
            }
            TextLayoutMoveCommand::NextLogicalWord => {
                let focus = selection.focus().next_logical_word(self.layout());
                if extend {
                    selection.extend(focus)
                } else {
                    focus.into()
                }
            }
            TextLayoutMoveCommand::PreviousLine => selection.previous_line(self.layout(), extend),
            TextLayoutMoveCommand::NextLine => selection.next_line(self.layout(), extend),
            TextLayoutMoveCommand::LineStart => selection.line_start(self.layout(), extend),
            TextLayoutMoveCommand::LineEnd => selection.line_end(self.layout(), extend),
            TextLayoutMoveCommand::HardLineStart => {
                selection.hard_line_start(self.layout(), extend)
            }
            TextLayoutMoveCommand::HardLineEnd => selection.hard_line_end(self.layout(), extend),
        };
        selection_to_positions(self.text(), moved)
    }

    pub fn move_selection_with_preferred_x(
        &self,
        selection: TextLayoutSelection,
        command: TextLayoutMoveCommand,
        extend: bool,
        preferred_x: Option<f32>,
    ) -> (TextLayoutSelection, Option<f32>) {
        if !matches!(
            command,
            TextLayoutMoveCommand::PreviousLine | TextLayoutMoveCommand::NextLine
        ) {
            return (self.move_selection(selection, command, extend), None);
        }
        let current_rect = self.caret_rect(selection.focus, 1.0);
        let preferred_x = preferred_x.unwrap_or(current_rect.x);
        let initial = self.move_selection(selection, command, extend);
        let target_rect = self.caret_rect(initial.focus, 1.0);
        let focus = self.position_for_point(preferred_x, target_rect.y + target_rect.height * 0.5);
        let selection = if extend {
            TextLayoutSelection {
                anchor: selection.anchor,
                focus,
            }
        } else {
            TextLayoutSelection {
                anchor: focus,
                focus,
            }
        };
        (selection, Some(preferred_x))
    }
}

fn position_to_cursor(
    layout: &parley::Layout<super::TextBrush>,
    text: &str,
    position: TextLayoutPosition,
) -> Cursor {
    Cursor::from_byte_index(
        layout,
        normalize_byte_offset(text, position.offset, position.affinity),
        affinity_to_parley(position.affinity),
    )
}

fn positions_to_selection(
    layout: &parley::Layout<super::TextBrush>,
    text: &str,
    selection: TextLayoutSelection,
) -> Selection {
    Selection::new(
        position_to_cursor(layout, text, selection.anchor),
        position_to_cursor(layout, text, selection.focus),
    )
}

fn selection_to_positions(text: &str, selection: Selection) -> TextLayoutSelection {
    TextLayoutSelection {
        anchor: cursor_to_position(text, selection.anchor()),
        focus: cursor_to_position(text, selection.focus()),
    }
}

fn cursor_to_position(text: &str, cursor: Cursor) -> TextLayoutPosition {
    let affinity = affinity_from_parley(cursor.affinity());
    TextLayoutPosition {
        offset: normalize_byte_offset(text, cursor.index(), affinity),
        affinity,
    }
}

fn normalize_byte_offset(text: &str, offset: usize, affinity: TextAffinity) -> usize {
    let offset = offset.min(text.len());
    if text.is_char_boundary(offset) {
        return offset;
    }
    match affinity {
        TextAffinity::Upstream => (0..offset)
            .rev()
            .find(|candidate| text.is_char_boundary(*candidate))
            .unwrap_or(0),
        TextAffinity::Downstream => (offset + 1..=text.len())
            .find(|candidate| text.is_char_boundary(*candidate))
            .unwrap_or(text.len()),
    }
}

fn affinity_to_parley(affinity: TextAffinity) -> Affinity {
    match affinity {
        TextAffinity::Upstream => Affinity::Upstream,
        TextAffinity::Downstream => Affinity::Downstream,
    }
}

fn affinity_from_parley(affinity: Affinity) -> TextAffinity {
    match affinity {
        Affinity::Upstream => TextAffinity::Upstream,
        Affinity::Downstream => TextAffinity::Downstream,
    }
}
