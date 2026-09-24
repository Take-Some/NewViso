use serde::{Deserialize, Serialize};

pub const NOISE_PROVIDER_ABI_ID: &str = "newviso.noise.provider.v1";
pub const NOISE_BACKEND_CAPABILITY_ID: &str = "noise.backend";
pub const NOISE_SERVICE_ID: &str = "noise.api";
pub const ENGINE_NOISE_SERVICE_ID: &str = "engine.noise";

pub const NOISE_METHOD_INFO_JSON: &str = "noise.info_json";
pub const NOISE_METHOD_SAMPLE_JSON_V1: &str = "noise.sample_json_v1";
pub const NOISE_METHOD_FBM_JSON_V1: &str = "noise.fbm_json_v1";
pub const NOISE_METHOD_BATCH_JSON_V1: &str = "noise.batch_json_v1";

pub const NOISE_METHODS_V1: &[&str] = &[
    NOISE_METHOD_INFO_JSON,
    NOISE_METHOD_SAMPLE_JSON_V1,
    NOISE_METHOD_FBM_JSON_V1,
    NOISE_METHOD_BATCH_JSON_V1,
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NoiseServiceInfo {
    pub protocol: String,
    pub provider: String,
    pub algorithm: String,
    pub deterministic: bool,
    pub dimensions: Vec<u8>,
    pub methods: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NoiseSampleRequest {
    #[serde(default)]
    pub seed: u64,
    pub point: Vec<f64>,
    #[serde(default)]
    pub normalized_01: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NoiseFbmRequest {
    #[serde(default)]
    pub seed: u64,
    pub point: Vec<f64>,
    #[serde(default = "default_octaves")]
    pub octaves: u8,
    #[serde(default = "default_frequency")]
    pub frequency: f64,
    #[serde(default = "default_lacunarity")]
    pub lacunarity: f64,
    #[serde(default = "default_gain")]
    pub gain: f64,
    #[serde(default)]
    pub normalized_01: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NoiseBatchRequest {
    #[serde(default)]
    pub seed: u64,
    pub points: Vec<Vec<f64>>,
    #[serde(default)]
    pub fbm: Option<NoiseFbmOptions>,
    #[serde(default)]
    pub normalized_01: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NoiseFbmOptions {
    #[serde(default = "default_octaves")]
    pub octaves: u8,
    #[serde(default = "default_frequency")]
    pub frequency: f64,
    #[serde(default = "default_lacunarity")]
    pub lacunarity: f64,
    #[serde(default = "default_gain")]
    pub gain: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NoiseValueResponse {
    pub value: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct NoiseBatchResponse {
    pub values: Vec<f64>,
}

pub const fn default_octaves() -> u8 {
    4
}

pub const fn default_frequency() -> f64 {
    1.0
}

pub const fn default_lacunarity() -> f64 {
    2.0
}

pub const fn default_gain() -> f64 {
    0.5
}

pub fn validate_point(point: &[f64]) -> Result<(), String> {
    if !(1..=3).contains(&point.len()) {
        return Err(format!(
            "noise point must contain 1, 2, or 3 coordinates; got {}",
            point.len()
        ));
    }
    for (index, value) in point.iter().enumerate() {
        if !value.is_finite() {
            return Err(format!("noise point[{index}] is not finite"));
        }
    }
    Ok(())
}

pub fn validate_fbm_options(options: &NoiseFbmOptions) -> Result<(), String> {
    if !(1..=32).contains(&options.octaves) {
        return Err(format!(
            "noise fBm octaves must be in 1..=32; got {}",
            options.octaves
        ));
    }
    if !options.frequency.is_finite() || options.frequency <= 0.0 {
        return Err("noise fBm frequency must be finite and > 0".to_owned());
    }
    if !options.lacunarity.is_finite() || options.lacunarity <= 0.0 {
        return Err("noise fBm lacunarity must be finite and > 0".to_owned());
    }
    if !options.gain.is_finite() || options.gain < 0.0 || options.gain > 2.0 {
        return Err("noise fBm gain must be finite and in 0..=2".to_owned());
    }
    Ok(())
}

impl From<&NoiseFbmRequest> for NoiseFbmOptions {
    fn from(request: &NoiseFbmRequest) -> Self {
        Self {
            octaves: request.octaves,
            frequency: request.frequency,
            lacunarity: request.lacunarity,
            gain: request.gain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_dimensions() {
        assert!(validate_point(&[1.0]).is_ok());
        assert!(validate_point(&[1.0, 2.0]).is_ok());
        assert!(validate_point(&[1.0, 2.0, 3.0]).is_ok());
        assert!(validate_point(&[]).is_err());
        assert!(validate_point(&[1.0, 2.0, 3.0, 4.0]).is_err());
    }

    #[test]
    fn validates_fbm_bounds() {
        let options = NoiseFbmOptions {
            octaves: 8,
            frequency: 0.25,
            lacunarity: 2.0,
            gain: 0.5,
        };
        assert!(validate_fbm_options(&options).is_ok());
    }
}
