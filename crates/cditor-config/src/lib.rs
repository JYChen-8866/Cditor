#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AppConfig {
    pub document: DocumentConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentConfig {
    pub typography: DocumentTypographyConfig,
    pub table: TableConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentTypographyConfig {
    pub fonts: DocumentFontsConfig,
    pub styles: DocumentTextStylesConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentFontsConfig {
    pub body: PlatformFontConfig,
    pub code: PlatformFontConfig,
    pub ui: PlatformFontConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlatformFontConfig {
    pub macos: &'static str,
    pub windows: &'static str,
    pub linux: &'static str,
}

impl PlatformFontConfig {
    pub const fn current(self) -> &'static str {
        if cfg!(target_os = "macos") {
            self.macos
        } else if cfg!(target_os = "windows") {
            self.windows
        } else {
            self.linux
        }
    }
}

static PROPORTIONAL_FONT_FAMILY_OVERRIDE: OnceLock<RwLock<Option<String>>> = OnceLock::new();

/// Overrides Cditor's body and UI font for an embedding host.
///
/// Standalone Cditor keeps the platform font from `config/app.toml` unless a
/// host installs an override before constructing editor views.
pub fn set_proportional_font_family(family: impl Into<String>) {
    let family = family.into();
    let family = (!family.trim().is_empty()).then_some(family);
    *PROPORTIONAL_FONT_FAMILY_OVERRIDE
        .get_or_init(|| RwLock::new(None))
        .write()
        .expect("proportional font override lock poisoned") = family;
}

pub fn proportional_font_family() -> String {
    let configured = APP_CONFIG.document.typography.fonts.body.current();
    let override_family = PROPORTIONAL_FONT_FAMILY_OVERRIDE
        .get_or_init(|| RwLock::new(None))
        .read()
        .expect("proportional font override lock poisoned");
    resolve_proportional_font_family(override_family.as_deref(), configured)
}

fn resolve_proportional_font_family(override_family: Option<&str>, configured: &str) -> String {
    override_family.unwrap_or(configured).to_owned()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentTextStylesConfig {
    pub body: TextStyleConfig,
    pub heading_1: TextStyleConfig,
    pub heading_2: TextStyleConfig,
    pub heading_3: TextStyleConfig,
    pub heading_4: TextStyleConfig,
    pub footnote: TextStyleConfig,
    pub table_cell: TextStyleConfig,
    pub table_header: TextStyleConfig,
    pub code: TextStyleConfig,
    pub ui: TextStyleConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyleConfig {
    pub size_px: f32,
    pub line_height_px: f32,
    pub weight: u16,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TableConfig {
    pub default_row_height_px: f32,
    pub cell_padding_x_px: f32,
    pub cell_padding_y_px: f32,
}

include!(concat!(env!("OUT_DIR"), "/app_config.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_document_typography_is_valid_and_shared() {
        let typography = APP_CONFIG.document.typography;
        assert_eq!(typography.fonts.body.current(), ".SystemUIFont");
        assert_eq!(typography.styles.body.weight, 400);
        assert_eq!(typography.styles.table_cell.weight, 400);
    }

    #[test]
    fn host_font_override_takes_precedence_without_changing_the_default() {
        assert_eq!(
            resolve_proportional_font_family(Some("Host Sans"), ".SystemUIFont"),
            "Host Sans"
        );
        assert_eq!(
            resolve_proportional_font_family(None, ".SystemUIFont"),
            ".SystemUIFont"
        );
    }
}
use std::sync::{OnceLock, RwLock};
