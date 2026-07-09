use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::block::code_toolbar::render_code_toolbar;
use crate::gui::input::CodeLanguageEditState;
use cditor_core::ids::BlockId;
use gpui::{AnyElement, Entity, FocusHandle, IntoElement, ParentElement, Styled, div, px, rgb};

pub const V1_CODE_BLOCK_MIN_HEIGHT_PX: f32 = 92.0;
pub const V1_CODE_BLOCK_RADIUS_PX: f32 = 8.0;
pub const V1_CODE_CONTENT_PADDING_TOP_PX: f32 = 34.0;
pub const V1_CODE_CONTENT_PADDING_X_PX: f32 = 14.0;
pub const V1_CODE_CONTENT_PADDING_BOTTOM_PX: f32 = 14.0;

pub fn render_code_block(
    block_id: BlockId,
    content: AnyElement,
    theme: GuiTheme,
    language: Option<&str>,
    language_edit: Option<&CodeLanguageEditState>,
    action_active: bool,
    view: Entity<CditorV2View>,
    code_language_focus: FocusHandle,
) -> AnyElement {
    div()
        .relative()
        .w_full()
        .min_h(px(V1_CODE_BLOCK_MIN_HEIGHT_PX))
        .rounded(px(V1_CODE_BLOCK_RADIUS_PX))
        .bg(rgb(if action_active {
            theme.action_background
        } else {
            theme.code_background
        }))
        .font_family("Menlo")
        .child(render_code_content(content, theme))
        .child(render_code_toolbar(
            block_id,
            theme,
            language,
            language_edit,
            view,
            code_language_focus,
        ))
        .into_any_element()
}

fn render_code_content(content: AnyElement, theme: GuiTheme) -> AnyElement {
    div()
        .w_full()
        .pt(px(V1_CODE_CONTENT_PADDING_TOP_PX))
        .px(px(V1_CODE_CONTENT_PADDING_X_PX))
        .pb(px(V1_CODE_CONTENT_PADDING_BOTTOM_PX))
        .text_color(rgb(theme.code_text))
        .child(content)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_code_block_geometry_constants_match_editor2() {
        assert_eq!(V1_CODE_BLOCK_MIN_HEIGHT_PX, 92.0);
        assert_eq!(V1_CODE_BLOCK_RADIUS_PX, 8.0);
        assert_eq!(V1_CODE_CONTENT_PADDING_TOP_PX, 34.0);
        assert_eq!(V1_CODE_CONTENT_PADDING_X_PX, 14.0);
        assert_eq!(V1_CODE_CONTENT_PADDING_BOTTOM_PX, 14.0);
    }
}
