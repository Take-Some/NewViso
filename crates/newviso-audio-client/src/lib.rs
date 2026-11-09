pub use newviso_audio_api::*;

use newviso_host as host;
use serde::{de::DeserializeOwned, Serialize};

#[derive(Clone, Copy, Debug, Default)]
pub struct AudioClient;

impl AudioClient {
    pub const fn new() -> Self {
        Self
    }

    pub fn info(&self) -> Result<AudioServiceInfo, String> {
        self.get(AUDIO_METHOD_INFO_JSON)
    }

    pub fn available(&self) -> bool {
        self.info().is_ok()
    }

    pub fn preload(&self, request: &AudioPreloadRequest) -> Result<AudioPreloadAck, String> {
        self.post(AUDIO_METHOD_PRELOAD_CLIP_JSON_V1, request)
    }

    pub fn play(&self, request: &AudioPlayRequest) -> Result<AudioPlayAck, String> {
        self.post(AUDIO_METHOD_PLAY_CLIP_JSON_V1, request)
    }

    pub fn play_uri(&self, uri: impl Into<String>) -> Result<AudioPlayAck, String> {
        self.play(&AudioPlayRequest::new(uri))
    }

    /// First-class XVAG playback through the existing low-level clip method.
    /// Additional spatial/route metadata is passed through to the provider.
    /// Device-side support must be verified against the installed backend.
    pub fn play_xvag(&self, request: &AudioPlayRequest) -> Result<AudioPlayAck, String> {
        let mut request = request.clone();
        request.clip.uri = normalize_xvag_uri(&request.clip.uri)?;
        self.play(&request.sanitized()?)
    }

    pub fn preload_xvag(&self, uri: &str) -> Result<AudioPreloadAck, String> {
        let clip = normalize_xvag_uri(uri)?;
        self.preload(&AudioPreloadRequest::new(clip))
    }

    /// Play a final YSCD cue through the existing engine.audio provider.
    pub fn play_cue(&self, request: &AudioCuePlayRequest) -> Result<AudioPlayAck, String> {
        let request = request.clone().sanitized()?;
        self.post(AUDIO_METHOD_PLAY_CUE_JSON_V1, &request)
    }

    pub fn stop(&self, voice_id: u64) -> Result<AudioVoiceAck, String> {
        self.post(
            AUDIO_METHOD_STOP_VOICE_JSON_V1,
            &AudioStopVoiceRequest { voice_id },
        )
    }

    pub fn update_voice(&self, request: &AudioVoiceUpdateRequest) -> Result<AudioVoiceAck, String> {
        self.post(AUDIO_METHOD_SET_VOICE_JSON_V1, request)
    }

    pub fn set_listener(&self, listener: AudioListenerState) -> Result<AudioListenerState, String> {
        self.post(AUDIO_METHOD_SET_LISTENER_JSON_V1, &listener)
    }

    pub fn diagnostics(&self) -> Result<AudioDiagnostics, String> {
        self.get(AUDIO_METHOD_DIAGNOSTICS_JSON_V1)
    }

    fn get<T: DeserializeOwned>(&self, method: &str) -> Result<T, String> {
        let bytes = host::call_service(ENGINE_AUDIO_SERVICE_ID, method, &[])?;
        serde_json::from_slice(&bytes).map_err(|error| {
            format!("audio service method '{method}' returned invalid JSON: {error}")
        })
    }

    fn post<T: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        request: &T,
    ) -> Result<R, String> {
        let bytes = serde_json::to_vec(request).map_err(|error| error.to_string())?;
        let response = host::call_service(ENGINE_AUDIO_SERVICE_ID, method, &bytes)?;
        serde_json::from_slice(&response).map_err(|error| {
            format!("audio service method '{method}' returned invalid JSON: {error}")
        })
    }
}
