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
    pub body: BodyFontConfig,
    pub code: PlatformFontConfig,
    pub ui: PlatformFontConfig,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyFontConfig {
    pub family: &'static str,
    pub regular: &'static [u8],
    pub medium: &'static [u8],
    pub semibold: &'static [u8],
    pub bold: &'static [u8],
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentTextStylesConfig {
    pub body: TextStyleConfig,
    pub heading_1: TextStyleConfig,
    pub heading_2: TextStyleConfig,
    pub heading_3: TextStyleConfig,
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
        assert_eq!(typography.fonts.body.family, "Alibaba PuHuiTi 3.0");
        assert!(!typography.fonts.body.regular.is_empty());
        assert!(!typography.fonts.body.medium.is_empty());
        assert_eq!(typography.styles.body.weight, 400);
        assert_eq!(typography.styles.table_cell.weight, 400);
    }
}
