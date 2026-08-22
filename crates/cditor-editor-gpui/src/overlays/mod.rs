pub(crate) mod ai_inline;
pub(crate) mod block_transform_menu;
pub(crate) mod callout_menu;
pub(crate) mod color_menu;
pub(crate) mod editor_context_menu;
pub(crate) mod floating_toolbar;
pub(crate) mod link_popup;
pub(crate) mod selection_overlay;
pub(crate) mod slash_menu;
pub(crate) mod table;
pub(crate) mod toast;
#[cfg(feature = "whiteboard")]
pub(crate) mod whiteboard_editor;

use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, prelude::FluentBuilder};

pub(crate) use ai_inline::{render_ai_preview_overlay, render_ai_prompt};
pub(crate) use block_transform_menu::{
    BlockTransformAction, BlockTransformAvailability, block_transform_menu_opens_left,
    block_transform_menu_top_offset, build_block_transform_popup_menu,
};
pub(crate) use color_menu::{
    ActiveColor, ColorMenuAction, PaletteColor, color_menu_geometry, gutter_color_menu_geometry,
};
pub(crate) use editor_context_menu::{render_editor_context_menu, show_editor_context_menu};
pub(crate) use floating_toolbar::{
    FloatingToolbarState, GUTTER_MENU_WIDTH_PX, InlineFormatAction, floating_toolbar_position,
    gutter_floating_toolbar_position, gutter_popup_menu_style, render_floating_toolbar,
    render_gutter_popup_menu, update_gutter_popup_menu,
};
pub(crate) use link_popup::render_link_edit_popup;
use selection_overlay::{
    action_selection_overlay_fragment, render_selection_overlay, selection_overlay_fragments,
};
pub(crate) use slash_menu::{
    SlashMenuCommand, SlashMenuItem, SlashMenuState, slash_query_before_caret,
};
pub(crate) use slash_menu::{
    build_slash_callout_popup_menu, build_slash_popup_menu, render_slash_menu,
    update_slash_popup_menu,
};
pub(crate) use toast::{GuiToast, render_toast, show_toast};
#[cfg(feature = "whiteboard")]
pub(crate) use whiteboard_editor::{WhiteboardEditorSession, render_whiteboard_editor};

use crate::document::DocumentLayoutMetrics;
use crate::theme::GuiTheme;
use cditor_runtime::EditorViewProjection;

pub(crate) fn render_editor_overlays(
    projection: &EditorViewProjection,
    theme: GuiTheme,
    document_layout: DocumentLayoutMetrics,
    action_block_id: Option<cditor_core::ids::BlockId>,
) -> AnyElement {
    let selection = selection_overlay_fragments(projection, document_layout);
    let action_selection =
        action_selection_overlay_fragment(projection, document_layout, action_block_id);
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .child(render_selection_overlay(&selection, theme))
        .when_some(action_selection, |this, fragment| {
            this.child(render_selection_overlay(&[fragment], theme))
        })
        .into_any_element()
}
