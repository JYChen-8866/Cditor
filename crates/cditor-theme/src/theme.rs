use serde::{Deserialize, Serialize};

use crate::ColorToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeId {
    CditorLight,
    CditorDark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThemeVersion(pub u64);

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
    pub scrollbar_track: u32,
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
            scrollbar_track: 0xf1f1ef,
            scrollbar: 0xc7c7c5,
            scrollbar_hover: 0x9b9a97,
        }
    }

    pub const fn dark() -> Self {
        Self {
            surface: 0x060606,
            page: 0x060606,
            panel: 0x0d0d0d,
            text: 0xe5e5e5,
            muted: 0xa1a1a1,
            border: 0x555555,
            strong_border: 0x555555,
            focused: 0x7c86ff,
            hover_surface: 0x1a1a1a,
            action_background: 0x1a1a1a,
            action_hover_background: 0x292929,
            action_accent: 0x7c86ff,
            gutter_background: 0x060606,
            gutter_foreground: 0xa1a1a1,
            prefix_text: 0xe5e5e5,
            quote_text: 0xe5e5e5,
            quote_bar: 0xa1a1a1,
            callout_background: 0x0d0d0d,
            callout_border: 0x1a1a1a,
            callout_icon_background: 0x1e1e1e,
            checkbox_border: 0xe5e5e5,
            checkbox_checked_background: 0x7c86ff,
            checkbox_checked_text: 0x060606,
            code_background: 0x0d0d0d,
            code_text: 0xe5e5e5,
            inline_code_background: 0x1e1e1e,
            inline_code_text: 0xe5e5e5,
            code_toolbar_background: 0x0d0d0d,
            code_toolbar_border: 0x555555,
            code_toolbar_text: 0xa1a1a1,
            code_toolbar_icon: 0xa1a1a1,
            code_toolbar_hover: 0x292929,
            table_header_background: 0x0d0d0d,
            table_active_border: 0x7c86ff,
            skeleton: 0x1e1e1e,
            danger: 0xff6467,
            scrollbar_track: 0x060606,
            scrollbar: 0x292929,
            scrollbar_hover: 0x3a3a3a,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_semantic_token_resolves_in_both_palettes() {
        let tokens = [
            ColorToken::Surface,
            ColorToken::Text,
            ColorToken::Focused,
            ColorToken::Danger,
            ColorToken::CodeBackground,
            ColorToken::TableActiveBorder,
            ColorToken::ScrollbarTrack,
            ColorToken::ScrollbarHover,
        ];
        for token in tokens {
            assert_ne!(
                GuiTheme::light().color(token),
                GuiTheme::dark().color(token)
            );
        }
    }
}
