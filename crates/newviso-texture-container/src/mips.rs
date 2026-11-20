use crate::error::{Result, TextureContainerError};

#[derive(Debug, Clone)]
pub struct TextureMipData {
    pub level: u32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct TextureEncodedMipData {
    pub level: u32,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

pub fn generate_rgba8_mips(
    width: u32,
    height: u32,
    base_rgba: Vec<u8>,
) -> Result<Vec<TextureMipData>> {
    if width == 0 || height == 0 {
        return Err(TextureContainerError::InvalidExtent {
            name: "<generated>".to_owned(),
            width,
            height,
        });
    }
    let expected = rgba8_len(width, height);
    if base_rgba.len() != expected {
        return Err(TextureContainerError::PayloadSizeMismatch {
            name: "<generated>".to_owned(),
            mip: 0,
            bytes: base_rgba.len(),
            expected,
        });
    }

    let mut out = Vec::new();
    out.push(TextureMipData {
        level: 0,
        width,
        height,
        rgba: base_rgba,
    });
    while out
        .last()
        .map(|m| m.width > 1 || m.height > 1)
        .unwrap_or(false)
    {
        let prev = out.last().expect("at least base mip exists");
        let next_w = (prev.width / 2).max(1);
        let next_h = (prev.height / 2).max(1);
        let next = downsample_rgba8_box(prev.width, prev.height, &prev.rgba, next_w, next_h);
        out.push(TextureMipData {
            level: out.len() as u32,
            width: next_w,
            height: next_h,
            rgba: next,
        });
    }
    Ok(out)
}

#[inline]
pub fn rgba8_len(width: u32, height: u32) -> usize {
    (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4)
}

fn downsample_rgba8_box(src_w: u32, src_h: u32, src: &[u8], dst_w: u32, dst_h: u32) -> Vec<u8> {
    let mut out = vec![0u8; rgba8_len(dst_w, dst_h)];
    for y in 0..dst_h {
        for x in 0..dst_w {
            let sx0 = (x * 2).min(src_w - 1);
            let sy0 = (y * 2).min(src_h - 1);
            let sx1 = (sx0 + 1).min(src_w - 1);
            let sy1 = (sy0 + 1).min(src_h - 1);
            let samples = [(sx0, sy0), (sx1, sy0), (sx0, sy1), (sx1, sy1)];

            // Filter color in premultiplied-alpha space. Straight RGBA averaging
            // lets arbitrary RGB stored under A=0 leak into lower mips. Foliage
            // source PNGs commonly contain white RGB in transparent texels, which
            // made leaf/grass atlases bleach as textureGrad selected coarser mips
            // while the camera rotated. Alpha-weighted RGB is invariant to those
            // hidden transparent colors and is also identical to the old result
            // for fully opaque textures.
            let mut alpha_sum = 0u32;
            let mut rgb_alpha_sum = [0u64; 3];
            for (sx, sy) in samples {
                let i = ((sy as usize * src_w as usize) + sx as usize) * 4;
                let a = src[i + 3] as u32;
                alpha_sum += a;
                rgb_alpha_sum[0] += src[i] as u64 * a as u64;
                rgb_alpha_sum[1] += src[i + 1] as u64 * a as u64;
                rgb_alpha_sum[2] += src[i + 2] as u64 * a as u64;
            }

            let o = ((y as usize * dst_w as usize) + x as usize) * 4;
            out[o + 3] = (alpha_sum / 4) as u8;
            if alpha_sum > 0 {
                out[o] = (rgb_alpha_sum[0] / alpha_sum as u64).min(255) as u8;
                out[o + 1] = (rgb_alpha_sum[1] / alpha_sum as u64).min(255) as u8;
                out[o + 2] = (rgb_alpha_sum[2] / alpha_sum as u64).min(255) as u8;
            } else {
                // Hidden RGB is deliberately canonicalized for fully transparent
                // texels so deeper mip levels cannot resurrect source matte color.
                out[o] = 0;
                out[o + 1] = 0;
                out[o + 2] = 0;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_white_texels_do_not_bleed_into_rgba_mips() {
        // One opaque green texel surrounded by transparent white is the classic
        // foliage-atlas matte case. The lower mip must keep the visible green, not
        // average toward white.
        let rgba = vec![
            24, 96, 32, 255, 255, 255, 255, 0, 255, 255, 255, 0, 255, 255, 255, 0,
        ];
        let mips = generate_rgba8_mips(2, 2, rgba).expect("mip generation");
        assert_eq!(mips.len(), 2);
        assert_eq!(mips[1].rgba, vec![24, 96, 32, 63]);
    }

    #[test]
    fn opaque_rgba_box_filter_keeps_legacy_average() {
        let rgba = vec![
            10, 20, 30, 255, 30, 40, 50, 255, 50, 60, 70, 255, 70, 80, 90, 255,
        ];
        let mips = generate_rgba8_mips(2, 2, rgba).expect("mip generation");
        assert_eq!(mips[1].rgba, vec![40, 50, 60, 255]);
    }
}
