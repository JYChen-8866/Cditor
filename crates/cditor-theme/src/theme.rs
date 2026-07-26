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
            surface: 0x202020,
            page: 0x202020,
            panel: 0x292929,
            text: 0xe6e6e6,
            muted: 0x9b9b9b,
            border: 0x3a3a3a,
            strong_border: 0x505050,
            focused: 0x529cca,
            hover_surface: 0x2f2f2f,
            action_background: 0x243d4d,
            action_hover_background: 0x343434,
            action_accent: 0x529cca,
            gutter_background: 0x202020,
            gutter_foreground: 0x9b9b9b,
            prefix_text: 0xe6e6e6,
            quote_text: 0xe6e6e6,
            quote_bar: 0x9b9b9b,
            callout_background: 0x2f2f2f,
            callout_border: 0x3a3a3a,
            callout_icon_background: 0x3a3a3a,
            checkbox_border: 0xc7c7c7,
            checkbox_checked_background: 0x529cca,
            checkbox_checked_text: 0xffffff,
            code_background: 0x272727,
            code_text: 0xe6e6e6,
            inline_code_background: 0x373737,
            inline_code_text: 0xff7369,
            code_toolbar_background: 0x292929,
            code_toolbar_border: 0x3a3a3a,
            code_toolbar_text: 0xb3b3b3,
            code_toolbar_icon: 0x9b9b9b,
            code_toolbar_hover: 0x383838,
            table_header_background: 0x292929,
            table_active_border: 0x529cca,
            skeleton: 0x343434,
            danger: 0xff7369,
            scrollbar_track: 0x2f2f2f,
            scrollbar: 0x555555,
            scrollbar_hover: 0x737373,
        }
    }

    pub const fn color(self, token: ColorToken) -> u32 {
        match token {
            ColorToken::Surface => self.surface,
            ColorToken::Page => self.page,
            ColorToken::Panel => self.panel,
            ColorToken::Text => self.text,
            ColorToken::Muted => self.muted,
            ColorToken::Border => self.border,
            ColorToken::StrongBorder => self.strong_border,
            ColorToken::Focused => self.focused,
            ColorToken::HoverSurface => self.hover_surface,
            ColorToken::ActionBackground => self.action_background,
            ColorToken::ActionHoverBackground => self.action_hover_background,
            ColorToken::ActionAccent => self.action_accent,
            ColorToken::GutterBackground => self.gutter_background,
            ColorToken::GutterForeground => self.gutter_foreground,
            ColorToken::PrefixText => self.prefix_text,
            ColorToken::QuoteText => self.quote_text,
            ColorToken::QuoteBar => self.quote_bar,
            ColorToken::CalloutBackground => self.callout_background,
            ColorToken::CalloutBorder => self.callout_border,
            ColorToken::CalloutIconBackground => self.callout_icon_background,
            ColorToken::CheckboxBorder => self.checkbox_border,
            ColorToken::CheckboxCheckedBackground => self.checkbox_checked_background,
            ColorToken::CheckboxCheckedText => self.checkbox_checked_text,
            ColorToken::CodeBackground => self.code_background,
            ColorToken::CodeText => self.code_text,
            ColorToken::InlineCodeBackground => self.inline_code_background,
            ColorToken::InlineCodeText => self.inline_code_text,
            ColorToken::CodeToolbarBackground => self.code_toolbar_background,
            ColorToken::CodeToolbarBorder => self.code_toolbar_border,
            ColorToken::CodeToolbarText => self.code_toolbar_text,
            ColorToken::CodeToolbarIcon => self.code_toolbar_icon,
            ColorToken::CodeToolbarHover => self.code_toolbar_hover,
            ColorToken::TableHeaderBackground => self.table_header_background,
            ColorToken::TableActiveBorder => self.table_active_border,
            ColorToken::Skeleton => self.skeleton,
            ColorToken::Danger => self.danger,
            ColorToken::ScrollbarTrack => self.scrollbar_track,
            ColorToken::Scrollbar => self.scrollbar,
            ColorToken::ScrollbarHover => self.scrollbar_hover,
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
