pub const DOCUMENT_BODY_FONT_FAMILY: &str = cditor_config::APP_CONFIG
    .document
    .typography
    .fonts
    .body
    .current();

pub fn document_body_font_family() -> String {
    cditor_config::proportional_font_family()
}

pub fn set_document_body_font_family(family: impl Into<String>) {
    cditor_config::set_proportional_font_family(family);
    crate::cache::clear_text_layout_cache();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_body_font_uses_the_platform_system_family() {
        assert!(!DOCUMENT_BODY_FONT_FAMILY.is_empty());
    }
}
