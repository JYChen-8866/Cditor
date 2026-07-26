use crate::FontToken;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFamily {
    pub primary: String,
    pub fallbacks: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyleToken {
    pub font: FontToken,
    pub size_px: f32,
    pub line_height_px: f32,
    pub weight: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Typography {
    pub body_family: FontFamily,
    pub heading_family: FontFamily,
    pub code_family: FontFamily,
    pub ui_family: FontFamily,
    pub body: TextStyleToken,
    pub heading_1: TextStyleToken,
    pub heading_2: TextStyleToken,
    pub heading_3: TextStyleToken,
    pub code: TextStyleToken,
    pub ui: TextStyleToken,
}

impl Typography {
    pub fn from_app_config() -> Self {
        let config = cditor_config::APP_CONFIG.document.typography;
        let body = FontFamily {
            primary: config.fonts.body.family.to_owned(),
            fallbacks: Vec::new(),
        };
        let ui = FontFamily {
            primary: config.fonts.ui.current().to_owned(),
            fallbacks: Vec::new(),
        };
        let code = FontFamily {
            primary: config.fonts.code.current().to_owned(),
            fallbacks: vec!["monospace".to_owned()],
        };
        Self {
            body_family: body.clone(),
            heading_family: body,
            code_family: code,
            ui_family: ui,
            body: token(FontToken::Body, config.styles.body),
            heading_1: token(FontToken::Heading, config.styles.heading_1),
            heading_2: token(FontToken::Heading, config.styles.heading_2),
            heading_3: token(FontToken::Heading, config.styles.heading_3),
            code: token(FontToken::Code, config.styles.code),
            ui: token(FontToken::Ui, config.styles.ui),
        }
    }
}

fn token(font: FontToken, config: cditor_config::TextStyleConfig) -> TextStyleToken {
    TextStyleToken::new(font, config.size_px, config.line_height_px, config.weight)
}

impl TextStyleToken {
    pub const fn new(font: FontToken, size_px: f32, line_height_px: f32, weight: u16) -> Self {
        Self {
            font,
            size_px,
            line_height_px,
            weight,
        }
    }

    pub fn is_valid(self) -> bool {
        self.size_px.is_finite()
            && self.size_px > 0.0
            && self.line_height_px.is_finite()
            && self.line_height_px >= self.size_px
            && (1..=1000).contains(&self.weight)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_typography_has_valid_monotonic_text_styles() {
        let typography = Typography::from_app_config();
        for style in [
            typography.body,
            typography.heading_1,
            typography.heading_2,
            typography.heading_3,
            typography.code,
            typography.ui,
        ] {
            assert!(style.is_valid());
        }
        assert!(typography.heading_1.size_px > typography.heading_2.size_px);
        assert!(typography.heading_2.size_px > typography.heading_3.size_px);
    }
}
