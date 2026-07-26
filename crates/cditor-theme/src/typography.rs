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
    pub fn notion_like() -> Self {
        let sans = FontFamily {
            primary: ".SystemUIFont".to_owned(),
            fallbacks: vec![
                "SF Pro Text".to_owned(),
                "PingFang SC".to_owned(),
                "Arial".to_owned(),
            ],
        };
        Self {
            body_family: sans.clone(),
            heading_family: sans.clone(),
            code_family: FontFamily {
                primary: "JetBrains Mono".to_owned(),
                fallbacks: vec!["SFMono-Regular".to_owned(), "monospace".to_owned()],
            },
            ui_family: sans,
            body: TextStyleToken::new(FontToken::Body, 16.0, 24.0, 400),
            heading_1: TextStyleToken::new(FontToken::Heading, 30.0, 38.0, 700),
            heading_2: TextStyleToken::new(FontToken::Heading, 24.0, 32.0, 600),
            heading_3: TextStyleToken::new(FontToken::Heading, 20.0, 28.0, 600),
            code: TextStyleToken::new(FontToken::Code, 14.0, 21.0, 400),
            ui: TextStyleToken::new(FontToken::Ui, 14.0, 20.0, 400),
        }
    }
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
    fn default_typography_has_valid_monotonic_text_styles() {
        let typography = Typography::notion_like();
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
