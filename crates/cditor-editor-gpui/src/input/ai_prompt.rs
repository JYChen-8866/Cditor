use std::ops::Range;

use cditor_core::ids::BlockId;
use cditor_runtime::AiRequestPresentation;
use gpui::Pixels;

#[derive(Debug, Clone, PartialEq)]
pub struct AiPromptState {
    pub block_id: BlockId,
    pub draft: String,
    pub caret_offset: usize,
    pub marked_range: Option<Range<usize>>,
    pub presentation: AiRequestPresentation,
    pub x: Pixels,
    pub y: Pixels,
}

impl AiPromptState {
    pub fn new(block_id: BlockId, x: Pixels, y: Pixels) -> Self {
        Self::with_presentation(block_id, x, y, AiRequestPresentation::Automatic)
    }

    pub fn with_presentation(
        block_id: BlockId,
        x: Pixels,
        y: Pixels,
        presentation: AiRequestPresentation,
    ) -> Self {
        Self {
            block_id,
            draft: String::new(),
            caret_offset: 0,
            marked_range: None,
            presentation,
            x,
            y,
        }
    }

    pub fn replace_range(&mut self, range: Range<usize>, text: &str) {
        let range = safe_range(&self.draft, range);
        self.draft.replace_range(range.clone(), text);
        self.caret_offset = range.start + text.len();
        self.marked_range = None;
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
    }

    pub fn input_replacement_range(&self) -> Range<usize> {
        self.marked_range
            .clone()
            .unwrap_or(self.caret_offset..self.caret_offset)
    }

    pub fn unmark(&mut self) {
        self.marked_range = None;
    }

    fn move_caret_to(&mut self, offset: usize) {
        self.caret_offset = clamp_to_char_boundary(&self.draft, offset);
        self.marked_range = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiPromptKeyResult {
    Submit,
    Cancel,
    Changed,
    Ignored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiPromptEditAction {
    Submit,
    Cancel,
    MoveLeft,
    MoveRight,
    MoveToStart,
    MoveToEnd,
    DeleteBackward,
    DeleteForward,
}

pub fn apply_ai_prompt_action(
    state: &mut AiPromptState,
    action: AiPromptEditAction,
) -> AiPromptKeyResult {
    match action {
        AiPromptEditAction::Submit => AiPromptKeyResult::Submit,
        AiPromptEditAction::Cancel => AiPromptKeyResult::Cancel,
        AiPromptEditAction::MoveLeft => {
            if let Some(previous) = previous_char_boundary(&state.draft, state.caret_offset) {
                state.move_caret_to(previous);
            }
            AiPromptKeyResult::Changed
        }
        AiPromptEditAction::MoveRight => {
            state.move_caret_to(next_char_boundary(&state.draft, state.caret_offset));
            AiPromptKeyResult::Changed
        }
        AiPromptEditAction::MoveToStart => {
            state.move_caret_to(0);
            AiPromptKeyResult::Changed
        }
        AiPromptEditAction::MoveToEnd => {
            state.move_caret_to(state.draft.len());
            AiPromptKeyResult::Changed
        }
        AiPromptEditAction::DeleteBackward => {
            if let Some(previous) = previous_char_boundary(&state.draft, state.caret_offset) {
                state.draft.replace_range(previous..state.caret_offset, "");
                state.caret_offset = previous;
                state.marked_range = None;
            }
            AiPromptKeyResult::Changed
        }
        AiPromptEditAction::DeleteForward => {
            let next = next_char_boundary(&state.draft, state.caret_offset);
            if next > state.caret_offset {
                state.draft.replace_range(state.caret_offset..next, "");
                state.marked_range = None;
            }
            AiPromptKeyResult::Changed
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_edits_multibyte_text_on_char_boundaries() {
        let mut state = AiPromptState::new(1, gpui::px(0.0), gpui::px(0.0));
        state.replace_range(0..0, "中文");
        assert_eq!(state.caret_offset, 6);
        assert_eq!(
            apply_ai_prompt_action(&mut state, AiPromptEditAction::DeleteBackward),
            AiPromptKeyResult::Changed
        );
        assert_eq!(state.draft, "中");
        assert_eq!(state.caret_offset, 3);
    }

    #[test]
    fn prompt_preserves_assistant_panel_presentation() {
        let state = AiPromptState::with_presentation(
            1,
            gpui::px(0.0),
            gpui::px(0.0),
            AiRequestPresentation::AssistantPanel,
        );
        assert_eq!(state.presentation, AiRequestPresentation::AssistantPanel);
    }

    #[test]
    fn prompt_actions_do_not_depend_on_native_key_names() {
        let mut state = AiPromptState::new(1, gpui::px(0.0), gpui::px(0.0));
        state.replace_range(0..0, "ab");
        assert_eq!(
            apply_ai_prompt_action(&mut state, AiPromptEditAction::DeleteBackward),
            AiPromptKeyResult::Changed
        );
        assert_eq!(state.draft, "a");
        assert_eq!(
            apply_ai_prompt_action(&mut state, AiPromptEditAction::Submit),
            AiPromptKeyResult::Submit
        );
    }
}
