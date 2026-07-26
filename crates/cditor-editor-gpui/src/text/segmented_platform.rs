use std::{ops::Range, sync::Arc};

use cditor_core::edit::TextAffinity;

use super::{
    TextLayoutMoveCommand, TextLayoutPosition, TextLayoutRect, TextLayoutSelection,
    TextLayoutSelectionKind, TextLayoutSnapshot,
};

#[derive(Clone)]
pub(crate) struct SegmentedLayoutFragment {
    pub(crate) byte_range: Range<usize>,
    pub(crate) top_px: f32,
    pub(crate) snapshot: TextLayoutSnapshot,
}

#[derive(Clone)]
pub(crate) struct SegmentedPlatformLayout {
    text: Arc<str>,
    fragments: Arc<[SegmentedLayoutFragment]>,
    estimated_line_count: usize,
}

impl SegmentedPlatformLayout {
    pub(crate) fn new(
        text: Arc<str>,
        fragments: Vec<SegmentedLayoutFragment>,
        estimated_line_count: usize,
    ) -> Self {
        Self {
            text,
            fragments: fragments.into(),
            estimated_line_count,
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn line_count(&self) -> usize {
        self.estimated_line_count
    }

    pub(crate) fn position_for_point(&self, x: f32, y: f32) -> TextLayoutPosition {
        let Some(fragment) = self.fragment_for_y(y) else {
            return TextLayoutPosition::downstream(self.text.len());
        };
        let mut position = fragment
            .snapshot
            .position_for_point(x, (y - fragment.top_px).max(0.0));
        position.offset = position
            .offset
            .saturating_add(fragment.byte_range.start)
            .min(fragment.byte_range.end);
        position
    }

    pub(crate) fn caret_rect(&self, position: TextLayoutPosition, width: f32) -> TextLayoutRect {
        let Some(fragment) = self.fragment_for_offset(position.offset) else {
            return TextLayoutRect {
                x: 0.0,
                y: 0.0,
                width,
                height: 0.0,
            };
        };
        let local = TextLayoutPosition {
            offset: position
                .offset
                .saturating_sub(fragment.byte_range.start)
                .min(fragment.snapshot.text().len()),
            affinity: position.affinity,
        };
        let mut rect = fragment.snapshot.caret_rect(local, width);
        rect.y += fragment.top_px;
        rect
    }

    pub(crate) fn range_rects(&self, range: Range<usize>) -> Vec<TextLayoutRect> {
        let mut rects = Vec::new();
        for fragment in self.fragments.iter() {
            let start = range.start.max(fragment.byte_range.start);
            let end = range.end.min(fragment.byte_range.end);
            if start >= end {
                continue;
            }
            let local = start - fragment.byte_range.start..end - fragment.byte_range.start;
            rects.extend(
                fragment
                    .snapshot
                    .range_rects(local)
                    .into_iter()
                    .map(|mut rect| {
                        rect.y += fragment.top_px;
                        rect
                    }),
            );
        }
        rects
    }

    pub(crate) fn selection_at_point(
        &self,
        x: f32,
        y: f32,
        kind: TextLayoutSelectionKind,
    ) -> TextLayoutSelection {
        let Some(fragment) = self.fragment_for_y(y) else {
            let end = TextLayoutPosition::downstream(self.text.len());
            return TextLayoutSelection {
                anchor: end,
                focus: end,
            };
        };
        let mut selection =
            fragment
                .snapshot
                .selection_at_point(x, (y - fragment.top_px).max(0.0), kind);
        selection.anchor.offset = selection
            .anchor
            .offset
            .saturating_add(fragment.byte_range.start)
            .min(fragment.byte_range.end);
        selection.focus.offset = selection
            .focus
            .offset
            .saturating_add(fragment.byte_range.start)
            .min(fragment.byte_range.end);
        selection
    }

    pub(crate) fn move_selection_with_preferred_x(
        &self,
        selection: TextLayoutSelection,
        command: TextLayoutMoveCommand,
        extend: bool,
        preferred_x: Option<f32>,
    ) -> (TextLayoutSelection, Option<f32>) {
        let Some((index, fragment)) = self.fragment_with_index_for_offset(selection.focus.offset)
        else {
            return (selection, preferred_x);
        };
        let local_focus = self.local_position(fragment, selection.focus);
        let local_selection = TextLayoutSelection {
            anchor: local_focus,
            focus: local_focus,
        };
        let (mut moved, preferred_x) = fragment.snapshot.move_selection_with_preferred_x(
            local_selection,
            command,
            false,
            preferred_x,
        );
        moved.focus.offset = moved
            .focus
            .offset
            .saturating_add(fragment.byte_range.start)
            .min(fragment.byte_range.end);

        if moved.focus.offset == selection.focus.offset {
            moved.focus = self.cross_fragment_position(index, command, preferred_x, moved.focus);
        }
        moved.anchor = if extend {
            selection.anchor
        } else {
            moved.focus
        };
        (moved, preferred_x)
    }

    fn cross_fragment_position(
        &self,
        index: usize,
        command: TextLayoutMoveCommand,
        preferred_x: Option<f32>,
        fallback: TextLayoutPosition,
    ) -> TextLayoutPosition {
        let previous = matches!(
            command,
            TextLayoutMoveCommand::PreviousVisual
                | TextLayoutMoveCommand::PreviousVisualWord
                | TextLayoutMoveCommand::PreviousLogicalWord
                | TextLayoutMoveCommand::PreviousLine
        );
        let next = matches!(
            command,
            TextLayoutMoveCommand::NextVisual
                | TextLayoutMoveCommand::NextVisualWord
                | TextLayoutMoveCommand::NextLogicalWord
                | TextLayoutMoveCommand::NextLine
        );
        let target = if previous {
            index
                .checked_sub(1)
                .and_then(|index| self.fragments.get(index))
        } else if next {
            self.fragments.get(index + 1)
        } else {
            None
        };
        let Some(target) = target else {
            return fallback;
        };
        if matches!(
            command,
            TextLayoutMoveCommand::PreviousLine | TextLayoutMoveCommand::NextLine
        ) {
            let y = if previous {
                target.snapshot.height().max(1.0) - 0.5
            } else {
                0.5
            };
            let mut position = target
                .snapshot
                .position_for_point(preferred_x.unwrap_or(0.0), y);
            position.offset += target.byte_range.start;
            return position;
        }
        TextLayoutPosition {
            offset: if previous {
                target.byte_range.end
            } else {
                target.byte_range.start
            },
            affinity: if previous {
                TextAffinity::Upstream
            } else {
                TextAffinity::Downstream
            },
        }
    }

    fn local_position(
        &self,
        fragment: &SegmentedLayoutFragment,
        position: TextLayoutPosition,
    ) -> TextLayoutPosition {
        TextLayoutPosition {
            offset: position
                .offset
                .saturating_sub(fragment.byte_range.start)
                .min(fragment.snapshot.text().len()),
            affinity: position.affinity,
        }
    }

    fn fragment_for_y(&self, y: f32) -> Option<&SegmentedLayoutFragment> {
        self.fragments
            .iter()
            .find(|fragment| y < fragment.top_px + fragment.snapshot.height())
            .or_else(|| self.fragments.last())
    }

    fn fragment_for_offset(&self, offset: usize) -> Option<&SegmentedLayoutFragment> {
        self.fragment_with_index_for_offset(offset)
            .map(|(_, fragment)| fragment)
    }

    fn fragment_with_index_for_offset(
        &self,
        offset: usize,
    ) -> Option<(usize, &SegmentedLayoutFragment)> {
        self.fragments
            .iter()
            .enumerate()
            .find(|(_, fragment)| {
                fragment.byte_range.contains(&offset)
                    || (offset == self.text.len() && fragment.byte_range.end == offset)
            })
            .or_else(|| {
                self.fragments
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, fragment)| fragment.byte_range.start.abs_diff(offset))
            })
    }
}

