#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuiTheme {
    pub surface: u32,
    pub page: u32,
    pub panel: u32,
    pub text: u32,
    pub muted: u32,
    pub border: u32,
    pub strong_border: u32,
    pub focused: u32,
    pub hover_surface: u32,
    pub action_background: u32,
    pub action_hover_background: u32,
    pub action_accent: u32,
    pub gutter_background: u32,
    pub gutter_foreground: u32,
    pub prefix_text: u32,
    pub quote_text: u32,
    pub quote_bar: u32,
    pub callout_background: u32,
    pub callout_border: u32,
    pub callout_icon_background: u32,
    pub checkbox_border: u32,
    pub checkbox_checked_background: u32,
    pub checkbox_checked_text: u32,
    pub code_background: u32,
    pub code_text: u32,
    pub inline_code_background: u32,
    pub inline_code_text: u32,
    pub code_toolbar_background: u32,
    pub code_toolbar_border: u32,
    pub code_toolbar_text: u32,
    pub code_toolbar_icon: u32,
    pub code_toolbar_hover: u32,
    pub table_header_background: u32,
    pub table_active_border: u32,
    pub skeleton: u32,
    pub danger: u32,
    pub scrollbar: u32,
    pub scrollbar_hover: u32,
}

impl GuiTheme {
    pub const fn light() -> Self {
        Self {
            surface: 0xffffff,
            page: 0xffffff,
            panel: 0xffffff,
            text: 0x37352f,
            muted: 0x9b9a97,
            border: 0xe9e9e7,
            strong_border: 0xd8d8d6,
            focused: 0x2383e2,
            hover_surface: 0xf1f1ef,
            action_background: 0xe8f2ff,
            action_hover_background: 0xf1f1ef,
            action_accent: 0x2383e2,
            gutter_background: 0xffffff,
            gutter_foreground: 0x9b9a97,
            prefix_text: 0x37352f,
            quote_text: 0x37352f,
            quote_bar: 0x37352f,
            callout_background: 0xf1f1ef,
            callout_border: 0xf1f1ef,
            callout_icon_background: 0xf1f1ef,
            checkbox_border: 0x37352f,
            checkbox_checked_background: 0x2383e2,
            checkbox_checked_text: 0xffffff,
            code_background: 0xf7f6f3,
            code_text: 0x37352f,
            inline_code_background: 0xf1f1ef,
            inline_code_text: 0xeb5757,
            code_toolbar_background: 0xffffff,
            code_toolbar_border: 0xe9e9e7,
            code_toolbar_text: 0x787774,
            code_toolbar_icon: 0x9b9a97,
            code_toolbar_hover: 0xf1f1ef,
            table_header_background: 0xf7f6f4,
            table_active_border: 0x2383e2,
            skeleton: 0xededeb,
            danger: 0xeb5757,
            scrollbar: 0xc7c7c5,
            scrollbar_hover: 0x9b9a97,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_theme_matches_notion_neutral_palette() {
        let theme = GuiTheme::light();

        assert_eq!(theme.text, 0x37352f);
        assert_eq!(theme.muted, 0x9b9a97);
        assert_eq!(theme.border, 0xe9e9e7);
        assert_eq!(theme.hover_surface, 0xf1f1ef);
        assert_eq!(theme.action_background, 0xe8f2ff);
        assert_eq!(theme.action_accent, 0x2383e2);
        assert_eq!(theme.inline_code_text, 0xeb5757);
    }
}
