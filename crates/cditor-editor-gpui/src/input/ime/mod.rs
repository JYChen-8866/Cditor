pub(crate) mod adapter;
mod document;
mod geometry;
mod offsets;
pub(crate) mod support;

pub use offsets::{
    clamp_to_char_boundary, marked_preview_range_to_base_range, utf8_range_to_utf16_range,
    utf8_to_utf16_offset, utf16_range_to_utf8_range,
};
