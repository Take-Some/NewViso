pub mod physics_audio;
pub use physics_audio::{
    AdmittedImpact, ImpactMaterial, PhysicsAudioContact, PhysicsAudioPolicy, PhysicsAudioRouter,
};

use serde::{Deserialize, Serialize};

pub const ENGINE_AUDIO_SERVICE_ID: &str = "engine.audio";
pub const AUDIO_SERVICE_ID: &str = "audio.api";
pub const AUDIO_BACKEND_CAPABILITY_ID: &str = "audio.backend";
pub const AUDIO_PROVIDER_ABI_ID: &str = "newviso.audio.provider.v1";

pub const AUDIO_METHOD_INFO_JSON: &str = "info_json";
pub const AUDIO_METHOD_PRELOAD_CLIP_JSON_V1: &str = "preload_clip_json_v1";
pub const AUDIO_METHOD_PLAY_CLIP_JSON_V1: &str = "play_clip_json_v1";
pub const AUDIO_METHOD_PLAY_CUE_JSON_V1: &str = "play_cue_json_v1";
pub const AUDIO_METHOD_STOP_VOICE_JSON_V1: &str = "stop_voice_json_v1";
pub const AUDIO_METHOD_SET_VOICE_JSON_V1: &str = "set_voice_json_v1";
pub const AUDIO_METHOD_SET_LISTENER_JSON_V1: &str = "set_listener_json_v1";
pub const AUDIO_METHOD_DIAGNOSTICS_JSON_V1: &str = "diagnostics_json_v1";
pub const AUDIO_METHOD_SHUTDOWN_V1: &str = "shutdown_v1";

