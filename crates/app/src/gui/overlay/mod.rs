pub mod caret_overlay;
pub mod selection_overlay;
pub mod slash_menu;
pub mod toast;

use gpui::{AnyElement, IntoElement, ParentElement, Styled, div};

pub use caret_overlay::{CaretOverlayRect, caret_overlay_rects, render_caret_overlay};
pub use selection_overlay::{
    SelectionOverlayFragment, render_selection_overlay, selection_overlay_fragments,
};
pub use slash_menu::{SlashMenuState, render_slash_menu, slash_query_before_caret};
pub use toast::{GuiToast, render_toast};

use crate::gui::GuiTheme;
use cditor_runtime::EditorViewProjection;

pub fn render_editor_overlays(projection: &EditorViewProjection, _theme: GuiTheme) -> AnyElement {
    let selection = selection_overlay_fragments(projection);
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .child(render_selection_overlay(&selection))
        .into_any_element()
}
