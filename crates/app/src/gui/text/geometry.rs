#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TextHitPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TextCaretRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