pub const AUDIO_PLAYBACK_METHODS_V1: &[&str] = &[
    AUDIO_METHOD_INFO_JSON,
    AUDIO_METHOD_PRELOAD_CLIP_JSON_V1,
    AUDIO_METHOD_PLAY_CLIP_JSON_V1,
    AUDIO_METHOD_PLAY_CUE_JSON_V1,
    AUDIO_METHOD_STOP_VOICE_JSON_V1,
    AUDIO_METHOD_SET_VOICE_JSON_V1,
    AUDIO_METHOD_SET_LISTENER_JSON_V1,
    AUDIO_METHOD_DIAGNOSTICS_JSON_V1,
    AUDIO_METHOD_SHUTDOWN_V1,
];

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AudioServiceInfo {
    pub protocol: String,
    pub provider: String,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub methods: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct AudioClipRef {
    pub uri: String,
}

impl AudioClipRef {
    pub fn new(uri: impl Into<String>) -> Self {
        Self { uri: uri.into() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AudioPreloadRequest {
    pub clip: AudioClipRef,
}

impl AudioPreloadRequest {
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            clip: AudioClipRef::new(uri),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AudioPreloadAck {
    pub accepted: bool,
    pub provider: String,
    pub cached_bytes: usize,
    #[serde(default)]
    pub message: String,
}

fn default_version() -> u32 {
    1
}
fn default_gain() -> f32 {
    1.0
}
fn default_speed() -> f32 {
    1.0
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AudioPlayRequest {
    #[serde(default = "default_version")]
    pub version: u32,
    pub clip: AudioClipRef,
    #[serde(default = "default_gain")]
    pub gain: f32,
    #[serde(default = "default_speed")]
    pub speed: f32,
    #[serde(default)]
    pub looping: bool,
    #[serde(default)]
    pub paused: bool,
    /// Optional source position. Provider must advertise / implement spatial clip playback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

impl AudioPlayRequest {
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            version: 1,
            clip: AudioClipRef::new(uri),
            gain: 1.0,
            speed: 1.0,
            looping: false,
            paused: false,
            position: None,
            route: None,
            scope_id: None,
            seed: None,
        }
    }

    pub fn sanitized(mut self) -> Result<Self, String> {
        if self.version != 1 {
            return Err(format!(
                "unsupported AudioPlayRequest version {}; expected 1",
                self.version
            ));
        }
        self.clip.uri = normalize_audio_uri(&self.clip.uri)?;
        if self
            .position
            .is_some_and(|p| p.iter().any(|x| !x.is_finite()))
        {
            return Err("audio clip position must contain finite coordinates".to_owned());
        }
        if self.route.as_deref().is_some_and(|r| r.trim().is_empty()) {
            return Err("audio clip route must not be empty".to_owned());
        }
        self.gain = sanitize_gain(self.gain);
        self.speed = sanitize_speed(self.speed);
        Ok(self)
    }
}

/// Strict XVAG VFS reference (no selectors, cue dictionaries, absolute paths).
pub fn normalize_xvag_uri(uri: &str) -> Result<String, String> {
    if uri.trim().starts_with('/') || uri.trim().starts_with('\\') {
        return Err("XVAG audio paths must be relative to the VFS root".to_owned());
    }
    let normalized = normalize_audio_uri(uri)?;
    if !normalized.to_ascii_lowercase().ends_with(".xvag")
        || normalized.contains('@')
        || normalized.contains(':')
        || normalized.contains('?')
        || normalized.contains('#')
        || normalized.starts_with("//")
    {
        return Err(format!(
            "expected a plain XVAG asset reference, got '{normalized}'"
        ));
    }
    Ok(normalized)
}

/// Authored sound-cue reference resolved by AssetManager (YSCD dictionary).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct AudioCueRef {
    pub logical_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AudioCuePlayRequest {
    #[serde(default = "default_version")]
    pub version: u32,
    pub cue: AudioCueRef,
    #[serde(default = "default_gain")]
    pub gain: f32,
    #[serde(default = "default_speed")]
    pub pitch: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
}

impl AudioCuePlayRequest {
    pub fn new(logical_path: impl Into<String>) -> Self {
        Self {
            version: 1,
            cue: AudioCueRef {
                logical_path: logical_path.into(),
            },
            gain: 1.0,
            pitch: 1.0,
            position: None,
            scope_id: None,
            seed: None,
            route: None,
        }
    }

    pub fn sanitized(mut self) -> Result<Self, String> {
        if self.version != 1 {
            return Err(format!(
                "unsupported AudioCuePlayRequest version {}",
                self.version
            ));
        }
        self.cue.logical_path = normalize_audio_uri(&self.cue.logical_path)?;
        if self
            .position
            .is_some_and(|point| point.iter().any(|c| !c.is_finite()))
        {
            return Err("audio cue position must be finite".to_owned());
        }
        if self
            .route
            .as_deref()
            .is_some_and(|route| route.trim().is_empty())
        {
            return Err("audio cue route must not be empty".to_owned());
        }
        self.gain = sanitize_gain(self.gain);
        self.pitch = sanitize_speed(self.pitch);
        Ok(self)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AudioPlayAck {
    pub accepted: bool,
    pub provider: String,
    pub voice_id: Option<u64>,
    #[serde(default)]
    pub message: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioStopVoiceRequest {
    pub voice_id: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AudioVoiceUpdateRequest {
    pub voice_id: u64,
    #[serde(default)]
    pub gain: Option<f32>,
    #[serde(default)]
    pub speed: Option<f32>,
    #[serde(default)]
    pub paused: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AudioVoiceAck {
    pub accepted: bool,
    pub voice_id: u64,
    #[serde(default)]
    pub message: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct AudioListenerState {
    #[serde(default)]
    pub position: [f32; 3],
    #[serde(default = "default_forward")]
    pub forward: [f32; 3],
    #[serde(default = "default_up")]
    pub up: [f32; 3],
    #[serde(default)]
    pub velocity: [f32; 3],
}

impl Default for AudioListenerState {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            forward: default_forward(),
            up: default_up(),
            velocity: [0.0; 3],
        }
    }
}

impl AudioListenerState {
    pub fn sanitized(mut self) -> Self {
        self.position = sanitize_vec3(self.position);
        self.forward = normalize_or(sanitize_vec3(self.forward), default_forward());
        self.up = normalize_or(sanitize_vec3(self.up), default_up());
        self.velocity = sanitize_vec3(self.velocity);
        self
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AudioDiagnostics {
    pub provider: String,
    pub output_ready: bool,
    pub active_voices: usize,
    pub cached_clips: usize,
    pub cache_bytes: usize,
    pub cache_limit_bytes: usize,
    #[serde(default)]
    pub last_error: Option<String>,
}

pub fn sanitize_gain(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 4.0)
    } else {
        1.0
    }
}

pub fn sanitize_speed(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.05, 4.0)
    } else {
        1.0
    }
}

pub fn normalize_audio_uri(uri: &str) -> Result<String, String> {
    let mut normalized = uri.trim().replace('\\', "/");
    while let Some(rest) = normalized.strip_prefix("./") {
        normalized = rest.to_owned();
    }
    normalized = normalized.trim_start_matches('/').to_owned();
    if normalized.is_empty() {
        return Err("audio clip uri is empty".to_owned());
    }
    if normalized.split('/').any(|part| part == "..") {
        return Err(format!("audio clip uri escapes VFS root: '{normalized}'"));
    }
    Ok(normalized)
}

fn default_forward() -> [f32; 3] {
    [0.0, 0.0, -1.0]
}
fn default_up() -> [f32; 3] {
    [0.0, 1.0, 0.0]
}
fn sanitize_vec3(value: [f32; 3]) -> [f32; 3] {
    value.map(|component| {
        if component.is_finite() {
            component
        } else {
            0.0
        }
    })
}
fn normalize_or(value: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let length_sq = value
        .iter()
        .map(|component| component * component)
        .sum::<f32>();
    if !length_sq.is_finite() || length_sq <= 1.0e-10 {
        return fallback;
    }
    let inv = length_sq.sqrt().recip();
    value.map(|component| component * inv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xvag_reference_disallows_cue_dictionary_and_selectors() {
        assert_eq!(
            normalize_xvag_uri(r".\audio\impact.xvag").unwrap(),
            "audio/impact.xvag"
        );
        for invalid in [
            "audio/old.yscd",
            "audio/old.yscd@impact",
            "audio/clip.xvag@selector",
            "../audio/clip.xvag",
            "C:/audio/clip.xvag",
            "/audio/clip.xvag",
            "audio/clip.xvag?take=1",
        ] {
            assert!(normalize_xvag_uri(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn xvag_playback_serializes_optional_spatial_metadata() {
        let mut req = AudioPlayRequest::new("audio/impacts/metal.xvag");
        req.position = Some([1.0, 2.0, 3.0]);
        req.route = Some("sfx".into());
        req.scope_id = Some(71);
        req.seed = Some(12);
        let req = req.sanitized().unwrap();
        let wire = serde_json::to_value(&req).unwrap();
        assert_eq!(wire["clip"]["uri"], "audio/impacts/metal.xvag");
        assert_eq!(wire["position"], serde_json::json!([1., 2., 3.]));
        assert_eq!(wire["route"], "sfx");
        assert_eq!(wire["scope_id"], 71);
        let mut invalid = AudioPlayRequest::new("audio/a.xvag");
        invalid.position = Some([f32::INFINITY, 0., 0.]);
        assert!(invalid.sanitized().is_err());
    }

    #[test]
    fn cue_request_preserves_native_yscd_contract_and_validates_coordinates() {
        let mut request = AudioCuePlayRequest::new("./audio/vehicles/impact.yscd@metal");
        request.gain = 8.0;
        request.pitch = f32::NAN;
        request.position = Some([2.0, 0.0, 1.0]);
        request.scope_id = Some(42);
        request.seed = Some(121);
        let sanitized = request.sanitized().unwrap();
        assert_eq!(
            sanitized.cue.logical_path,
            "audio/vehicles/impact.yscd@metal"
        );
        assert_eq!(sanitized.gain, 4.0);
        assert_eq!(sanitized.pitch, 1.0);
        assert_eq!(sanitized.scope_id, Some(42));
        let encoded = serde_json::to_value(&sanitized).unwrap();
        assert_eq!(
            encoded["cue"]["logical_path"],
            "audio/vehicles/impact.yscd@metal"
        );
        assert_eq!(encoded["position"], serde_json::json!([2.0, 0.0, 1.0]));
        assert_eq!(encoded["seed"], 121);
        assert!(encoded.get("route").is_none());

        let mut invalid = AudioCuePlayRequest::new("audio/valid.yscd");
        invalid.position = Some([f32::NAN, 0.0, 0.0]);
        assert!(invalid.sanitized().is_err());
    }

    #[test]
    fn play_request_is_sanitized() {
        let request = AudioPlayRequest {
            version: 1,
            clip: AudioClipRef::new("./audio/test.wav"),
            gain: 99.0,
            speed: f32::NAN,
            looping: false,
            paused: false,
            position: None,
            route: None,
            scope_id: None,
            seed: None,
        }
        .sanitized()
        .unwrap();
        assert_eq!(request.clip.uri, "audio/test.wav");
        assert_eq!(request.gain, 4.0);
        assert_eq!(request.speed, 1.0);
    }
}
