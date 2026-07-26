use std::borrow::Cow;

pub const UI_FONT_FAMILY: &str = "Assistant";
pub const CANVAS_FONT_FAMILY: &str = "Virgil";
pub const CANVAS_FONT_STACK: &str = "'Virgil', 'HanziPen SC', 'Cangnanshoujiti', 'KaiTi'";

pub(crate) const VIRGIL: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/FG_Virgil.ttf"));
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
