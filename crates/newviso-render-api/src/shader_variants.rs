use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShaderStageKind {
    Vertex,
    Fragment,
    Compute,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderVariantDefine {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderVariantKeyDto {
    pub shader_family: String,
    pub variant_id: String,
    #[serde(default)]
    pub pass: String,
    #[serde(default)]
    pub stage: Option<ShaderStageKind>,
    #[serde(default)]
    pub defines: Vec<ShaderVariantDefine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderVariantRecordDto {
    pub key: ShaderVariantKeyDto,
    pub source_ref: String,
    pub entry_point: String,
    pub target_profile: String,
    pub cache_key: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShaderVariantRegistryDto {
    pub schema: String,
    #[serde(default)]
    pub deterministic: bool,
    #[serde(default)]
    pub variants: Vec<ShaderVariantRecordDto>,
}

impl ShaderVariantRegistryDto {
    #[inline]
    pub fn find_variant(&self, variant_id: &str) -> Option<&ShaderVariantRecordDto> {
        self.variants
            .iter()
            .find(|record| record.key.variant_id == variant_id)
    }
}
