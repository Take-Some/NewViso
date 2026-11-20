#[inline]
pub(super) fn blocks(width: u32, height: u32) -> (usize, usize) {
    ((width as usize).div_ceil(4), (height as usize).div_ceil(4))
}

#[inline]
pub(super) fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    (((r as u16) >> 3) << 11) | (((g as u16) >> 2) << 5) | ((b as u16) >> 3)
}

#[inline]
pub(super) fn unpack_rgb565(value: u16) -> [u8; 3] {
    let r = ((value >> 11) & 0x1f) as u8;
    let g = ((value >> 5) & 0x3f) as u8;
    let b = (value & 0x1f) as u8;
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}

pub(super) fn alpha_palette(a0: u8, a1: u8) -> [u8; 8] {
    if a0 > a1 {
        [
            a0,
            a1,
            ((6 * a0 as u16 + a1 as u16) / 7) as u8,
            ((5 * a0 as u16 + 2 * a1 as u16) / 7) as u8,
            ((4 * a0 as u16 + 3 * a1 as u16) / 7) as u8,
            ((3 * a0 as u16 + 4 * a1 as u16) / 7) as u8,
            ((2 * a0 as u16 + 5 * a1 as u16) / 7) as u8,
            ((a0 as u16 + 6 * a1 as u16) / 7) as u8,
        ]
    } else {
        [
            a0,
            a1,
            ((4 * a0 as u16 + a1 as u16) / 5) as u8,
            ((3 * a0 as u16 + 2 * a1 as u16) / 5) as u8,
            ((2 * a0 as u16 + 3 * a1 as u16) / 5) as u8,
            ((a0 as u16 + 4 * a1 as u16) / 5) as u8,
            0,
            255,
        ]
    }
}
