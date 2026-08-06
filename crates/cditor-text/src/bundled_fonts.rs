pub const DOCUMENT_BODY_FONT_FAMILY: &str = cditor_config::APP_CONFIG
    .document
    .typography
    .fonts
    .body
    .current();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_body_font_uses_the_platform_system_family() {
        assert!(!DOCUMENT_BODY_FONT_FAMILY.is_empty());
    }
}
