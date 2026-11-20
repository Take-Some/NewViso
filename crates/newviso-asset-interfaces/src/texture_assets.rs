/// Runtime-ready texture packet returned by AssetManager.
///
/// Important: this is not a decoder contract. The codec pipeline must already
/// have converted the source container (DDS/PNG/JPEG/etc.) into RGBA8 or an
/// explicit renderer-native payload. Runtime code only consumes this normalized
/// packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba8TextureAsset {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Rgba8TextureAsset {
    #[inline]
    pub fn expected_len(width: u32, height: u32) -> usize {
        (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4)
    }

    #[inline]
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err(format!("rgba8 texture has zero extent {width}x{height}"));
        }
        let expected = Self::expected_len(width, height);
        if rgba.len() != expected {
            return Err(format!(
                "rgba8 texture payload size mismatch bytes={} expected={} extent={}x{}",
                rgba.len(),
                expected,
                width,
                height
            ));
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeTextureFormat {
    Rgba8Unorm,
    Rgba8Srgb,
    Bc1RgbaUnorm,
    Bc1RgbaSrgb,
    Bc3RgbaUnorm,
    Bc3RgbaSrgb,
    Bc5RgUnorm,
    Bc7RgbaUnorm,
    Bc7RgbaSrgb,
}

impl RuntimeTextureFormat {
    #[inline]
    pub const fn as_wire_id(self) -> u16 {
        match self {
            Self::Rgba8Unorm => 1,
            Self::Rgba8Srgb => 2,
            Self::Bc1RgbaUnorm => 101,
            Self::Bc1RgbaSrgb => 102,
            Self::Bc3RgbaUnorm => 103,
            Self::Bc3RgbaSrgb => 104,
            Self::Bc5RgUnorm => 105,
            Self::Bc7RgbaUnorm => 106,
            Self::Bc7RgbaSrgb => 107,
        }
    }

    #[inline]
    pub const fn from_wire_id(id: u16) -> Option<Self> {
        match id {
            1 => Some(Self::Rgba8Unorm),
            2 => Some(Self::Rgba8Srgb),
            101 => Some(Self::Bc1RgbaUnorm),
            102 => Some(Self::Bc1RgbaSrgb),
            103 => Some(Self::Bc3RgbaUnorm),
            104 => Some(Self::Bc3RgbaSrgb),
            105 => Some(Self::Bc5RgUnorm),
            106 => Some(Self::Bc7RgbaUnorm),
            107 => Some(Self::Bc7RgbaSrgb),
            _ => None,
        }
    }

    #[inline]
    pub fn from_name(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "RGBA8_UNORM" | "RGBA8" => Some(Self::Rgba8Unorm),
            "RGBA8_SRGB" => Some(Self::Rgba8Srgb),
            "BC1_RGBA_UNORM" | "BC1_UNORM" | "BC1" => Some(Self::Bc1RgbaUnorm),
            "BC1_RGBA_SRGB" | "BC1_SRGB" => Some(Self::Bc1RgbaSrgb),
            "BC3_RGBA_UNORM" | "BC3_UNORM" | "BC3" => Some(Self::Bc3RgbaUnorm),
            "BC3_RGBA_SRGB" | "BC3_SRGB" => Some(Self::Bc3RgbaSrgb),
            "BC5_RG_UNORM" | "BC5_UNORM" | "BC5" => Some(Self::Bc5RgUnorm),
            "BC7_RGBA_UNORM" | "BC7_UNORM" | "BC7" => Some(Self::Bc7RgbaUnorm),
            "BC7_RGBA_SRGB" | "BC7_SRGB" => Some(Self::Bc7RgbaSrgb),
            _ => None,
        }
    }

    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rgba8Unorm => "RGBA8_UNORM",
            Self::Rgba8Srgb => "RGBA8_SRGB",
            Self::Bc1RgbaUnorm => "BC1_RGBA_UNORM",
            Self::Bc1RgbaSrgb => "BC1_RGBA_SRGB",
            Self::Bc3RgbaUnorm => "BC3_RGBA_UNORM",
            Self::Bc3RgbaSrgb => "BC3_RGBA_SRGB",
            Self::Bc5RgUnorm => "BC5_RG_UNORM",
            Self::Bc7RgbaUnorm => "BC7_RGBA_UNORM",
            Self::Bc7RgbaSrgb => "BC7_RGBA_SRGB",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTextureMip {
    pub level: u32,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTextureAsset {
    pub width: u32,
    pub height: u32,
    pub format: RuntimeTextureFormat,
    pub mips: Vec<RuntimeTextureMip>,
}

impl RuntimeTextureAsset {
    #[inline]
    pub fn concatenated_payload_and_layout(&self) -> (Vec<u8>, Vec<RuntimeTextureMipLayout>) {
        let total_bytes = self.mips.iter().map(|mip| mip.bytes.len()).sum::<usize>();
        let mut data = Vec::with_capacity(total_bytes);
        let mut layout = Vec::with_capacity(self.mips.len());
        for mip in &self.mips {
            let offset = data.len() as u64;
            data.extend_from_slice(&mip.bytes);
            layout.push(RuntimeTextureMipLayout {
                level: mip.level,
                width: mip.width,
                height: mip.height,
                offset,
                byte_len: mip.bytes.len() as u64,
            });
        }
        (data, layout)
    }

    #[inline]
    pub fn into_concatenated_payload_and_layout(self) -> (Vec<u8>, Vec<RuntimeTextureMipLayout>) {
        let mut mips = self.mips.into_iter();
        let Some(first) = mips.next() else {
            return (Vec::new(), Vec::new());
        };
        let mip_count = mips.len() + 1;
        let remaining_bytes = mips
            .as_slice()
            .iter()
            .map(|mip| mip.bytes.len())
            .sum::<usize>();
        let RuntimeTextureMip {
            level,
            width,
            height,
            bytes,
        } = first;
        let first_byte_len = bytes.len() as u64;
        let mut data = bytes;
        data.reserve(remaining_bytes);
        let mut layout = Vec::with_capacity(mip_count);
        layout.push(RuntimeTextureMipLayout {
            level,
            width,
            height,
            offset: 0,
            byte_len: first_byte_len,
        });
        for mip in mips {
            let RuntimeTextureMip {
                level,
                width,
                height,
                mut bytes,
            } = mip;
            let offset = data.len() as u64;
            let byte_len = bytes.len() as u64;
            data.append(&mut bytes);
            layout.push(RuntimeTextureMipLayout {
                level,
                width,
                height,
                offset,
                byte_len,
            });
        }
        (data, layout)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeTextureMipLayout {
    pub level: u32,
    pub width: u32,
    pub height: u32,
    pub offset: u64,
    pub byte_len: u64,
}

pub mod texture_wire {
    pub const MAGIC: [u8; 4] = *b"NTRT";
    pub const VERSION_RGBA8_V1: u16 = 1;
    pub const VERSION_RUNTIME_V2: u16 = 2;
    pub const HEADER_LEN: usize = 20;
    pub const RUNTIME_HEADER_LEN: usize = 32;
    pub const RUNTIME_MIP_RECORD_LEN: usize = 20;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concatenated_mip_payload_has_stable_offsets() {
        let texture = RuntimeTextureAsset {
            width: 4,
            height: 4,
            format: RuntimeTextureFormat::Rgba8Unorm,
            mips: vec![
                RuntimeTextureMip {
                    level: 0,
                    width: 4,
                    height: 4,
                    bytes: vec![1, 2, 3, 4],
                },
                RuntimeTextureMip {
                    level: 1,
                    width: 2,
                    height: 2,
                    bytes: vec![5, 6],
                },
            ],
        };

        let (payload, layout) = texture.concatenated_payload_and_layout();
        assert_eq!(payload, vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(layout.len(), 2);
        assert_eq!((layout[0].offset, layout[0].byte_len), (0, 4));
        assert_eq!((layout[1].offset, layout[1].byte_len), (4, 2));
    }

    #[test]
    fn owned_single_mip_payload_reuses_source_allocation() {
        let texture = RuntimeTextureAsset {
            width: 4,
            height: 4,
            format: RuntimeTextureFormat::Rgba8Unorm,
            mips: vec![RuntimeTextureMip {
                level: 0,
                width: 4,
                height: 4,
                bytes: vec![1, 2, 3, 4],
            }],
        };
        let source_ptr = texture.mips[0].bytes.as_ptr();

        let (payload, layout) = texture.into_concatenated_payload_and_layout();

        assert_eq!(payload, vec![1, 2, 3, 4]);
        assert_eq!(payload.as_ptr(), source_ptr);
        assert_eq!(layout.len(), 1);
        assert_eq!((layout[0].offset, layout[0].byte_len), (0, 4));
    }
}
