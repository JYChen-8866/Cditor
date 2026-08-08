#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextCursorOwnership {
    Custom,
    Native,
    Hidden,
}

#[cfg(feature = "mobile-text-session")]
pub(crate) fn platform_text_cursor_ownership(window: &gpui::Window) -> TextCursorOwnership {
    match window.text_cursor_ownership() {
        gpui::PlatformTextCursorOwnership::Custom => TextCursorOwnership::Custom,
        gpui::PlatformTextCursorOwnership::Native => TextCursorOwnership::Native,
        gpui::PlatformTextCursorOwnership::Hidden => TextCursorOwnership::Hidden,
    }
}

#[cfg(not(feature = "mobile-text-session"))]
pub(crate) fn platform_text_cursor_ownership(_window: &gpui::Window) -> TextCursorOwnership {
    TextCursorOwnership::Custom
}

pub(crate) fn should_paint_custom_caret(
    focused: bool,
    blink_visible: bool,
    has_marked_text: bool,
    ownership: TextCursorOwnership,
) -> bool {
    focused && blink_visible && !has_marked_text && ownership == TextCursorOwnership::Custom
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
    fn marked_text_focus_and_blink_still_gate_custom_caret() {
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
        assert!(!should_paint_custom_caret(
            true,
            true,
            true,
            TextCursorOwnership::Custom,
        ));
    }
}
