pub mod actions;
pub mod ai_prompt;
pub mod clipboard;
pub mod code_language;
pub mod command;
pub mod ime;
pub mod mouse;
pub mod platform_adapter;
pub mod single_line;

pub use actions::bind_cditor_keys;
pub use ai_prompt::{AiPromptEditAction, AiPromptKeyResult, AiPromptState, apply_ai_prompt_action};
pub use code_language::{
    CODE_LANGUAGE_MAX_SUGGESTIONS, CODE_LANGUAGE_VISIBLE_SUGGESTIONS, CodeLanguageEditAction,
    CodeLanguageEditKeyResult, CodeLanguageEditState, CodeLanguageItem, CodeLanguagePopupPlacement,
    apply_code_language_action, is_code_language_text,
};
pub use command::GuiInputCommand;
pub use mouse::{
    BlockDragSelectionController, begin_table_cell_text_selection_from_mouse,
    focus_block_from_mouse, focus_table_cell_from_mouse, gutter_mouse_down_from_mouse,
    hover_block_from_mouse, toggle_block_fold_from_mouse, toggle_todo_from_mouse,
    update_table_cell_text_selection_from_mouse,
};
pub use single_line::{
    SINGLE_LINE_INPUT_FONT_SIZE_PX, SingleLineTextInputElement, single_line_input_max_x,
    single_line_text_offset_for_x, single_line_text_x_for_offset, single_line_visible_range_x,
    single_line_visible_x_for_offset,
};
