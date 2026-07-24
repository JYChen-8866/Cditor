pub(crate) mod ai_inline;
pub(crate) mod block_transform_menu;
pub(crate) mod color_menu;
pub(crate) mod floating_toolbar;
pub(crate) mod selection_overlay;
pub(crate) mod slash_menu;
pub(crate) mod table;
pub(crate) mod toast;
pub(crate) mod whiteboard_editor;

use gpui::{AnyElement, IntoElement, ParentElement, Styled, div};

pub(crate) use ai_inline::{render_ai_preview_overlay, render_ai_prompt};
pub(crate) use block_transform_menu::{
    BlockTransformAction, BlockTransformAvailability, block_transform_menu_opens_left,
    block_transform_menu_top_offset,
};
pub(crate) use color_menu::{ActiveColor, ColorMenuAction, PaletteColor, color_menu_geometry};
pub(crate) use floating_toolbar::{
    FloatingToolbarState, InlineFormatAction, floating_toolbar_position,
    gutter_floating_toolbar_position, render_floating_toolbar,
};
use selection_overlay::{render_selection_overlay, selection_overlay_fragments};
pub(crate) use slash_menu::render_slash_menu;
pub(crate) use slash_menu::{
    SlashMenuCommand, SlashMenuItem, SlashMenuState, slash_query_before_caret,
};
pub(crate) use toast::{GuiToast, render_toast, show_toast};
pub(crate) use whiteboard_editor::{WhiteboardEditorSession, render_whiteboard_editor};

use crate::theme::GuiTheme;
use cditor_runtime::EditorViewProjection;

pub(crate) fn render_editor_overlays(
    projection: &EditorViewProjection,
    theme: GuiTheme,
) -> AnyElement {
    let selection = selection_overlay_fragments(projection);
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .child(render_selection_overlay(&selection, theme))
        .into_any_element()
}
