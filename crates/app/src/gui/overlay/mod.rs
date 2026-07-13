pub mod ai_inline;
pub mod block_transform_menu;
pub mod caret_overlay;
pub mod color_menu;
pub mod floating_toolbar;
pub mod selection_overlay;
pub mod slash_menu;
pub mod table;
pub mod toast;
pub mod whiteboard_editor;

use gpui::{AnyElement, IntoElement, ParentElement, Styled, div};

pub(crate) use ai_inline::{render_ai_preview_overlay, render_ai_prompt};
pub use block_transform_menu::{
    BlockTransformAction, BlockTransformAvailability, block_transform_menu_opens_left,
    block_transform_menu_top_offset, render_block_transform_menu,
};
pub use caret_overlay::{CaretOverlayRect, caret_overlay_rects, render_caret_overlay};
pub use color_menu::{
    ActiveColor, ColorMenuAction, PaletteColor, color_menu_geometry, render_color_menu,
};
pub use floating_toolbar::{
    FloatingToolbarState, InlineFormatAction, floating_toolbar_position,
    left_aligned_floating_toolbar_position, render_floating_toolbar,
};
pub use selection_overlay::{
    SelectionOverlayFragment, render_selection_overlay, selection_overlay_fragments,
};
pub use slash_menu::{
    SlashMenuCommand, SlashMenuItem, SlashMenuState, render_slash_menu, slash_query_before_caret,
};
pub use toast::{GuiToast, render_toast};
pub(crate) use whiteboard_editor::{WhiteboardEditorSession, render_whiteboard_editor};

use crate::gui::GuiTheme;
use cditor_runtime::EditorViewProjection;

pub fn render_editor_overlays(projection: &EditorViewProjection, theme: GuiTheme) -> AnyElement {
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