#[derive(Clone)]
pub(crate) enum PlatformTextLayoutSnapshot {
    Full(TextLayoutSnapshot),
    Segmented(SegmentedPlatformLayout),
}

impl From<TextLayoutSnapshot> for PlatformTextLayoutSnapshot {
    fn from(snapshot: TextLayoutSnapshot) -> Self {
        Self::Full(snapshot)
    }
}

impl PlatformTextLayoutSnapshot {
    pub(crate) fn text(&self) -> &str {
        match self {
            Self::Full(snapshot) => snapshot.text(),
            Self::Segmented(snapshot) => snapshot.text(),
        }
    }

    pub(crate) fn line_count(&self) -> usize {
        match self {
            Self::Full(snapshot) => snapshot.line_count(),
            Self::Segmented(snapshot) => snapshot.line_count(),
        }
    }

    pub(crate) fn position_for_point(&self, x: f32, y: f32) -> TextLayoutPosition {
        match self {
            Self::Full(snapshot) => snapshot.position_for_point(x, y),
            Self::Segmented(snapshot) => snapshot.position_for_point(x, y),
        }
    }

    pub(crate) fn caret_rect(&self, position: TextLayoutPosition, width: f32) -> TextLayoutRect {
        match self {
            Self::Full(snapshot) => snapshot.caret_rect(position, width),
            Self::Segmented(snapshot) => snapshot.caret_rect(position, width),
        }
    }

