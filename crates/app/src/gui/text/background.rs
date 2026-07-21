use crate::gui::GuiTheme;

pub(super) fn text_selection_background(theme: GuiTheme) -> u32 {
    (theme.focused << 8) | 0x26
}
