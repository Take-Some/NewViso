use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextureDictionarySelector {
    pub schema: String,
    pub dictionary_path: String,
    #[serde(default)]
    pub texture_name: Option<String>,
    #[serde(default)]
    pub texture_hash: Option<u64>,
    #[serde(default)]
    pub material_path: Option<String>,
}

impl TextureDictionarySelector {
    pub const SCHEMA: &'static str = "newengine.texture_dictionary.selector.v1";

    pub fn by_name(
        dictionary_path: impl Into<String>,
        texture_name: impl Into<String>,
        material_path: Option<String>,
    ) -> Self {
        Self {
            schema: Self::SCHEMA.to_owned(),
            dictionary_path: normalize_path(&dictionary_path.into()),
            texture_name: Some(texture_name.into()),
            texture_hash: None,
            material_path,
        }
    }

    pub fn by_hash(
        dictionary_path: impl Into<String>,
        texture_hash: u64,
        material_path: Option<String>,
    ) -> Self {
        Self {
            schema: Self::SCHEMA.to_owned(),
            dictionary_path: normalize_path(&dictionary_path.into()),
            texture_name: None,
            texture_hash: Some(texture_hash),
            material_path,
        }
    }

    pub fn first(dictionary_path: impl Into<String>, material_path: Option<String>) -> Self {
        Self {
            schema: Self::SCHEMA.to_owned(),
            dictionary_path: normalize_path(&dictionary_path.into()),
            texture_name: None,
            texture_hash: None,
            material_path,
        }
    }

    pub fn to_settings_json(&self) -> String {
        serde_json::to_string(self).expect("selector serialization is infallible")
    }

    pub fn from_settings_json(
        value: &str,
    ) -> std::result::Result<Option<Self>, TextureSelectorError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let selector: Self = serde_json::from_str(trimmed).map_err(TextureSelectorError::Json)?;
        if selector.schema != Self::SCHEMA {
            return Err(TextureSelectorError::BadSchema(selector.schema));
        }
        Ok(Some(selector))
    }

    pub fn parse_material_path(
        value: &str,
    ) -> std::result::Result<Option<Self>, TextureSelectorError> {
        let normalized = normalize_path(value);
        let Some((dictionary_path, selector)) = normalized.rsplit_once('@') else {
            return Ok(None);
        };
        if dictionary_path.trim().is_empty() || selector.trim().is_empty() {
            return Err(TextureSelectorError::InvalidMaterialPath(normalized));
        }
        let selector = selector.trim();
        let parsed_hash = parse_hash(selector);
        let material_path = Some(normalized.clone());
        Ok(Some(match parsed_hash {
            Some(hash) => Self::by_hash(dictionary_path, hash, material_path),
            None => Self::by_name(dictionary_path, selector, material_path),
        }))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TextureSelectorError {
    #[error("texture selector json parse failed: {0}")]
    Json(serde_json::Error),
    #[error("texture selector schema mismatch: {0}")]
    BadSchema(String),
    #[error("invalid material texture reference: {0}")]
    InvalidMaterialPath(String),
}

fn normalize_path(value: &str) -> String {
    let mut s = value.trim().replace('\\', "/");
    while let Some(rest) = s.strip_prefix("./") {
        s = rest.to_owned();
    }
    s = s.trim_start_matches('/').to_owned();
    while s.contains("//") {
        s = s.replace("//", "/");
    }
    s
}

fn parse_hash(value: &str) -> Option<u64> {
    let s = value.trim();
    let raw = s
        .strip_prefix("hash:")
        .or_else(|| s.strip_prefix("hash="))
        .unwrap_or(s);
    if let Some(hex) = raw.strip_prefix("0x") {
        return u64::from_str_radix(hex, 16).ok();
    }
    raw.parse::<u64>().ok()
}
