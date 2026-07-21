use cditor_core::edit::TextAffinity;
use parley::{Affinity, Cursor, Selection};

use super::ParleyLayoutSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParleyTextPosition {
    pub offset: usize,
    pub affinity: TextAffinity,
}

impl ParleyTextPosition {
    pub const fn downstream(offset: usize) -> Self {
        Self {
            offset,
            affinity: TextAffinity::Downstream,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParleyRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParleySelection {
    pub anchor: ParleyTextPosition,
    pub focus: ParleyTextPosition,
}

impl ParleySelection {
    pub fn text_range(self) -> std::ops::Range<usize> {
        self.anchor.offset.min(self.focus.offset)..self.anchor.offset.max(self.focus.offset)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParleySelectionKind {
    Cluster,
    Word,
    Line,
    HardLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParleyMoveCommand {
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

impl ParleyLayoutSnapshot {
    pub fn position_for_point(&self, x: f32, y: f32) -> ParleyTextPosition {
        let scale = self.display_scale();
        cursor_to_position(
            self.text(),
            Cursor::from_point(self.layout(), x * scale, y * scale),
        )
    }

    pub fn caret_rect(&self, position: ParleyTextPosition, width: f32) -> ParleyRect {
        let scale = self.display_scale();
        let cursor = position_to_cursor(self.layout(), self.text(), position);
        bounding_box_to_rect(cursor.geometry(self.layout(), width * scale), scale)
    }

    pub fn selection_rects(&self, selection: ParleySelection) -> Vec<ParleyRect> {
        let scale = self.display_scale();
        positions_to_selection(self.layout(), self.text(), selection)
            .geometry(self.layout())
            .into_iter()
            .map(|(bounds, _)| bounding_box_to_rect(bounds, scale))
            .collect()
    }

    pub fn selection_at_point(&self, x: f32, y: f32, kind: ParleySelectionKind) -> ParleySelection {
        let scale = self.display_scale();
        let x = x * scale;
        let y = y * scale;
        let selection = match kind {
            ParleySelectionKind::Cluster => Selection::from_point(self.layout(), x, y),
            ParleySelectionKind::Word => Selection::word_from_point(self.layout(), x, y),
            ParleySelectionKind::Line => Selection::line_from_point(self.layout(), x, y),
            ParleySelectionKind::HardLine => Selection::hard_line_from_point(self.layout(), x, y),
        };
        selection_to_positions(self.text(), selection)
    }

    pub fn move_selection(
        &self,
        selection: ParleySelection,
        command: ParleyMoveCommand,
        extend: bool,
    ) -> ParleySelection {
        let selection = positions_to_selection(self.layout(), self.text(), selection);
        let moved = match command {
            ParleyMoveCommand::PreviousVisual => selection.previous_visual(self.layout(), extend),
            ParleyMoveCommand::NextVisual => selection.next_visual(self.layout(), extend),
            ParleyMoveCommand::PreviousVisualWord => {
                selection.previous_visual_word(self.layout(), extend)
            }
            ParleyMoveCommand::NextVisualWord => selection.next_visual_word(self.layout(), extend),
            ParleyMoveCommand::PreviousLogicalWord => {
                let focus = selection.focus().previous_logical_word(self.layout());
                if extend {
                    selection.extend(focus)
                } else {
                    focus.into()
                }
            }
            ParleyMoveCommand::NextLogicalWord => {
                let focus = selection.focus().next_logical_word(self.layout());
                if extend {
                    selection.extend(focus)
                } else {
                    focus.into()
                }
            }
            ParleyMoveCommand::PreviousLine => selection.previous_line(self.layout(), extend),
            ParleyMoveCommand::NextLine => selection.next_line(self.layout(), extend),
            ParleyMoveCommand::LineStart => selection.line_start(self.layout(), extend),
            ParleyMoveCommand::LineEnd => selection.line_end(self.layout(), extend),
            ParleyMoveCommand::HardLineStart => selection.hard_line_start(self.layout(), extend),
            ParleyMoveCommand::HardLineEnd => selection.hard_line_end(self.layout(), extend),
        };
        selection_to_positions(self.text(), moved)
    }

    pub fn move_selection_with_preferred_x(
        &self,
        selection: ParleySelection,
        command: ParleyMoveCommand,
        extend: bool,
        preferred_x: Option<f32>,
    ) -> (ParleySelection, Option<f32>) {
        if !matches!(
            command,
            ParleyMoveCommand::PreviousLine | ParleyMoveCommand::NextLine
        ) {
            return (self.move_selection(selection, command, extend), None);
        }
        let current_rect = self.caret_rect(selection.focus, 1.0);
        let preferred_x = preferred_x.unwrap_or(current_rect.x);
        let initial = self.move_selection(selection, command, extend);
        let target_rect = self.caret_rect(initial.focus, 1.0);
        let focus = self.position_for_point(preferred_x, target_rect.y + target_rect.height * 0.5);
        let selection = if extend {
            ParleySelection {
                anchor: selection.anchor,
                focus,
            }
        } else {
            ParleySelection {
                anchor: focus,
                focus,
            }
        };
        (selection, Some(preferred_x))
    }
}

fn position_to_cursor(
    layout: &parley::Layout<super::ParleyBrush>,
    text: &str,
    position: ParleyTextPosition,
) -> Cursor {
    Cursor::from_byte_index(
        layout,
        normalize_byte_offset(text, position.offset, position.affinity),
        affinity_to_parley(position.affinity),
    )
}

fn positions_to_selection(
    layout: &parley::Layout<super::ParleyBrush>,
    text: &str,
    selection: ParleySelection,
) -> Selection {
    Selection::new(
        position_to_cursor(layout, text, selection.anchor),
        position_to_cursor(layout, text, selection.focus),
    )
}

fn selection_to_positions(text: &str, selection: Selection) -> ParleySelection {
    ParleySelection {
        anchor: cursor_to_position(text, selection.anchor()),
        focus: cursor_to_position(text, selection.focus()),
    }
}

fn cursor_to_position(text: &str, cursor: Cursor) -> ParleyTextPosition {
    let affinity = affinity_from_parley(cursor.affinity());
    ParleyTextPosition {
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

fn bounding_box_to_rect(bounds: parley::BoundingBox, scale: f32) -> ParleyRect {
    ParleyRect {
        x: bounds.x0 as f32 / scale,
        y: bounds.y0 as f32 / scale,
        width: (bounds.x1 - bounds.x0) as f32 / scale,
        height: (bounds.y1 - bounds.y0) as f32 / scale,
    }
}
