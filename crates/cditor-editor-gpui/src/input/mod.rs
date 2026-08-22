pub mod actions;
pub mod ai_prompt;
pub mod clipboard;
pub mod code_language;
pub mod command;
pub mod ime;
pub(crate) mod keyboard;
pub mod link_edit;
pub mod mouse;
#[cfg(feature = "mobile-text-session")]
mod native_actions;
pub mod platform_adapter;
pub(crate) mod routing;
pub mod single_line;
mod table_cell_navigation;
pub(crate) mod trace;

pub use actions::{bind_cditor_keys, cditor_key_bindings};
pub use ai_prompt::{AiPromptEditAction, AiPromptKeyResult, AiPromptState, apply_ai_prompt_action};
#[allow(
    unused_imports,
    reason = "kept as part of the input module's public language-item API"
)]
pub use code_language::{
    CODE_LANGUAGE_VISIBLE_SUGGESTIONS, CodeLanguageEditAction, CodeLanguageEditKeyResult,
    CodeLanguageEditState, CodeLanguageItem, CodeLanguagePopupPlacement,
    apply_code_language_action,
};
pub use command::GuiInputCommand;
pub use mouse::{
    BlockDragSelectionController, begin_table_cell_text_selection_from_mouse,
    focus_block_from_mouse, gutter_mouse_down_from_mouse, hover_block_from_mouse,
    open_link_from_mouse, toggle_block_fold_from_mouse, toggle_todo_from_mouse,
    update_table_cell_text_selection_from_mouse,
};
pub use single_line::{
    SINGLE_LINE_INPUT_FONT_SIZE_PX, SingleLineTextInputElement, single_line_text_offset_for_x,
    single_line_visible_range_x,
};
