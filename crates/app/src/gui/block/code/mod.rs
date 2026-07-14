use crate::gui::GuiTheme;
use crate::gui::app::CditorV2View;
use crate::gui::input::CodeLanguageEditState;
use crate::gui::platform::EDITOR_MONO_FONT_FAMILY;
use cditor_core::ids::BlockId;
use gpui::{
    AnyElement, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement, Styled, div,
    px, rgb,
};

pub(crate) mod highlight;
mod toolbar;

use highlight::code_theme_item;
use toolbar::render_code_toolbar;

pub const V1_CODE_BLOCK_MIN_HEIGHT_PX: f32 = 92.0;
pub const V1_CODE_BLOCK_RADIUS_PX: f32 = 3.0;
pub const V1_CODE_CONTENT_PADDING_TOP_PX: f32 = 34.0;
pub const V1_CODE_CONTENT_PADDING_X_PX: f32 = 14.0;
pub const V1_CODE_CONTENT_PADDING_BOTTOM_PX: f32 = 14.0;

pub fn render_code_block(
    block_id: BlockId,
    content: AnyElement,
    theme: GuiTheme,
    language: Option<&str>,
    language_edit: Option<&CodeLanguageEditState>,
    code_theme_menu_open: bool,
    code_highlight_theme: &'static str,
    action_active: bool,
    view: Entity<CditorV2View>,
    code_language_focus: FocusHandle,
) -> AnyElement {
    let code_theme = code_theme_item(code_highlight_theme);
    div()
        .relative()
        .w_full()
        .min_h(px(V1_CODE_BLOCK_MIN_HEIGHT_PX))
        .rounded(px(V1_CODE_BLOCK_RADIUS_PX))
        .group("notion-code-block")
        .bg(rgb(if action_active {
            blend_rgb(code_theme.background, theme.focused, 0.12)
        } else {
            code_theme.background
        }))
        .font_family(EDITOR_MONO_FONT_FAMILY)
        .child(render_code_content(content, code_theme.foreground))
        .child(render_code_toolbar(
            block_id,
            theme,
            language,
            language_edit,
            code_theme_menu_open,
            code_highlight_theme,
            view,
            code_language_focus,
        ))
        .into_any_element()
}

fn blend_rgb(background: u32, accent: u32, accent_alpha: f32) -> u32 {
    let alpha = accent_alpha.clamp(0.0, 1.0);
    let channel = |shift: u32| {
        let background = ((background >> shift) & 0xff_u32) as f32;
        let accent = ((accent >> shift) & 0xff_u32) as f32;
        (background * (1.0 - alpha) + accent * alpha).round() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

fn render_code_content(content: AnyElement, text_color: u32) -> AnyElement {
    div()
        .w_full()
        .pt(px(V1_CODE_CONTENT_PADDING_TOP_PX))
        .px(px(V1_CODE_CONTENT_PADDING_X_PX))
        .pb(px(V1_CODE_CONTENT_PADDING_BOTTOM_PX))
        .text_color(rgb(text_color))
        .child(content)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_code_block_geometry_constants_match_editor2() {
        assert_eq!(V1_CODE_BLOCK_MIN_HEIGHT_PX, 92.0);
        assert_eq!(V1_CODE_BLOCK_RADIUS_PX, 3.0);
        assert_eq!(V1_CODE_CONTENT_PADDING_TOP_PX, 34.0);
        assert_eq!(V1_CODE_CONTENT_PADDING_X_PX, 14.0);
        assert_eq!(V1_CODE_CONTENT_PADDING_BOTTOM_PX, 14.0);
    }

    #[test]
    fn action_tint_preserves_most_of_the_code_theme_background() {
        assert_eq!(blend_rgb(0x000000, 0xffffff, 0.0), 0x000000);
        assert_eq!(blend_rgb(0x000000, 0xffffff, 1.0), 0xffffff);
        assert_eq!(blend_rgb(0x282a36, 0x2383e2, 0.12), 0x27354b);
    }
}
