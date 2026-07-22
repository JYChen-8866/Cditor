use crate::{GuiTheme, ThemeId};

pub const fn default_palette(id: ThemeId) -> GuiTheme {
    match id {
        ThemeId::CditorLight => GuiTheme::light(),
        ThemeId::CditorDark => GuiTheme::dark(),
    }
}
