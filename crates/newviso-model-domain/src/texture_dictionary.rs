use serde::{Deserialize, Serialize};

use crate::{
    TEXTURE_DICTIONARY_ASSET_KIND, TEXTURE_DICTIONARY_CONTAINER, TEXTURE_DICTIONARY_MANIFEST_SCHEMA,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextureDictionaryManifest {
    pub schema: String,
    pub source: String,
    pub asset_kind: String,
    pub container: String,
    pub runtime_ready: bool,
    /// Reserved for private cache implementation notes. Public authored content does not reference cache extensions.
    pub previous_cache_container: Option<String>,
    pub entries: Vec<TextureDictionaryEntry>,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
}

impl Default for TextureDictionaryManifest {
    fn default() -> Self {
        Self {
            schema: TEXTURE_DICTIONARY_MANIFEST_SCHEMA.to_owned(),
            source: String::new(),
            asset_kind: TEXTURE_DICTIONARY_ASSET_KIND.to_owned(),
            container: TEXTURE_DICTIONARY_CONTAINER.to_owned(),
            runtime_ready: true,
            previous_cache_container: None,
            entries: Vec::new(),
            warnings: Vec::new(),
            notes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TextureDictionaryEntry {
    pub name: String,
    pub name_hash: u64,
    pub stable_id: String,
    pub pixel_format: String,
    pub color_space: String,
    pub width: u32,
    pub height: u32,
    pub mip_count: u32,
    pub warnings: Vec<String>,
}
