#![forbid(unsafe_op_in_unsafe_fn)]

use abi_stable::std_types::{ROption, RSlice, RString};
use abi_stable::StableAbi;

#[repr(C)]
#[derive(Debug, Clone, StableAbi)]
pub struct PluginBinaryAssetV1 {
    pub media_type: RString,
    pub bytes: RSlice<'static, u8>,
}

impl PluginBinaryAssetV1 {
    #[inline]
    pub fn new(media_type: impl Into<RString>, bytes: &'static [u8]) -> Self {
        Self {
            media_type: media_type.into(),
            bytes: RSlice::from_slice(bytes),
        }
    }

    #[inline]
    pub fn png(bytes: &'static [u8]) -> Self {
        Self::new("image/png", bytes)
    }
}

#[repr(C)]
#[derive(Debug, Clone, StableAbi)]
pub struct PluginUiAssetsV1 {
    pub icon_small: ROption<PluginBinaryAssetV1>,
}

impl PluginUiAssetsV1 {
    #[inline]
    pub const fn empty() -> Self {
        Self {
            icon_small: ROption::RNone,
        }
    }

    #[inline]
    pub fn icon_png(bytes: &'static [u8]) -> Self {
        Self {
            icon_small: ROption::RSome(PluginBinaryAssetV1::png(bytes)),
        }
    }
}
