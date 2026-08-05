//! Theme tokens shared between the Drafft whiteboard surface and the host app.
//!
//! The board reads this global during render, and the host (Cditor editor
//! layer) refreshes it whenever the active editor theme changes, so the
//! whiteboard canvas and chrome follow the surrounding application.

/// A small set of 24-bit RGB colors consumed by the Drafft board UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhiteboardTheme {
    /// Canvas background color.
    pub page: u32,
    /// Primary UI text color.
    pub text: u32,
    /// Muted / secondary text color.
    pub muted: u32,
    /// Border color for panels and controls.
    pub border: u32,
    /// Panel / toolbar background color.
    pub panel: u32,
    /// Hover surface color.
    pub hover: u32,
    /// Accent color (selection, active tool).
    pub accent: u32,
    /// Foreground painted on accent controls.
    pub on_accent: u32,
    /// Ink color used for shape strokes and text on the canvas.
    pub ink: u32,
    /// Background grid color.
    pub grid: u32,
    /// Destructive / danger color.
    pub danger: u32,
}

impl Default for WhiteboardTheme {
    fn default() -> Self {
        Self {
            page: 0xffffff,
            text: 0x37352f,
            muted: 0x9b9a97,
            border: 0xe9e9e7,
            panel: 0xffffff,
            hover: 0xf1f1ef,
            accent: 0x2383e2,
            on_accent: 0xffffff,
            ink: 0x37352f,
            grid: 0xc8c8c8,
            danger: 0xeb5757,
        }
    }
}

impl WhiteboardTheme {
    /// Returns the theme registered in the app context, or the light default
    /// when the host has not installed one yet.
    pub fn get(cx: &gpui::App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_default()
    }

    /// Installs the theme so every Drafft board renders with it.
    pub fn set(cx: &mut gpui::App, theme: Self) {
        cx.set_global(theme);
    }
}

impl gpui::Global for WhiteboardTheme {}

/// Resolved chrome colors for the whiteboard toolbars and panels, derived from
/// the active [`WhiteboardTheme`] so the floating UI follows the app theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhiteboardChrome {
    /// Toolbar / panel background.
    pub bg: u32,
    /// Raised surface (popovers, active tabs).
    pub bg_strong: u32,
    /// Primary UI text.
    pub text: u32,
    /// Muted / secondary text.
    pub text_muted: u32,
    /// Hairline border.
    pub border: u32,
    /// Stronger border.
    pub border_strong: u32,
    /// Hover surface.
    pub hover: u32,
    /// Active / selected wash.
    pub active: u32,
    /// Accent color.
    pub accent: u32,
    /// Text color placed on the accent fill.
    pub on_accent: u32,
    /// Destructive action color.
    pub danger: u32,
}

impl From<WhiteboardTheme> for WhiteboardChrome {
    fn from(theme: WhiteboardTheme) -> Self {
        Self {
            bg: theme.panel,
            bg_strong: theme.hover,
            text: theme.text,
            text_muted: theme.muted,
            border: theme.border,
            border_strong: theme.border,
            hover: theme.hover,
            active: blend(theme.accent, theme.panel, 0.18),
            accent: theme.accent,
            on_accent: theme.on_accent,
            danger: theme.danger,
        }
    }
}

/// Returns the whiteboard chrome for the current app context.
pub fn chrome(cx: &gpui::App) -> WhiteboardChrome {
    WhiteboardChrome::from(WhiteboardTheme::get(cx))
}

fn blend(foreground: u32, background: u32, alpha: f64) -> u32 {
    let fg_r = (foreground >> 16) & 0xff;
    let fg_g = (foreground >> 8) & 0xff;
    let fg_b = foreground & 0xff;
    let bg_r = (background >> 16) & 0xff;
    let bg_g = (background >> 8) & 0xff;
    let bg_b = background & 0xff;
    let mix = |f: u32, b: u32| (f as f64 * alpha + b as f64 * (1.0 - alpha)).round() as u32;
    (mix(fg_r, bg_r) << 16) | (mix(fg_g, bg_g) << 8) | mix(fg_b, bg_b)
}
