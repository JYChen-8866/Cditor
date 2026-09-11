// Keep the editor-facing path stable while the theme implementation lives in
// the framework-independent theme crate.
pub use cditor_theme::GuiTheme;

const LIGHT_SELECTION_BACKGROUND: u32 = 0xdbeafe;
const DARK_SELECTION_BACKGROUND: u32 = 0x26304a;

/// Selection uses the prototype's `accent-soft` light blue in light themes and
/// a comparably subdued blue-black in dark themes.
pub(crate) fn selection_background_color(theme: GuiTheme) -> u32 {
    let red = (theme.surface >> 16) & 0xff;
    let green = (theme.surface >> 8) & 0xff;
    let blue = theme.surface & 0xff;
    let luminance = (red * 299 + green * 587 + blue * 114) / 1000;
    if luminance < 128 {
        DARK_SELECTION_BACKGROUND
    } else {
        LIGHT_SELECTION_BACKGROUND
    }
}

/// Aurin-controlled editor theme stored as a gpui global.
/// When Aurin toggles dark mode, it updates this global,
/// and the editor render function reads it to pick light/dark colors.
#[derive(Debug, Clone)]
pub struct EditorTheme {
    pub theme: GuiTheme,
    pub is_dark: bool,
}

impl gpui::Global for EditorTheme {}

impl Default for EditorTheme {
    fn default() -> Self {
        Self {
            theme: GuiTheme::light(),
            is_dark: false,
        }
    }
}

pub fn active_theme(cx: &gpui::App) -> GuiTheme {
    cx.try_global::<EditorTheme>()
        .map(|t| t.theme)
        .unwrap_or_else(GuiTheme::light)
}

pub fn is_dark_mode(cx: &gpui::App) -> bool {
    cx.try_global::<EditorTheme>()
        .map(|t| t.is_dark)
        .unwrap_or(false)
}
