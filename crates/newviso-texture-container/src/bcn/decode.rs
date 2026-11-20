use crate::mips::rgba8_len;

use super::{
    common::{alpha_palette, blocks, unpack_rgb565},
    BcnEncodeError,
};

pub(super) fn decode_bc1(
    width: u32,
    height: u32,
    bytes: &[u8],
    format: &str,
) -> Result<Vec<u8>, BcnEncodeError> {
    let (block_width, block_height) = blocks(width, height);
    let expected = block_width * block_height * 8;
    validate_payload(bytes, expected, width, height, format)?;

    let mut out = vec![0u8; rgba8_len(width, height)];
    let mut cursor = 0usize;
    for block_y in 0..block_height as u32 {
        for block_x in 0..block_width as u32 {
            let color0 = u16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
            let color1 = u16::from_le_bytes([bytes[cursor + 2], bytes[cursor + 3]]);
            let indices = u32::from_le_bytes([
                bytes[cursor + 4],
                bytes[cursor + 5],
                bytes[cursor + 6],
                bytes[cursor + 7],
            ]);
            cursor += 8;

            let palette0 = unpack_rgb565(color0);
            let palette1 = unpack_rgb565(color1);
            let palette = [
                [palette0[0], palette0[1], palette0[2], 255],
                [palette1[0], palette1[1], palette1[2], 255],
                [
                    ((2 * palette0[0] as u16 + palette1[0] as u16) / 3) as u8,
                    ((2 * palette0[1] as u16 + palette1[1] as u16) / 3) as u8,
                    ((2 * palette0[2] as u16 + palette1[2] as u16) / 3) as u8,
                    255,
                ],
                [
                    ((palette0[0] as u16 + 2 * palette1[0] as u16) / 3) as u8,
                    ((palette0[1] as u16 + 2 * palette1[1] as u16) / 3) as u8,
                    ((palette0[2] as u16 + 2 * palette1[2] as u16) / 3) as u8,
                    255,
                ],
            ];
            write_decoded_block(width, height, &mut out, block_x, block_y, |index| {
                palette[((indices >> (index * 2)) & 3) as usize]
            });
        }
    }
    Ok(out)
}

pub(super) fn decode_bc3(
    width: u32,
    height: u32,
    bytes: &[u8],
    format: &str,
) -> Result<Vec<u8>, BcnEncodeError> {
    let (block_width, block_height) = blocks(width, height);
    let expected = block_width * block_height * 16;
    validate_payload(bytes, expected, width, height, format)?;

    let mut out = vec![0u8; rgba8_len(width, height)];
    let mut cursor = 0usize;
    for block_y in 0..block_height as u32 {
        for block_x in 0..block_width as u32 {
            let alphas = decode_bc4_block(&bytes[cursor..cursor + 8]);
            let colors = decode_bc1(4, 4, &bytes[cursor + 8..cursor + 16], format)?;
            cursor += 16;
            write_decoded_block(width, height, &mut out, block_x, block_y, |index| {
                [
                    colors[index * 4],
                    colors[index * 4 + 1],
                    colors[index * 4 + 2],
                    alphas[index],
                ]
            });
        }
    }
    Ok(out)
}

pub(super) fn decode_bc5(
    width: u32,
    height: u32,
    bytes: &[u8],
    format: &str,
) -> Result<Vec<u8>, BcnEncodeError> {
    let (block_width, block_height) = blocks(width, height);
    let expected = block_width * block_height * 16;
    validate_payload(bytes, expected, width, height, format)?;

    let mut out = vec![0u8; rgba8_len(width, height)];
    let mut cursor = 0usize;
    for block_y in 0..block_height as u32 {
        for block_x in 0..block_width as u32 {
            let red = decode_bc4_block(&bytes[cursor..cursor + 8]);
            let green = decode_bc4_block(&bytes[cursor + 8..cursor + 16]);
            cursor += 16;
            write_decoded_block(width, height, &mut out, block_x, block_y, |index| {
                [red[index], green[index], 255, 255]
            });
        }
    }
    Ok(out)
}

fn validate_payload(
    bytes: &[u8],
    expected: usize,
    width: u32,
    height: u32,
    format: &str,
) -> Result<(), BcnEncodeError> {
    if bytes.len() != expected {
        return Err(BcnEncodeError::InvalidBcnPayload {
            bytes: bytes.len(),
            expected,
            width,
            height,
            format: format.to_owned(),
        });
    }
    Ok(())
}

fn decode_bc4_block(bytes: &[u8]) -> [u8; 16] {
    let palette = alpha_palette(bytes[0], bytes[1]);
    let mut bits = 0u64;
    for index in 0..6 {
        bits |= (bytes[2 + index] as u64) << (8 * index);
    }
    let mut out = [0u8; 16];
    for (index, value) in out.iter_mut().enumerate() {
        *value = palette[((bits >> (index * 3)) & 7) as usize];
    }
    out
}

fn write_decoded_block<F: Fn(usize) -> [u8; 4]>(
    width: u32,
    height: u32,
    out: &mut [u8],
    block_x: u32,
    block_y: u32,
    sample: F,
) {
    for y in 0..4u32 {
        for x in 0..4u32 {
            let destination_x = block_x * 4 + x;
            let destination_y = block_y * 4 + y;
            if destination_x >= width || destination_y >= height {
                continue;
            }
            let destination =
                ((destination_y as usize * width as usize) + destination_x as usize) * 4;
            let pixel = sample((y * 4 + x) as usize);
            out[destination..destination + 4].copy_from_slice(&pixel);
        }
    }
}
