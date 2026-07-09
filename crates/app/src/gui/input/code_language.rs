use cditor_core::ids::BlockId;
use gpui::KeyDownEvent;
use std::ops::Range;

pub const CODE_LANGUAGE_MAX_SUGGESTIONS: usize = 64;
pub const CODE_LANGUAGE_VISIBLE_SUGGESTIONS: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeLanguagePopupPlacement {
    Below,
    Above,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeLanguageItem {
    pub value: String,
    pub label: String,
}

impl CodeLanguageItem {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            label: value.clone(),
            value,
        }
    }

    pub fn labeled(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeLanguageEditState {
    pub block_id: BlockId,
    pub original: String,
    pub draft: String,
    pub is_open: bool,
    pub selected_index: usize,
    pub scroll_start: usize,
    pub placement: CodeLanguagePopupPlacement,
    pub caret_offset: usize,
    pub marked_range: Option<Range<usize>>,
}

impl CodeLanguageEditState {
    pub fn new(block_id: BlockId, language: Option<&str>) -> Self {
        Self::new_with_placement(block_id, language, CodeLanguagePopupPlacement::Below)
    }

    pub fn new_with_placement(
        block_id: BlockId,
        language: Option<&str>,
        placement: CodeLanguagePopupPlacement,
    ) -> Self {
        let original = language.unwrap_or_default().to_owned();
        Self {
            block_id,
            original: original.clone(),
            draft: original.clone(),
            is_open: true,
            selected_index: 0,
            scroll_start: 0,
            placement,
            caret_offset: original.len(),
            marked_range: None,
        }
    }

    pub fn normalized_draft(&self) -> Option<String> {
        normalize_code_language(&self.draft)
    }

    pub fn matching_items(&self) -> Vec<CodeLanguageItem> {
        matching_code_language_items(&self.draft, CODE_LANGUAGE_MAX_SUGGESTIONS)
    }

    pub fn selected_item(&self) -> Option<CodeLanguageItem> {
        self.matching_items().get(self.selected_index).cloned()
    }

    fn reset_selection(&mut self) {
        self.selected_index = 0;
        self.scroll_start = 0;
        self.is_open = true;
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.matching_items().len();
        if len == 0 {
            self.selected_index = 0;
            self.scroll_start = 0;
            return;
        }
        let current = self.selected_index.min(len - 1) as isize;
        self.selected_index = (current + delta).rem_euclid(len as isize) as usize;
        self.keep_selected_item_visible(len);
        self.is_open = true;
    }

    pub fn scroll_suggestions(&mut self, delta_rows: isize) -> bool {
        let len = self.matching_items().len();
        if len <= CODE_LANGUAGE_VISIBLE_SUGGESTIONS || delta_rows == 0 {
            return false;
        }
        let max_start = len.saturating_sub(CODE_LANGUAGE_VISIBLE_SUGGESTIONS);
        let next_scroll_start =
            (self.scroll_start as isize + delta_rows).clamp(0, max_start as isize) as usize;
        if next_scroll_start == self.scroll_start {
            return false;
        }
        self.scroll_start = next_scroll_start;
        if self.selected_index < self.scroll_start {
            self.selected_index = self.scroll_start;
        } else if self.selected_index >= self.scroll_start + CODE_LANGUAGE_VISIBLE_SUGGESTIONS {
            self.selected_index = self.scroll_start + CODE_LANGUAGE_VISIBLE_SUGGESTIONS - 1;
        }
        self.is_open = true;
        true
    }

    fn keep_selected_item_visible(&mut self, len: usize) {
        if len <= CODE_LANGUAGE_VISIBLE_SUGGESTIONS {
            self.scroll_start = 0;
            return;
        }
        if self.selected_index < self.scroll_start {
            self.scroll_start = self.selected_index;
        } else if self.selected_index >= self.scroll_start + CODE_LANGUAGE_VISIBLE_SUGGESTIONS {
            self.scroll_start = self.selected_index + 1 - CODE_LANGUAGE_VISIBLE_SUGGESTIONS;
        }
        self.scroll_start = self
            .scroll_start
            .min(len.saturating_sub(CODE_LANGUAGE_VISIBLE_SUGGESTIONS));
    }

    fn move_caret_to(&mut self, offset: usize) {
        self.caret_offset = clamp_to_char_boundary(&self.draft, offset);
        self.marked_range = None;
    }

    fn move_caret_left(&mut self) {
        if let Some(previous) = previous_char_boundary(&self.draft, self.caret_offset) {
            self.move_caret_to(previous);
        }
    }

    fn move_caret_right(&mut self) {
        self.move_caret_to(next_char_boundary(&self.draft, self.caret_offset));
    }
}

impl CodeLanguageEditState {
    pub fn replace_range(&mut self, range: Range<usize>, text: &str) {
        let range = safe_range(&self.draft, range);
        self.draft.replace_range(range.clone(), text);
        self.caret_offset = range.start + text.len();
        self.marked_range = None;
        self.reset_selection();
    }

    pub fn replace_and_mark_range(
        &mut self,
        range: Range<usize>,
        text: &str,
        selected_range: Option<Range<usize>>,
    ) {
        let range = safe_range(&self.draft, range);
        self.draft.replace_range(range.clone(), text);
        self.marked_range = Some(range.start..range.start + text.len());
        self.caret_offset = selected_range
            .map(|selection| range.start + selection.end.min(text.len()))
            .unwrap_or(range.start + text.len());
        self.reset_selection();
    }

    pub fn unmark(&mut self) {
        self.marked_range = None;
    }

    pub fn input_replacement_range(&self) -> Range<usize> {
        self.marked_range
            .clone()
            .unwrap_or(self.caret_offset..self.caret_offset)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeLanguageEditKeyResult {
    Commit,
    Cancel,
    Changed,
    Ignored,
}

pub fn apply_code_language_key(
    state: &mut CodeLanguageEditState,
    event: &KeyDownEvent,
) -> CodeLanguageEditKeyResult {
    if event.keystroke.is_ime_in_progress() {
        return CodeLanguageEditKeyResult::Ignored;
    }
    let modifiers = event.keystroke.modifiers;
    if modifiers.platform || modifiers.control || modifiers.alt {
        return CodeLanguageEditKeyResult::Ignored;
    }
    match event.keystroke.key.as_str() {
        "enter" | "tab" => {
            if let Some(item) = state.selected_item() {
                state.draft = item.value;
            }
            CodeLanguageEditKeyResult::Commit
        }
        "escape" => CodeLanguageEditKeyResult::Cancel,
        "up" => {
            state.move_selection(-1);
            CodeLanguageEditKeyResult::Changed
        }
        "down" => {
            state.move_selection(1);
            CodeLanguageEditKeyResult::Changed
        }
        "left" => {
            state.move_caret_left();
            CodeLanguageEditKeyResult::Changed
        }
        "right" => {
            state.move_caret_right();
            CodeLanguageEditKeyResult::Changed
        }
        "home" => {
            state.move_caret_to(0);
            CodeLanguageEditKeyResult::Changed
        }
        "end" => {
            state.move_caret_to(state.draft.len());
            CodeLanguageEditKeyResult::Changed
        }
        "backspace" => {
            if let Some(previous) = previous_char_boundary(&state.draft, state.caret_offset) {
                state.draft.replace_range(previous..state.caret_offset, "");
                state.caret_offset = previous;
                state.marked_range = None;
                state.reset_selection();
            }
            CodeLanguageEditKeyResult::Changed
        }
        "delete" => {
            let next = next_char_boundary(&state.draft, state.caret_offset);
            if next > state.caret_offset {
                state.draft.replace_range(state.caret_offset..next, "");
                state.marked_range = None;
                state.reset_selection();
            }
            CodeLanguageEditKeyResult::Changed
        }
        _ => CodeLanguageEditKeyResult::Ignored,
    }
}

fn safe_range(text: &str, range: Range<usize>) -> Range<usize> {
    let start = clamp_to_char_boundary(text, range.start.min(text.len()));
    let end = clamp_to_char_boundary(text, range.end.min(text.len())).max(start);
    start..end
}

fn clamp_to_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn previous_char_boundary(text: &str, offset: usize) -> Option<usize> {
    let offset = clamp_to_char_boundary(text, offset);
    text[..offset]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
}

fn next_char_boundary(text: &str, offset: usize) -> usize {
    let offset = clamp_to_char_boundary(text, offset);
    text[offset..]
        .char_indices()
        .nth(1)
        .map(|(index, _)| offset + index)
        .unwrap_or(text.len())
}

pub fn normalize_code_language(language: &str) -> Option<String> {
    let normalized = language.trim().to_lowercase();
    (!normalized.is_empty() && normalized != "plain text").then_some(normalized)
}

pub fn code_language_items() -> Vec<CodeLanguageItem> {
    [
        CodeLanguageItem::labeled("plain text", "Plain Text"),
        CodeLanguageItem::labeled("rust", "Rust"),
        CodeLanguageItem::labeled("typescript", "TypeScript"),
        CodeLanguageItem::labeled("javascript", "JavaScript"),
        CodeLanguageItem::labeled("tsx", "TSX"),
        CodeLanguageItem::labeled("jsx", "JSX"),
        CodeLanguageItem::labeled("python", "Python"),
        CodeLanguageItem::labeled("go", "Go"),
        CodeLanguageItem::labeled("java", "Java"),
        CodeLanguageItem::labeled("kotlin", "Kotlin"),
        CodeLanguageItem::labeled("swift", "Swift"),
        CodeLanguageItem::labeled("c", "C"),
        CodeLanguageItem::labeled("cpp", "C++"),
        CodeLanguageItem::labeled("csharp", "C#"),
        CodeLanguageItem::labeled("html", "HTML"),
        CodeLanguageItem::labeled("css", "CSS"),
        CodeLanguageItem::labeled("scss", "SCSS"),
        CodeLanguageItem::labeled("json", "JSON"),
        CodeLanguageItem::labeled("yaml", "YAML"),
        CodeLanguageItem::labeled("toml", "TOML"),
        CodeLanguageItem::labeled("markdown", "Markdown"),
        CodeLanguageItem::labeled("sql", "SQL"),
        CodeLanguageItem::labeled("shell", "Shell"),
        CodeLanguageItem::labeled("bash", "Bash"),
        CodeLanguageItem::labeled("zsh", "Zsh"),
        CodeLanguageItem::labeled("dockerfile", "Dockerfile"),
        CodeLanguageItem::labeled("diff", "Diff"),
    ]
    .into_iter()
    .collect()
}

pub fn matching_code_language_items(query: &str, max: usize) -> Vec<CodeLanguageItem> {
    let query = query.trim().to_lowercase();
    code_language_items()
        .into_iter()
        .filter(|item| {
            query.is_empty()
                || item.value.to_lowercase().contains(&query)
                || item.label.to_lowercase().contains(&query)
        })
        .take(max.max(1))
        .collect()
}

pub fn is_code_language_text(text: &str) -> bool {
    text.chars().all(is_code_language_char)
}

fn is_code_language_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '+' | '#' | '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event_for_key(key: &str) -> KeyDownEvent {
        KeyDownEvent {
            keystroke: gpui::Keystroke {
                modifiers: gpui::Modifiers::default(),
                key: key.into(),
                key_char: Some(key.to_owned()),
            },
            is_held: false,
            prefer_character_input: false,
        }
    }

    #[test]
    fn code_language_edit_replaces_range_with_language_name_text() {
        let mut state = CodeLanguageEditState::new(1, Some("rs"));

        state.replace_range(state.input_replacement_range(), "-");
        state.replace_range(state.input_replacement_range(), "x");
        assert_eq!(state.draft, "rs-x");
        assert_eq!(state.normalized_draft().as_deref(), Some("rs-x"));
    }

    #[test]
    fn code_language_edit_filters_and_moves_selection() {
        let mut state = CodeLanguageEditState::new(1, None);
        state.draft = "type".to_owned();

        let matches = state.matching_items();
        assert!(matches.iter().any(|item| item.value == "typescript"));

        state.draft.clear();
        assert_eq!(
            apply_code_language_key(&mut state, &event_for_key("down")),
            CodeLanguageEditKeyResult::Changed
        );
        assert_eq!(state.selected_index, 1);
    }

    #[test]
    fn code_language_edit_scrolls_selected_item_into_visible_window() {
        let mut state = CodeLanguageEditState::new(1, None);

        for _ in 0..CODE_LANGUAGE_VISIBLE_SUGGESTIONS {
            assert_eq!(
                apply_code_language_key(&mut state, &event_for_key("down")),
                CodeLanguageEditKeyResult::Changed
            );
        }

        assert_eq!(state.selected_index, CODE_LANGUAGE_VISIBLE_SUGGESTIONS);
        assert_eq!(state.scroll_start, 1);

        assert_eq!(
            apply_code_language_key(&mut state, &event_for_key("up")),
            CodeLanguageEditKeyResult::Changed
        );
        assert_eq!(state.selected_index, CODE_LANGUAGE_VISIBLE_SUGGESTIONS - 1);
        assert_eq!(state.scroll_start, 1);
    }

    #[test]
    fn code_language_edit_scrolls_suggestions_from_mouse_wheel() {
        let mut state = CodeLanguageEditState::new(1, None);

        assert!(state.scroll_suggestions(2));
        assert_eq!(state.scroll_start, 2);
        assert_eq!(state.selected_index, 2);

        assert!(state.scroll_suggestions(-1));
        assert_eq!(state.scroll_start, 1);
        assert_eq!(state.selected_index, 2);
    }

    #[test]
    fn code_language_edit_enter_commits_and_escape_cancels() {
        let mut state = CodeLanguageEditState::new(1, None);

        assert_eq!(
            apply_code_language_key(&mut state, &event_for_key("enter")),
            CodeLanguageEditKeyResult::Commit
        );
        assert_eq!(
            apply_code_language_key(&mut state, &event_for_key("escape")),
            CodeLanguageEditKeyResult::Cancel
        );
    }

    #[test]
    fn code_language_edit_tracks_caret_and_marked_text_for_ime() {
        let mut state = CodeLanguageEditState::new(1, Some("ru"));

        state.replace_and_mark_range(state.input_replacement_range(), "日", Some(3..3));
        assert_eq!(state.draft, "ru日");
        assert_eq!(state.marked_range, Some(2..5));
        assert_eq!(state.caret_offset, 5);

        state.unmark();
        assert_eq!(state.marked_range, None);

        assert_eq!(
            apply_code_language_key(&mut state, &event_for_key("left")),
            CodeLanguageEditKeyResult::Changed
        );
        assert_eq!(state.caret_offset, 2);

        assert_eq!(
            apply_code_language_key(&mut state, &event_for_key("delete")),
            CodeLanguageEditKeyResult::Changed
        );
        assert_eq!(state.draft, "ru");
    }

    #[test]
    fn code_language_edit_clamps_replacement_ranges_to_char_boundaries() {
        let mut state = CodeLanguageEditState::new(1, Some("r日😀"));

        state.replace_range(2..7, "x");

        assert_eq!(state.draft, "rx😀");
        assert_eq!(state.caret_offset, "rx".len());

        state.replace_and_mark_range(1..3, "한", Some("한".len().."한".len()));

        assert_eq!(state.draft, "r한😀");
        assert_eq!(state.marked_range, Some(1.."r한".len()));
        assert_eq!(state.caret_offset, "r한".len());
    }
}
