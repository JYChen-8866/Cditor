pub mod caret_overlay;
pub mod selection_overlay;

use gpui::{AnyElement, IntoElement, ParentElement, Styled, div};

pub use caret_overlay::{CaretOverlayRect, caret_overlay_rects, render_caret_overlay};
pub use selection_overlay::{
    SelectionOverlayFragment, render_selection_overlay, selection_overlay_fragments,
};

use crate::gui::GuiTheme;
use crate::runtime::EditorViewProjection;

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
