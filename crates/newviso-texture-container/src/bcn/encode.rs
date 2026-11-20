use super::common::{alpha_palette, blocks, rgb565, unpack_rgb565};

#[inline]
fn sample_rgba(width: u32, height: u32, rgba: &[u8], x: u32, y: u32) -> [u8; 4] {
    let sample_x = x.min(width.saturating_sub(1));
    let sample_y = y.min(height.saturating_sub(1));
    let index = ((sample_y as usize * width as usize) + sample_x as usize) * 4;
    [
        rgba[index],
        rgba[index + 1],
        rgba[index + 2],
        rgba[index + 3],
    ]
}

pub(super) fn encode_bc1(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let (block_width, block_height) = blocks(width, height);
    let mut out = Vec::with_capacity(block_width * block_height * 8);
    for block_y in 0..block_height as u32 {
        for block_x in 0..block_width as u32 {
            let mut min = [255u8; 3];
            let mut max = [0u8; 3];
            let mut pixels = [[0u8; 4]; 16];
            for y in 0..4u32 {
                for x in 0..4u32 {
                    let pixel = sample_rgba(width, height, rgba, block_x * 4 + x, block_y * 4 + y);
                    pixels[(y * 4 + x) as usize] = pixel;
                    for channel in 0..3 {
                        min[channel] = min[channel].min(pixel[channel]);
                        max[channel] = max[channel].max(pixel[channel]);
                    }
                }
            }

            let mut color0 = rgb565(max[0], max[1], max[2]);
            let mut color1 = rgb565(min[0], min[1], min[2]);
            if color0 <= color1 {
                core::mem::swap(&mut color0, &mut color1);
            }
            let palette0 = unpack_rgb565(color0);
            let palette1 = unpack_rgb565(color1);
            let palette = [
                palette0,
                palette1,
                [
                    ((2 * palette0[0] as u16 + palette1[0] as u16) / 3) as u8,
                    ((2 * palette0[1] as u16 + palette1[1] as u16) / 3) as u8,
                    ((2 * palette0[2] as u16 + palette1[2] as u16) / 3) as u8,
                ],
                [
                    ((palette0[0] as u16 + 2 * palette1[0] as u16) / 3) as u8,
                    ((palette0[1] as u16 + 2 * palette1[1] as u16) / 3) as u8,
                    ((palette0[2] as u16 + 2 * palette1[2] as u16) / 3) as u8,
                ],
            ];

            let mut indices = 0u32;
            for (index, pixel) in pixels.iter().enumerate() {
                let mut best = 0usize;
                let mut best_distance = u32::MAX;
                for (candidate_index, candidate) in palette.iter().enumerate() {
                    let dr = pixel[0] as i32 - candidate[0] as i32;
                    let dg = pixel[1] as i32 - candidate[1] as i32;
                    let db = pixel[2] as i32 - candidate[2] as i32;
                    let distance = (dr * dr + dg * dg + db * db) as u32;
                    if distance < best_distance {
                        best_distance = distance;
                        best = candidate_index;
                    }
                }
                indices |= (best as u32) << (index * 2);
            }

            out.extend_from_slice(&color0.to_le_bytes());
            out.extend_from_slice(&color1.to_le_bytes());
            out.extend_from_slice(&indices.to_le_bytes());
        }
    }
    out
}

pub(super) fn encode_bc3(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let (block_width, block_height) = blocks(width, height);
    let mut out = Vec::with_capacity(block_width * block_height * 16);
    for block_y in 0..block_height as u32 {
        for block_x in 0..block_width as u32 {
            let mut alpha = [255u8; 16];
            let mut block_rgba = [0u8; 64];
            for y in 0..4u32 {
                for x in 0..4u32 {
                    let pixel = sample_rgba(width, height, rgba, block_x * 4 + x, block_y * 4 + y);
                    let index = (y * 4 + x) as usize;
                    alpha[index] = pixel[3];
                    block_rgba[index * 4..index * 4 + 4].copy_from_slice(&pixel);
                }
            }
            out.extend_from_slice(&encode_bc4_block(&alpha));
            out.extend_from_slice(&encode_bc1(4, 4, &block_rgba));
        }
    }
    out
}

pub(super) fn encode_bc5(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let (block_width, block_height) = blocks(width, height);
    let mut out = Vec::with_capacity(block_width * block_height * 16);
    for block_y in 0..block_height as u32 {
        for block_x in 0..block_width as u32 {
            let mut red = [128u8; 16];
            let mut green = [128u8; 16];
            for y in 0..4u32 {
                for x in 0..4u32 {
                    let pixel = sample_rgba(width, height, rgba, block_x * 4 + x, block_y * 4 + y);
                    let index = (y * 4 + x) as usize;
                    red[index] = pixel[0];
                    green[index] = pixel[1];
                }
            }
            out.extend_from_slice(&encode_bc4_block(&red));
            out.extend_from_slice(&encode_bc4_block(&green));
        }
    }
    out
}

fn encode_bc4_block(values: &[u8; 16]) -> [u8; 8] {
    let mut min = 255u8;
    let mut max = 0u8;
    for &value in values {
        min = min.min(value);
        max = max.max(value);
    }
    let alpha0 = max;
    let alpha1 = min;
    let palette = alpha_palette(alpha0, alpha1);
    let mut bits = 0u64;
    for (index, &value) in values.iter().enumerate() {
        let mut best = 0u64;
        let mut best_distance = u16::MAX;
        for (candidate_index, &candidate) in palette.iter().enumerate() {
            let distance = (value as i16 - candidate as i16).unsigned_abs();
            if distance < best_distance {
                best_distance = distance;
                best = candidate_index as u64;
            }
        }
        bits |= best << (index * 3);
    }

    let mut out = [0u8; 8];
    out[0] = alpha0;
    out[1] = alpha1;
    for index in 0..6 {
        out[2 + index] = ((bits >> (8 * index)) & 0xff) as u8;
    }
    out
}
