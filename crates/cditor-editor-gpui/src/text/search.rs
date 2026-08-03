use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextSearchRange {
    pub(crate) byte_range: Range<usize>,
    pub(crate) current: bool,
}

pub(crate) const fn search_background(current: bool) -> u32 {
    if current { 0xf59e0bcc } else { 0xfacc1566 }
}
