use std::sync::Arc;

use fontique::Blob;
use parley::FontContext;

const BODY_FONT: cditor_config::BodyFontConfig =
    cditor_config::APP_CONFIG.document.typography.fonts.body;

pub const DOCUMENT_BODY_FONT_FAMILY: &str = BODY_FONT.family;
pub(crate) const DOCUMENT_BODY_FONT_REGULAR: &[u8] = BODY_FONT.regular;
pub(crate) const DOCUMENT_BODY_FONT_MEDIUM: &[u8] = BODY_FONT.medium;
pub(crate) const DOCUMENT_BODY_FONT_SEMIBOLD: &[u8] = BODY_FONT.semibold;
pub(crate) const DOCUMENT_BODY_FONT_BOLD: &[u8] = BODY_FONT.bold;

pub(crate) fn register_document_fonts(font: &mut FontContext) {
    for data in [
        DOCUMENT_BODY_FONT_REGULAR,
        DOCUMENT_BODY_FONT_MEDIUM,
        DOCUMENT_BODY_FONT_SEMIBOLD,
        DOCUMENT_BODY_FONT_BOLD,
    ] {
        let registered = font
            .collection
            .register_fonts(Blob::new(Arc::new(data)), None);
        debug_assert!(registered.iter().any(|(family_id, _)| {
            font.collection.family_name(*family_id) == Some(DOCUMENT_BODY_FONT_FAMILY)
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_document_font_registers_under_the_configured_family() {
        let mut font = FontContext::new();
        register_document_fonts(&mut font);

        assert!(
            font.collection
                .family_names()
                .any(|name| name == DOCUMENT_BODY_FONT_FAMILY)
        );
    }
}
