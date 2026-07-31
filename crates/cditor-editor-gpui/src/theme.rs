// Keep the editor-facing path stable while the theme implementation lives in
// the framework-independent theme crate.
pub use cditor_theme::GuiTheme;

/// Aurin-controlled editor theme stored as a gpui global.
/// When Aurin toggles dark mode, it updates this global,
/// and the editor render function reads it to pick light/dark colors.
#[derive(Debug, Clone)]
pub struct EditorTheme(pub GuiTheme);

impl gpui::Global for EditorTheme {}

impl Default for EditorTheme {
    fn default() -> Self {
        Self(GuiTheme::light())
    }
}

pub fn active_theme(cx: &gpui::App) -> GuiTheme {
    cx.try_global::<EditorTheme>()
        .map(|t| t.0)
        .unwrap_or_else(GuiTheme::light)
}
