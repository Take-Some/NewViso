#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TextureDictionaryManifest {
    pub version: u16,
    pub default_format: String,
    pub entries: Vec<TextureEntryMeta>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TextureEntryMeta {
    pub name: String,
    pub name_hash: u64,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub color_space: String,
    pub byte_offset: u64,
    pub byte_len: u64,
    pub mip_count: u32,
    pub mips: Vec<TextureMipMeta>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TextureMipMeta {
    pub level: u32,
    pub width: u32,
    pub height: u32,
    pub byte_offset: u64,
    pub byte_len: u64,
}
