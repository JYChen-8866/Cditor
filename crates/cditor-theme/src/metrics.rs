use crate::{BorderToken, IconToken, RadiusToken, SpacingToken};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorMetrics {
    pub content_max_width_px: f32,
    pub block_min_height_px: f32,
    pub gutter_width_px: f32,
    pub panel_radius: RadiusToken,
    pub control_border: BorderToken,
    pub icon_size: IconToken,
}

impl EditorMetrics {
    pub const fn notion_like() -> Self {
        Self {
            content_max_width_px: 708.0,
            block_min_height_px: 30.0,
            gutter_width_px: 44.0,
            panel_radius: RadiusToken::Panel,
            control_border: BorderToken::Hairline,
            icon_size: IconToken::Sm,
        }
    }

    pub const fn spacing(token: SpacingToken) -> f32 {
        match token {
            SpacingToken::None => 0.0,
            SpacingToken::Xs => 2.0,
            SpacingToken::Sm => 4.0,
            SpacingToken::Md => 8.0,
            SpacingToken::Lg => 12.0,
            SpacingToken::Xl => 16.0,
        }
    }

    pub const fn radius(token: RadiusToken) -> f32 {
        match token {
            RadiusToken::None => 0.0,
            RadiusToken::Control => 4.0,
            RadiusToken::Panel => 6.0,
            RadiusToken::Round => 999.0,
        }
    }

    pub const fn border_width(token: BorderToken) -> f32 {
        match token {
            BorderToken::None => 0.0,
            BorderToken::Hairline | BorderToken::Control => 1.0,
            BorderToken::Focus => 2.0,
        }
    }

    pub const fn icon_size(token: IconToken) -> f32 {
        match token {
            IconToken::Xs => 12.0,
            IconToken::Sm => 16.0,
            IconToken::Md => 20.0,
            IconToken::Lg => 24.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_scales_are_monotonic_and_professional() {
        assert!(
            EditorMetrics::spacing(SpacingToken::Xs) < EditorMetrics::spacing(SpacingToken::Xl)
        );
        assert!(EditorMetrics::radius(RadiusToken::Panel) <= 8.0);
        assert_eq!(EditorMetrics::border_width(BorderToken::Focus), 2.0);
        assert_eq!(EditorMetrics::icon_size(IconToken::Sm), 16.0);
    }
}
