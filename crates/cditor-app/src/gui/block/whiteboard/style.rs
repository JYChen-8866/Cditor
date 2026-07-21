use std::rc::Rc;

use ding_board::{WhiteboardStyle, WhiteboardStyleFn};
use gpui::{Hsla, rgb};

use crate::gui::GuiTheme;

pub(crate) fn whiteboard_style_fn(theme: GuiTheme) -> WhiteboardStyleFn {
    Rc::new(move || whiteboard_style(theme))
}

fn whiteboard_style(theme: GuiTheme) -> WhiteboardStyle {
    WhiteboardStyle {
        bg: Hsla::from(rgb(theme.page)),
        grid: Hsla::from(rgb(theme.border)),
        text: Hsla::from(rgb(theme.muted)),
        ink: Hsla::from(rgb(theme.text)),
        panel: Hsla::from(rgb(theme.surface)),
        panel_strong: Hsla::from(rgb(theme.surface)),
        accent: Hsla::from(rgb(theme.action_background)),
        selection: Hsla::from(rgb(theme.action_accent)),
        swatches: vec![
            Hsla::from(rgb(theme.text)),
            Hsla::from(rgb(theme.action_accent)),
            Hsla::from(rgb(theme.danger)),
            Hsla::from(rgb(theme.muted)),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whiteboard_style_maps_editor_theme_semantically() {
        let theme = GuiTheme::light();
        let style = whiteboard_style(theme);

        assert_eq!(style.bg, Hsla::from(rgb(theme.page)));
        assert_eq!(style.grid, Hsla::from(rgb(theme.border)));
        assert_eq!(style.selection, Hsla::from(rgb(theme.action_accent)));
        assert_eq!(style.swatches.len(), 4);
    }
}
