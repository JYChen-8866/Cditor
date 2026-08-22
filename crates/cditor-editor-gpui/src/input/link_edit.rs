use std::ops::Range;

use cditor_core::ids::BlockId;

/// Which of the link popup's two single-line inputs owns the platform IME.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkEditField {
    Text,
    Url,
}

/// State for the "add link" popup opened from the text-selection toolbar.
///
/// Both inputs are registered platform-IME targets (the same single input
/// pipeline as `code_language_edit` / `ai_prompt`): composition, candidate
/// geometry, and key routing all flow through the editor's one IME
/// implementation instead of ad-hoc text fields.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LinkEditState {
    pub block_id: BlockId,
    /// Selected byte range in the block the link applies to.
    pub range: Range<usize>,
    /// Label draft, prefilled with the selected text.
    pub text_draft: String,
    pub original_text: String,
    /// Destination draft, prefilled from an existing link on the selection.
    pub href_draft: String,
    pub focused_field: LinkEditField,
    pub caret_offset: usize,
    pub marked_range: Option<Range<usize>>,
    /// Popup anchor captured from the floating toolbar at open time.
    pub x: f32,
    pub y: f32,
}

impl LinkEditState {
    pub fn new(
        block_id: BlockId,
        range: Range<usize>,
        selected_text: String,
        existing_href: Option<String>,
        x: f32,
        y: f32,
    ) -> Self {
        let href_draft = existing_href.unwrap_or_default();
        Self {
            block_id,
            range,
            original_text: selected_text.clone(),
            text_draft: selected_text,
            caret_offset: href_draft.len(),
            href_draft,
            focused_field: LinkEditField::Url,
            marked_range: None,
            x,
            y,
        }
    }

    pub fn active_draft(&self) -> &str {
        match self.focused_field {
            LinkEditField::Text => &self.text_draft,
            LinkEditField::Url => &self.href_draft,
        }
    }

    fn active_draft_mut(&mut self) -> &mut String {
        match self.focused_field {
            LinkEditField::Text => &mut self.text_draft,
            LinkEditField::Url => &mut self.href_draft,
        }
    }

    pub fn focus_field(&mut self, field: LinkEditField) {
        if self.focused_field != field {
            self.focused_field = field;
            self.caret_offset = self.active_draft().len();
            self.marked_range = None;
        }
    }

    pub fn toggle_field(&mut self) {
        self.focus_field(match self.focused_field {
            LinkEditField::Text => LinkEditField::Url,
            LinkEditField::Url => LinkEditField::Text,
        });
    }

    pub fn input_replacement_range(&self) -> Range<usize> {
        self.marked_range
            .clone()
            .unwrap_or(self.caret_offset..self.caret_offset)
    }

    pub fn replace_range(&mut self, range: Range<usize>, text: &str) {
        let field = self.focused_field;
        let caret = {
            let draft = self.active_draft_mut();
            let range = clamp_range(draft, range);
            draft.replace_range(range.clone(), text);
            range.start + text.len()
        };
        let _ = field;
        self.caret_offset = caret;
        self.marked_range = None;
    }

    pub fn replace_and_mark_range(
        &mut self,
        range: Range<usize>,
        text: &str,
        selected_range: Option<Range<usize>>,
    ) {
        let start = {
            let draft = self.active_draft_mut();
            let range = clamp_range(draft, range);
            draft.replace_range(range.clone(), text);
            range.start
        };
        self.marked_range = (!text.is_empty()).then(|| start..start + text.len());
        self.caret_offset = selected_range
            .map(|selected| start + selected.start.min(text.len()))
            .unwrap_or(start + text.len());
    }

    pub fn unmark(&mut self) {
        self.marked_range = None;
    }

    pub fn move_caret_left(&mut self) {
        let draft = self.active_draft();
        self.caret_offset = draft[..self.caret_offset.min(draft.len())]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
    }

    pub fn move_caret_right(&mut self) {
        let draft = self.active_draft();
        let offset = self.caret_offset.min(draft.len());
        self.caret_offset = draft[offset..]
            .chars()
            .next()
            .map(|ch| offset + ch.len_utf8())
            .unwrap_or(offset);
    }

    pub fn delete_backward(&mut self) {
        let offset = {
            let caret = self.caret_offset;
            let draft = self.active_draft();
            draft[..caret.min(draft.len())]
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
        };
        if let Some(start) = offset {
            let caret = self.caret_offset;
            self.active_draft_mut().replace_range(start..caret, "");
            self.caret_offset = start;
            self.marked_range = None;
        }
    }

    /// The href the commit should apply: trimmed, `None` when empty.
    pub fn normalized_href(&self) -> Option<String> {
        let trimmed = self.href_draft.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    }

    /// The label replacement, only when it differs from the original text.
    pub fn label_replacement(&self) -> Option<String> {
        (self.text_draft != self.original_text && !self.text_draft.is_empty())
            .then(|| self.text_draft.clone())
    }
}

fn clamp_range(text: &str, range: Range<usize>) -> Range<usize> {
    let mut start = range.start.min(text.len());
    let mut end = range.end.min(text.len());
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    while end > start && !text.is_char_boundary(end) {
        end -= 1;
    }
    start..end.max(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> LinkEditState {
        LinkEditState::new(7, 3..9, "示例".to_owned(), None, 10.0, 20.0)
    }

    #[test]
    fn url_field_owns_the_ime_by_default_and_tab_switches_fields() {
        let mut edit = state();
        assert_eq!(edit.focused_field, LinkEditField::Url);
        edit.replace_range(0..0, "https://a.example");
        assert_eq!(edit.href_draft, "https://a.example");

        edit.toggle_field();
        assert_eq!(edit.focused_field, LinkEditField::Text);
        assert_eq!(edit.caret_offset, "示例".len());
        edit.replace_range(edit.input_replacement_range(), "站");
        assert_eq!(edit.text_draft, "示例站");
        assert_eq!(edit.label_replacement().as_deref(), Some("示例站"));
    }

    #[test]
    fn composition_marks_and_commits_inside_the_active_field() {
        let mut edit = state();
        edit.replace_and_mark_range(0..0, "nihao", Some(5..5));
        assert_eq!(edit.marked_range, Some(0..5));
        edit.replace_range(edit.input_replacement_range(), "你好");
        assert_eq!(edit.href_draft, "你好");
        assert_eq!(edit.marked_range, None);
        assert_eq!(edit.caret_offset, "你好".len());
    }

    #[test]
    fn normalized_href_trims_and_treats_empty_as_clear() {
        let mut edit = state();
        assert_eq!(edit.normalized_href(), None);
        edit.replace_range(0..0, "  https://example.com  ");
        assert_eq!(
            edit.normalized_href().as_deref(),
            Some("https://example.com")
        );
    }
}
