use std::borrow::Cow;

pub const UI_FONT_FAMILY: &str = "Assistant";
pub const CANVAS_FONT_FAMILY: &str = "Virgil";
pub const CANVAS_FONT_STACK: &str = "'Cditor Canvas Optical Regular', 'Cditor HanziPen Optical Regular', 'Cditor Drafft CJK', 'Cangnanshoujiti', 'KaiTi'";

pub(crate) const OUTLINE_VIRGIL_FAMILY: &str = "Cditor Canvas Optical Regular";
#[cfg(target_os = "macos")]
pub(crate) const OUTLINE_HANZIPEN_FAMILY: &str = "Cditor HanziPen Optical Regular";
pub(crate) const OUTLINE_CJK_FAMILY: &str = "Cditor Drafft CJK";

pub(crate) const VIRGIL: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/FG_Virgil.ttf"));
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) const CJK_REGULAR: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/fonts/AlibabaPuHuiTi-Regular.ttf"
));
pub(crate) const CJK_MEDIUM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/fonts/AlibabaPuHuiTi-Medium.ttf"
));
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) const CJK_BOLD: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/fonts/AlibabaPuHuiTi-Bold.ttf"
));
const ASSISTANT_REGULAR: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/Assistant-Regular.woff2"
));
const ASSISTANT_MEDIUM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/Assistant-Medium.woff2"
));
const ASSISTANT_SEMIBOLD: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/Assistant-SemiBold.woff2"
));
const ASSISTANT_BOLD: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/Assistant-Bold.woff2"
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
    #[cfg(any(target_arch = "wasm32", test))]
    {
        vec![Cow::Borrowed(CJK_REGULAR), Cow::Borrowed(CJK_BOLD)]
    }
    #[cfg(not(any(target_arch = "wasm32", test)))]
    {
        Vec::new()
    }
}
