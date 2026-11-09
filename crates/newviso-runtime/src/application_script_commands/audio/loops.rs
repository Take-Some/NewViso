use super::*;

impl EngineApplication {
    /// Named loops own one provider voice. Updating gain never restarts playback.
    pub(super) fn apply_audio_loop_command(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        use newviso_audio_client::{AudioClient, AudioPlayAck, AudioVoiceUpdateRequest};
        let key = command
            .get("key")
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty() && v.len() <= 128)
            .ok_or_else(|| format!("script command[{index}] audio.loop.set requires key"))?;
        let enabled = command
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let audio = AudioClient::new();
        if !audio.available() {
            return Ok(());
        }
        if !enabled {
            if let Some((_, id)) = self.script_audio_loops.remove(key) {
                audio.stop(id)?;
            }
            return Ok(());
        }
        let native_clip = command
            .get("clip")
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty());
        let cue = native_clip
            .or_else(|| {
                command
                    .get("cue")
                    .and_then(Value::as_str)
                    .filter(|v| !v.trim().is_empty())
            })
            .ok_or_else(|| {
                format!("script command[{index}] audio.loop.set requires clip (XVAG) or legacy cue")
            })?;
        // Validate before comparing/stopping a currently running loop.
        if native_clip.is_some() {
            newviso_audio_client::normalize_xvag_uri(cue)?;
        }
        let gain = command
            .get("gain")
            .map(|_| command_number(command, "gain", index))
            .transpose()?
            .unwrap_or(1.0)
            .clamp(0.0, 4.0);
        let pitch = command
            .get("pitch")
            .map(|_| command_number(command, "pitch", index))
            .transpose()?
            .unwrap_or(1.0)
            .clamp(0.05, 4.0);
        if let Some((playing, id)) = self.script_audio_loops.get(key).cloned() {
            if playing == cue {
                let ack = audio.update_voice(&AudioVoiceUpdateRequest {
                    voice_id: id,
                    gain: Some(gain),
                    speed: Some(pitch),
                    paused: Some(false),
                })?;
                if ack.accepted {
                    return Ok(());
                }
            }
            let _ = audio.stop(id);
            self.script_audio_loops.remove(key);
        }
        let ack: AudioPlayAck = if native_clip.is_some() {
            let mut request = newviso_audio_client::AudioPlayRequest::new(cue);
            request.looping = true;
            request.gain = gain;
            request.speed = pitch;
            request.position = command
                .get("position")
                .map(|_| command_vec3(command, "position", index))
                .transpose()?;
            request.route = command
                .get("route")
                .and_then(Value::as_str)
                .map(str::to_owned);
            audio.play_xvag(&request)?
        } else {
            // Transitional compatibility: already-authored YSCD projects.
            let address = AssetAddress::parse(cue)?;
            let info =
                newviso_assets_client::AssetClient::new().resolve_type(address.logical_path())?;
            if info
                .get("descriptor")
                .and_then(|d| d.get("asset_kind"))
                .and_then(Value::as_str)
                != Some("sound_cue_dictionary")
            {
                return Err(format!("script command[{index}] audio loop requires an XVAG clip or valid legacy YSCD cue '{cue}'"));
            }
            let mut request =
                json!({"version":1,"cue":{"logical_path":cue},"gain":gain,"pitch":pitch});
            if command.get("position").is_some() {
                request["position"] = json!(command_vec3(command, "position", index)?);
            }
            let encoded = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
            let reply = host::call_service("engine.audio", "play_cue_json_v1", &encoded)?;
            serde_json::from_slice(&reply).map_err(|e| e.to_string())?
        };
        if !ack.accepted {
            return Err(format!(
                "script command[{index}] audio loop rejected: {}",
                ack.message
            ));
        }
        let id = ack
            .voice_id
            .ok_or_else(|| format!("script command[{index}] audio loop returned no voice id"))?;
        self.script_audio_loops
            .insert(key.to_owned(), (cue.to_owned(), id));
        Ok(())
    }

    pub(crate) fn stop_script_audio_loops(&mut self) {
        let audio = newviso_audio_client::AudioClient::new();
        for (_, (_, id)) in std::mem::take(&mut self.script_audio_loops) {
            let _ = audio.stop(id);
        }
    }
}
