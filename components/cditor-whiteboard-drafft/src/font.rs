use std::borrow::Cow;

pub const UI_FONT_FAMILY: &str = "Assistant";
pub const CANVAS_FONT_FAMILY: &str = "Virgil";
pub const CANVAS_FONT_STACK: &str = "'Cditor Canvas Optical Regular', 'Cditor HanziPen Optical Regular', 'Cditor Drafft CJK', 'Cangnanshoujiti', 'KaiTi'";

pub(crate) const OUTLINE_VIRGIL_FAMILY: &str = "Cditor Canvas Optical Regular";
#[cfg(target_os = "macos")]
pub(crate) const OUTLINE_HANZIPEN_FAMILY: &str = "Cditor HanziPen Optical Regular";

pub(crate) const VIRGIL: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/FG_Virgil.ttf"));
const ASSISTANT_REGULAR: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/Assistant-Regular.ttf"
));
const ASSISTANT_MEDIUM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/Assistant-Medium.ttf"
));
const ASSISTANT_SEMIBOLD: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/Assistant-SemiBold.ttf"
));
const ASSISTANT_BOLD: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/Assistant-Bold.ttf"
));

pub fn bundled_fonts() -> Vec<Cow<'static, [u8]>> {
    vec![
        Cow::Borrowed(VIRGIL),
        Cow::Borrowed(ASSISTANT_REGULAR),
        Cow::Borrowed(ASSISTANT_MEDIUM),
        Cow::Borrowed(ASSISTANT_SEMIBOLD),
        Cow::Borrowed(ASSISTANT_BOLD),
    ]
}

pub fn cjk_fallback_fonts() -> Vec<Cow<'static, [u8]>> {
    Vec::new()
}
