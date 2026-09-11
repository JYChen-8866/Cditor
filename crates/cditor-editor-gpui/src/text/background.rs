use crate::theme::{GuiTheme, selection_background_color};

pub(super) fn text_selection_background(theme: GuiTheme) -> u32 {
    // Prototype accent-soft: a light blue that remains clearly visible without
    // competing with the text. Glyphs are painted after this quad.
    (selection_background_color(theme) << 8) | 0xff
}
