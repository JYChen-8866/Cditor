use std::ops::Range;

use cditor_core::edit::TextAffinity;
use gpui::{AnyElement, FontStyle, FontWeight, IntoElement, ScrollHandle};

#[cfg(test)]
use super::InlineBoxRenderer;
use super::metrics::text_layout_options;
use super::{RichTextElement, RichTextGpuiElement, RichTextTypography};
use crate::platform::BODY_FONT_FAMILY;
use crate::text::{
    InlineBoxSpec, RichTextLayoutInput, TextHitPoint, TextLayoutCacheRequest, TextLayoutPosition,
    TextLayoutRect, TextLayoutSelection, TextLayoutSelectionKind, TextLayoutSnapshot,
    cached_text_layout_with_request,
};
#[cfg(test)]
use crate::text::{PositionedInlineBox, TextCaretRect};
use crate::theme::GuiTheme;

impl RichTextElement {
    pub fn new(input: RichTextLayoutInput, theme: GuiTheme) -> Self {
        Self {
            input,
            theme,
            caret_offset: None,
            caret_affinity: TextAffinity::Downstream,
            marked_range: None,
            selection_range: None,
            search_ranges: Vec::new(),
            base_text_color: None,
            typography: RichTextTypography::default(),
            placeholder_text: None,
            inline_boxes: Vec::new(),
            inline_box_renderer: None,
            caret_reveal_scroll_handle: None,
            input_handler: None,
            require_prewarmed_layout: false,
        }
    }

    pub fn with_caret(mut self, caret_offset: Option<usize>) -> Self {
        self.caret_offset = caret_offset;
        self
    }

    pub fn with_caret_affinity(mut self, affinity: TextAffinity) -> Self {
        self.caret_affinity = affinity;
        self
    }

    pub fn with_marked_range(mut self, marked_range: Option<Range<usize>>) -> Self {
        self.marked_range = marked_range;
        self
    }

    pub fn with_selection_range(mut self, selection_range: Option<Range<usize>>) -> Self {
        self.selection_range = selection_range;
        self
    }

    pub(crate) fn with_search_ranges(
        mut self,
        search_ranges: Vec<crate::text::TextSearchRange>,
    ) -> Self {
        self.search_ranges = search_ranges;
        self
    }

    pub fn with_base_text_color(mut self, color: Option<u32>) -> Self {
        self.base_text_color = color;
        self
    }

    pub fn with_typography(mut self, typography: RichTextTypography) -> Self {
        self.typography = typography;
        self
    }

    pub fn with_placeholder(mut self, placeholder: Option<impl Into<String>>) -> Self {
        self.placeholder_text = placeholder.map(Into::into);
        self
    }

    pub fn with_caret_reveal_scroll_handle(mut self, scroll_handle: Option<ScrollHandle>) -> Self {
        self.caret_reveal_scroll_handle = scroll_handle;
        self
    }

    pub(crate) fn with_prewarmed_layout(mut self) -> Self {
        self.require_prewarmed_layout = true;
        self
    }

    #[cfg(test)]
    pub fn with_inline_boxes(
        mut self,
        inline_boxes: Vec<InlineBoxSpec>,
        renderer: InlineBoxRenderer,
    ) -> Self {
        self.inline_boxes = inline_boxes;
        self.inline_box_renderer = Some(renderer);
        self
    }

    #[cfg(test)]
    pub fn hit_test(&self, point: TextHitPoint) -> usize {
        self.hit_test_position(point).offset
    }

    pub fn hit_test_position(&self, point: TextHitPoint) -> TextLayoutPosition {
        self.default_text_layout()
            .position_for_point(point.x as f32, point.y as f32)
    }

    /// Cold interaction fallback: derive multi-click selection from the exact
    /// same cached synchronous layout used for the caret hit.
    pub fn selection_at_point(
        &self,
        point: TextHitPoint,
        kind: TextLayoutSelectionKind,
    ) -> TextLayoutSelection {
        self.default_text_layout()
            .selection_at_point(point.x as f32, point.y as f32, kind)
    }

    pub(crate) fn local_caret_rect_for_offset(&self, offset: usize) -> TextLayoutRect {
        self.default_text_layout()
            .caret_rect(TextLayoutPosition::downstream(offset), 1.0)
    }

    pub(crate) fn local_rects_for_range(&self, range: Range<usize>) -> Vec<TextLayoutRect> {
        self.default_text_layout().range_rects(range)
    }

    #[cfg(test)]
    pub fn candidate_rect_for_offset(&self, offset: usize) -> TextCaretRect {
        let rect = self.local_caret_rect_for_offset(offset);
        TextCaretRect {
            x: rect.x as f64,
            y: rect.y as f64,
            width: rect.width as f64,
            height: rect.height as f64,
        }
    }

    #[cfg(test)]
    pub fn candidate_rect_for_caret(&self) -> Option<TextCaretRect> {
        self.caret_offset
            .map(|offset| self.candidate_rect_for_offset(offset))
    }

    #[cfg(test)]
    pub fn positioned_inline_boxes(&self) -> Vec<PositionedInlineBox> {
        self.default_text_layout().inline_boxes()
    }

    fn default_text_layout(&self) -> TextLayoutSnapshot {
        default_text_layout_for_input(
            &self.input,
            self.theme,
            self.base_text_color,
            self.typography,
            self.inline_boxes.clone(),
            self.input_handler
                .as_ref()
                .is_some_and(|handler| handler.focused),
        )
    }

    pub fn render(&self) -> AnyElement {
        RichTextGpuiElement {
            input: self.input.clone(),
            theme: self.theme,
            caret_offset: self.caret_offset,
            caret_affinity: self.caret_affinity,
            marked_range: self.marked_range.clone(),
            selection_range: self.selection_range.clone(),
            search_ranges: self.search_ranges.clone(),
            base_text_color: self.base_text_color,
            typography: self.typography,
            placeholder_text: self.placeholder_text.clone(),
            inline_boxes: self.inline_boxes.clone(),
            inline_box_renderer: self.inline_box_renderer.clone(),
            caret_reveal_scroll_handle: self.caret_reveal_scroll_handle.clone(),
            input_handler: self.input_handler.clone(),
            require_prewarmed_layout: self.require_prewarmed_layout,
        }
        .into_any_element()
    }
}

pub(super) fn default_text_layout_for_input(
    input: &RichTextLayoutInput,
    theme: GuiTheme,
    base_text_color: Option<u32>,
    typography: RichTextTypography,
    inline_boxes: Vec<InlineBoxSpec>,
    editing: bool,
) -> TextLayoutSnapshot {
    let cache_request = if editing {
        TextLayoutCacheRequest::editing()
    } else {
        TextLayoutCacheRequest::visible()
    };
    cached_text_layout_with_request(
        input,
        theme,
        &text_layout_options(
            input,
            theme,
            base_text_color,
            BODY_FONT_FAMILY,
            FontWeight::NORMAL,
            FontStyle::Normal,
            1.0,
            Some(input.width_px as f32),
            typography,
            inline_boxes,
        ),
        cache_request,
    )
    .layout
}

impl IntoElement for RichTextGpuiElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
