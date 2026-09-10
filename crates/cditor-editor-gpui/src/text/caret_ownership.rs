#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextCursorOwnership {
    Custom,
    Native,
    Hidden,
}

/// Zed's editor and ui_input both use a 2px bar cursor. Keep every editable
/// surface on the same width so caret ownership never changes its appearance.
pub(crate) const CUSTOM_CARET_WIDTH_PX: f32 = 2.0;

#[cfg(feature = "mobile-text-session")]
pub(crate) fn platform_text_cursor_ownership(window: &gpui::Window) -> TextCursorOwnership {
    match window.text_cursor_ownership() {
        gpui::PlatformTextCursorOwnership::Custom => TextCursorOwnership::Custom,
        gpui::PlatformTextCursorOwnership::Native => TextCursorOwnership::Native,
        gpui::PlatformTextCursorOwnership::Hidden => TextCursorOwnership::Hidden,
    }
}

/// Desktop 上 GPUI 的 custom text element 不保证存在平台 caret，因此由
/// Cditor 独占绘制。mobile-text-session 再把所有权交还给平台协议。
#[cfg(not(feature = "mobile-text-session"))]
pub(crate) fn platform_text_cursor_ownership(_window: &gpui::Window) -> TextCursorOwnership {
    TextCursorOwnership::Custom
}

pub(crate) fn should_paint_custom_caret(
    focused: bool,
    blink_visible: bool,
    _has_marked_text: bool,
    ownership: TextCursorOwnership,
) -> bool {
    focused && blink_visible && ownership == TextCursorOwnership::Custom
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_caret_requires_exclusive_platform_ownership() {
        assert!(should_paint_custom_caret(
            true,
            true,
            false,
            TextCursorOwnership::Custom,
        ));
        assert!(!should_paint_custom_caret(
            true,
            true,
            false,
            TextCursorOwnership::Native,
        ));
        assert!(!should_paint_custom_caret(
            true,
            true,
            false,
            TextCursorOwnership::Hidden,
        ));
    }

    #[test]
    fn custom_caret_stays_visible_during_marked_text_but_requires_focus_and_blink() {
        assert!(!should_paint_custom_caret(
            false,
            true,
            false,
            TextCursorOwnership::Custom,
        ));
        assert!(!should_paint_custom_caret(
            true,
            false,
            false,
            TextCursorOwnership::Custom,
        ));
        assert!(should_paint_custom_caret(
            true,
            true,
            true,
            TextCursorOwnership::Custom,
        ));
    }
}
