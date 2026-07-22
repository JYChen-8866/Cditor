pub type PackedRgb = u32;

pub const fn rgb(red: u8, green: u8, blue: u8) -> PackedRgb {
    ((red as u32) << 16) | ((green as u32) << 8) | blue as u32
}

pub const fn channels(color: PackedRgb) -> (u8, u8, u8) {
    (
        ((color >> 16) & 0xff) as u8,
        ((color >> 8) & 0xff) as u8,
        (color & 0xff) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_rgb_round_trips_channels() {
        let color = rgb(0x23, 0x83, 0xe2);
        assert_eq!(color, 0x2383e2);
        assert_eq!(channels(color), (0x23, 0x83, 0xe2));
    }
}
