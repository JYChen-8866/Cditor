use std::io::Cursor;

use image::{ImageEncoder, RgbaImage};

pub(crate) const MAX_ENCODED_IMAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_DECODED_IMAGE_WIDTH: u32 = 8192;
const MAX_DECODED_IMAGE_HEIGHT: u32 = 8192;
const MAX_DECODED_IMAGE_ALLOC_BYTES: u64 = 64 * 1024 * 1024;
const DISPLAY_IMAGE_MAX_EDGE_PX: u32 = 2048;

pub(crate) fn validated_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.is_empty() || bytes.len() > MAX_ENCODED_IMAGE_BYTES {
        return None;
    }
    let dimensions = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    decoded_dimensions_within_limits(dimensions.0, dimensions.1).then_some(dimensions)
}

pub(crate) fn decode_display_rgba(bytes: &[u8]) -> Option<RgbaImage> {
    decode_rgba(bytes, Some(DISPLAY_IMAGE_MAX_EDGE_PX))
}

pub(crate) fn decode_rgba(bytes: &[u8], max_edge: Option<u32>) -> Option<RgbaImage> {
    let dimensions = validated_dimensions(bytes)?;
    let mut reader = image::ImageReader::new(Cursor::new(bytes));
    reader = reader.with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODED_IMAGE_WIDTH);
    limits.max_image_height = Some(MAX_DECODED_IMAGE_HEIGHT);
    limits.max_alloc = Some(MAX_DECODED_IMAGE_ALLOC_BYTES);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let output_dimensions = max_edge
        .map(|edge| raster_dimensions_within_edge(dimensions.0, dimensions.1, edge))
        .unwrap_or(dimensions);
    let raster = if output_dimensions == dimensions {
        decoded.into_rgba8()
    } else {
        decoded
            .resize_exact(
                output_dimensions.0,
                output_dimensions.1,
                image::imageops::FilterType::Triangle,
            )
            .into_rgba8()
    };
    rgba_allocation_within_limit(raster.width(), raster.height()).then_some(raster)
}

pub(crate) fn encode_png(raster: &RgbaImage) -> Option<Vec<u8>> {
    if !rgba_allocation_within_limit(raster.width(), raster.height()) {
        return None;
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            raster.as_raw(),
            raster.width(),
            raster.height(),
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(png)
}

pub(crate) fn rotated_raster_dimensions(width: u32, height: u32, angle: f64) -> Option<(u32, u32)> {
    if width == 0 || height == 0 || !angle.is_finite() {
        return None;
    }
    let cosine = angle.cos().abs();
    let sine = angle.sin().abs();
    let target_width = (f64::from(width) * cosine + f64::from(height) * sine - 1e-9)
        .ceil()
        .max(1.0);
    let target_height = (f64::from(width) * sine + f64::from(height) * cosine - 1e-9)
        .ceil()
        .max(1.0);
    if target_width > f64::from(u32::MAX) || target_height > f64::from(u32::MAX) {
        return None;
    }
    let dimensions = (target_width as u32, target_height as u32);
    rgba_allocation_within_limit(dimensions.0, dimensions.1).then_some(dimensions)
}

fn raster_dimensions_within_edge(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
    let max_edge = max_edge.max(1);
    let longest_edge = width.max(height);
    if longest_edge <= max_edge {
        return (width, height);
    }
    if width >= height {
        let scaled_height =
            (u64::from(height) * u64::from(max_edge) + u64::from(width) / 2) / u64::from(width);
        (max_edge, u32::try_from(scaled_height).unwrap_or(1).max(1))
    } else {
        let scaled_width =
            (u64::from(width) * u64::from(max_edge) + u64::from(height) / 2) / u64::from(height);
        (u32::try_from(scaled_width).unwrap_or(1).max(1), max_edge)
    }
}

fn decoded_dimensions_within_limits(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= MAX_DECODED_IMAGE_WIDTH
        && height <= MAX_DECODED_IMAGE_HEIGHT
        && rgba_allocation_within_limit(width, height)
}

fn rgba_allocation_within_limit(width: u32, height: u32) -> bool {
    u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .is_some_and(|bytes| bytes <= MAX_DECODED_IMAGE_ALLOC_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded_png(width: u32, height: u32) -> Vec<u8> {
        encode_png(&RgbaImage::new(width, height)).expect("test PNG should encode")
    }

    #[test]
    fn display_decode_preserves_aspect_ratio_with_a_bounded_edge() {
        let decoded = decode_display_rgba(&encoded_png(4096, 1024)).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2048, 512));
    }

    #[test]
    fn dimensions_reject_a_header_above_the_decoded_budget() {
        assert!(!decoded_dimensions_within_limits(4097, 4097));
        assert!(!decoded_dimensions_within_limits(8192, 2049));
    }

    #[test]
    fn rotated_dimensions_reject_non_finite_and_over_budget_outputs() {
        assert!(rotated_raster_dimensions(100, 100, f64::NAN).is_none());
        assert!(rotated_raster_dimensions(4096, 4096, std::f64::consts::FRAC_PI_4).is_none());
        assert_eq!(
            rotated_raster_dimensions(2048, 2048, std::f64::consts::FRAC_PI_4),
            Some((2897, 2897))
        );
    }
}