    pub(crate) fn range_rects(&self, range: Range<usize>) -> Vec<TextLayoutRect> {
        match self {
            Self::Full(snapshot) => snapshot.range_rects(range),
            Self::Segmented(snapshot) => snapshot.range_rects(range),
        }
    }

    pub(crate) fn selection_at_point(
        &self,
        x: f32,
        y: f32,
        kind: TextLayoutSelectionKind,
    ) -> TextLayoutSelection {
        match self {
            Self::Full(snapshot) => snapshot.selection_at_point(x, y, kind),
            Self::Segmented(snapshot) => snapshot.selection_at_point(x, y, kind),
        }
    }

    pub(crate) fn move_selection_with_preferred_x(
        &self,
        selection: TextLayoutSelection,
        command: TextLayoutMoveCommand,
        extend: bool,
        preferred_x: Option<f32>,
    ) -> (TextLayoutSelection, Option<f32>) {
        match self {
            Self::Full(snapshot) => {
                snapshot.move_selection_with_preferred_x(selection, command, extend, preferred_x)
            }
            Self::Segmented(snapshot) => {
                snapshot.move_selection_with_preferred_x(selection, command, extend, preferred_x)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        text::{
            RichTextLayoutInput, TextLayoutOptions, TextLayoutSurfaceId, TextLineHeight,
            TextStyleConfig, build_text_layout,
        },
        theme::GuiTheme,
    };
    use cditor_core::rich_text::{InlineSpan, RichBlockKind, TextAlign};

    fn fragment(text: &str, byte_range: Range<usize>, top_px: f32) -> SegmentedLayoutFragment {
        let input = RichTextLayoutInput {
            block_id: 1,
            surface_id: TextLayoutSurfaceId::Block(1),
            content_version: 1,
            layout_version: 1,
            kind: RichBlockKind::Code { language: None },
            text_align: TextAlign::Start,
            spans: vec![InlineSpan::plain(text)].into(),
            width_px: 300.0,
            theme_version: 1,
            font_version: 1,
        };
        let snapshot = build_text_layout(
            &input,
            GuiTheme::light(),
            &TextLayoutOptions {
                width: Some(300.0),
                base_style: TextStyleConfig {
                    line_height: TextLineHeight::Absolute(24.0),
                    ..TextStyleConfig::default()
                },
                ..TextLayoutOptions::default()
            },
        );
        SegmentedLayoutFragment {
            byte_range,
            top_px,
            snapshot,
        }
    }

    fn two_segment_layout() -> SegmentedPlatformLayout {
        SegmentedPlatformLayout::new(
            Arc::from("first\nsecond"),
            vec![
                fragment("first", 0..5, 0.0),
                fragment("second", 6..12, 24.0),
            ],
            2,
        )
    }

    #[test]
    fn point_hit_translates_segment_local_offsets_to_global_bytes() {
        let layout = two_segment_layout();
        let position = layout.position_for_point(1.0, 25.0);

        assert_eq!(position.offset, 6);
    }

    #[test]
    fn caret_and_range_geometry_include_segment_top_offset() {
        let layout = two_segment_layout();
        let caret = layout.caret_rect(TextLayoutPosition::downstream(6), 1.0);
        let selection = layout.range_rects(6..12);

        assert!(caret.y >= 24.0);
        assert!(selection.iter().all(|rect| rect.y >= 24.0));
    }

    #[test]
    fn visual_navigation_crosses_segment_boundary_without_local_offset_leak() {
        let layout = two_segment_layout();
        let selection = TextLayoutSelection {
            anchor: TextLayoutPosition::downstream(6),
            focus: TextLayoutPosition::downstream(6),
        };
        let (moved, _) = layout.move_selection_with_preferred_x(
            selection,
            TextLayoutMoveCommand::PreviousVisual,
            false,
            None,
        );

        assert_eq!(moved.focus.offset, 5);
    }

    #[test]
    fn extending_selection_preserves_global_anchor() {
        let layout = two_segment_layout();
        let selection = TextLayoutSelection {
            anchor: TextLayoutPosition::downstream(2),
            focus: TextLayoutPosition::downstream(8),
        };
        let (moved, _) = layout.move_selection_with_preferred_x(
            selection,
            TextLayoutMoveCommand::NextVisual,
            true,
            None,
        );

        assert_eq!(moved.anchor.offset, 2);
        assert!(moved.focus.offset > 8);
    }
}
