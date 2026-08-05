#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct TextTheme {
    pub link_text: u32,
    pub document_link_text: u32,
    pub inline_code_text: u32,
    pub inline_code_background: u32,
}
