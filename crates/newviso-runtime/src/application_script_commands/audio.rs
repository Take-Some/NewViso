use super::*;

impl EngineApplication {
    pub(super) fn apply_audio_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
            "audio.route.gain" => {
                let route = command
                    .get("route")
                    .and_then(Value::as_str)
                    .filter(|route| !route.trim().is_empty())
                    .ok_or_else(|| {
                        format!("script command[{index}] audio.route.gain requires 'route'")
                    })?;
                let gain = command_number(command, "gain", index)?.clamp(0.0, 4.0);
                invoke_audio_service(
                    index,
                    "set_route_gain_json_v1",
                    &json!({"route": route, "gain": gain}),
                )?;
            }
            "audio.cue.preload" => {
                let cue = command
                    .get("cue")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        format!("script command[{index}] audio.cue.preload requires string 'cue'")
                    })?;
                let request = json!({
                    "cue": { "logical_path": cue }
                });
                invoke_audio_service(index, "preload_cue_json_v1", &request)?;
            }
            "audio.cue.play" => {
                let cue = command
                    .get("cue")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        format!("script command[{index}] audio.cue.play requires string 'cue'")
                    })?;
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
                let mut request = json!({
                    "version": 1,
                    "cue": { "logical_path": cue },
                    "gain": gain,
                    "pitch": pitch
                });
                if command.get("position").is_some() {
                    request["position"] = json!(command_vec3(command, "position", index)?);
                }
                if let Some(route) = command.get("route").and_then(Value::as_str) {
                    request["route"] = Value::String(route.to_owned());
                }
                if let Some(seed) = command.get("seed").and_then(Value::as_u64) {
                    request["seed"] = json!(seed);
                }
                if let Some(scope_id) = command.get("scope_id").and_then(Value::as_u64) {
                    request["scope_id"] = json!(scope_id);
                }
                invoke_audio_service(index, "play_cue_json_v1", &request)?;
            }

            "audio.loop.set" => {
                self.apply_audio_loop_command(command, index)?;
            }
            "audio.listener.set" => {
                let request = json!({
                    "position": command_vec3(command, "position", index)?,
                    "forward": command
                        .get("forward")
                        .map(|_| command_vec3(command, "forward", index))
                        .transpose()?
                        .unwrap_or([0.0, 0.0, -1.0]),
                    "up": command
                        .get("up")
                        .map(|_| command_vec3(command, "up", index))
                        .transpose()?
                        .unwrap_or([0.0, 1.0, 0.0]),
                    "velocity": command
                        .get("velocity")
                        .map(|_| command_vec3(command, "velocity", index))
                        .transpose()?
                        .unwrap_or([0.0, 0.0, 0.0])
                });
                invoke_audio_service(index, "set_listener_json_v1", &request)?;
            }
            _ => return Err(unsupported_command(op, index)),
        }
        Ok(())
    }
}

pub(super) fn sync_audio_listener_if_available(
    index: usize,
    request: &Value,
) -> Result<(), String> {
    let encoded = serde_json::to_vec(request).map_err(|error| {
        format!("script command[{index}] audio listener encode failed: {error}")
    })?;
    match host::call_service("engine.audio", "set_listener_json_v1", &encoded) {
        Ok(_) => Ok(()),
        Err(error) if error.contains("is not registered") => Ok(()),
        Err(error) => Err(format!(
            "script command[{index}] default engine.audio listener sync failed: {error}"
        )),
    }
}

pub(super) fn invoke_audio_service(
    index: usize,
    method: &str,
    request: &Value,
) -> Result<(), String> {
    let encoded = serde_json::to_vec(request)
        .map_err(|error| format!("script command[{index}] audio request encode failed: {error}"))?;
    match host::call_service("engine.audio", method, &encoded) {
        Ok(reply) => {
            if let Ok(ack) = serde_json::from_slice::<Value>(&reply) {
                if ack.get("accepted").and_then(Value::as_bool) == Some(false) {
                    return Err(format!(
                        "script command[{index}] engine.audio method '{method}' rejected: {}",
                        ack.get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("request rejected")
                    ));
                }
            }
            Ok(())
        }
        Err(error) if error.contains("is not registered") => {
            host::warn(
                "newviso.audio",
                format!(
                    "script command[{index}] skipped because engine.audio is unavailable method='{method}'"
                ),
            );
            Ok(())
        }
        Err(error) => Err(format!(
            "script command[{index}] engine.audio method '{method}' failed: {error}"
        )),
    }
}


impl EngineApplication {
    /// Named loops own one provider voice. Updating gain never restarts playback.
    pub(super) fn apply_audio_loop_command(&mut self, command: &Value, index: usize) -> Result<(), String> {
        use newviso_audio_client::{AudioClient, AudioPlayAck, AudioVoiceUpdateRequest};
        let key = command.get("key").and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty() && v.len() <= 128)
            .ok_or_else(|| format!("script command[{index}] audio.loop.set requires key"))?;
        let enabled = command.get("enabled").and_then(Value::as_bool).unwrap_or(true);
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
        let cue = command.get("cue").and_then(Value::as_str).filter(|v| !v.trim().is_empty())
            .ok_or_else(|| format!("script command[{index}] audio.loop.set requires cue"))?;
        let gain = command.get("gain").map(|_| command_number(command, "gain", index))
            .transpose()?.unwrap_or(1.0).clamp(0.0, 4.0);
        let pitch = command.get("pitch").map(|_| command_number(command, "pitch", index))
            .transpose()?.unwrap_or(1.0).clamp(0.05, 4.0);
        if let Some((playing, id)) = self.script_audio_loops.get(key).cloned() {
            if playing == cue {
                let ack = audio.update_voice(&AudioVoiceUpdateRequest {
                    voice_id:id,gain:Some(gain),speed:Some(pitch),paused:Some(false)
                })?;
                if ack.accepted { return Ok(()); }
            }
            let _ = audio.stop(id);
            self.script_audio_loops.remove(key);
        }
        let address = AssetAddress::parse(cue)?;
        let info = newviso_assets_client::AssetClient::new().resolve_type(address.logical_path())?;
        if info.get("descriptor").and_then(|d| d.get("asset_kind")).and_then(Value::as_str)
            != Some("sound_cue_dictionary") {
            return Err(format!("script command[{index}] audio loop requires a final YSCD cue '{cue}'"));
        }
        let mut request=json!({"version":1,"cue":{"logical_path":cue},"gain":gain,"pitch":pitch});
        if command.get("position").is_some() {
            request["position"]=json!(command_vec3(command,"position",index)?);
        }
        let encoded=serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        let reply=host::call_service("engine.audio","play_cue_json_v1",&encoded)?;
        let ack:AudioPlayAck=serde_json::from_slice(&reply).map_err(|e| e.to_string())?;
        if !ack.accepted {
            return Err(format!("script command[{index}] audio loop rejected: {}",ack.message));
        }
        let id=ack.voice_id.ok_or_else(|| format!("script command[{index}] audio loop returned no voice id"))?;
        self.script_audio_loops.insert(key.to_owned(),(cue.to_owned(),id));
        Ok(())
    }

    pub(crate) fn stop_script_audio_loops(&mut self) {
        let audio=newviso_audio_client::AudioClient::new();
        for (_, (_, id)) in std::mem::take(&mut self.script_audio_loops) {
            let _=audio.stop(id);
        }
    }
}

// Synchronized locomotion audio is engine-owned. Project controllers supply
// authoritative physics samples, never their own footstep timers.
impl EngineApplication {
    pub(super) fn apply_character_audio_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        if op == "character.audio.reset" {
            let actor_id = command.get("actor_id").and_then(Value::as_str)
                .ok_or_else(|| format!("script command[{index}] character.audio.reset requires actor_id"))?;
            self.character_motion_samples.remove(actor_id);
            self.character_audio.remove(actor_id);
            return Ok(());
        }

        use application_character_audio::{CharacterMotionSample, Gait};
        let text = |field: &str| command.get(field).and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("script command[{index}] character.audio.motion requires {field}"));
        let actor_id = text("actor_id")?;
        let scene_id = text("scene_id")?;
        let gait = Gait::parse(text("gait")?)
            .ok_or_else(|| format!("script command[{index}] invalid locomotion gait"))?;
        let surface = command.get("surface").and_then(Value::as_str).unwrap_or("stone");
        if !matches!(surface, "dirt" | "grass" | "metal" | "mud" | "snow" | "stone" | "water" | "wood") {
            return Err(format!("script command[{index}] invalid character audio surface '{surface}'"));
        }
        let dictionary = command.get("dictionary").and_then(Value::as_str)
            .unwrap_or("audio/footsteps/footsteps.yscd");
        if dictionary.trim().is_empty() || dictionary.contains('@') {
            return Err(format!("script command[{index}] invalid sound dictionary"));
        }
        let grounded = command.get("grounded").and_then(Value::as_bool)
            .ok_or_else(|| format!("script command[{index}] requires grounded boolean"))?;
        let speed = command_number(command, "speed", index)?;
        let delta_seconds = command_number(command, "delta_seconds", index)?;
        let impact_speed = command.get("impact_speed")
            .map(|_| command_number(command, "impact_speed", index))
            .transpose()?.unwrap_or(0.0);
        if !(0.0..=150.0).contains(&speed)
            || !(0.0..=0.25).contains(&delta_seconds)
        {
            return Err(format!("script command[{index}] invalid motion speed or delta_seconds"));
        }
        // Preload the bank before movement's first contact: audio startup and
        // first footstep must not contend for the same streamed cue.
        if (grounded && speed > 0.12) || command.get("jumped").and_then(Value::as_bool) == Some(true) {
        for action in ["walk", "run", "sprint", "land", "lift"] {
            if action == "lift"
                && !matches!(surface, "dirt" | "metal" | "stone" | "water" | "wood") {
                continue;
            }
            let cue = format!("{dictionary}@{surface}_{action}");
            if self.character_audio_preloaded.insert(cue.clone()) {
                invoke_audio_service(index, "preload_cue_json_v1",
                    &json!({"cue": {"logical_path": cue}}))?;
            }
        }

        // Audio follows the animation's authored playback clock. Never retime
        // the visible clip from an estimated stride: this caused overcranked
        // locomotion and altered jump/landing transition fidelity.
        }
        if std::env::var_os("NEWVISO_AUDIO_CONTACT_DIAG").is_some() && speed > 0.12 {
            host::info(
                "newviso.character.audio",
                format!("motion actor={actor_id} scene={scene_id} gait={gait:?} speed={speed:.2} grounded={grounded}"),
            );
        }

        // One clock drives both the visible gait and audio contact cadence.
        // Retime the already-bound animation in place before scene.tick(dt).
        // Clamp is intrinsic to Gait::playback_rate (0.70..1.30), so a legacy
        // clip cannot suddenly play at 4x speed.
        if grounded {
            if let Some(stable_id) = self.scene.runtime_entity_stable_id(scene_id) {
                if let Some(clip) = self.scene.entity_animation_playback(stable_id) {
                    let name = clip.get("clip").and_then(Value::as_str).unwrap_or("");
                    let duration = clip.get("duration_seconds")
                        .and_then(Value::as_f64).unwrap_or(0.0) as f32;
                    let looping = clip.get("looping").and_then(Value::as_bool).unwrap_or(false);
                    if looping && application_character_audio::clip_has_gait(&name.to_lowercase(), gait) {
                        if let Some(rate) = gait.playback_rate(speed, duration) {
                            let _ = self.scene.set_entity_locomotion_rate(stable_id, rate);
                        }
                    }
                }
            }
        }

        self.character_motion_samples.insert(actor_id.to_owned(), CharacterMotionSample {
            actor_id: actor_id.to_owned(),
            scene_id: scene_id.to_owned(),
            position: command_vec3(command, "position", index)?,
            grounded,
            jumped: command.get("jumped").and_then(Value::as_bool).unwrap_or(false),
            landed: command.get("landed").and_then(Value::as_bool).unwrap_or(false),
            gait,
            surface: surface.to_owned(),
            dictionary: dictionary.to_owned(),
            speed,
            delta_seconds,
            impact_speed,
        });
        Ok(())
    }

    /// Runs after the scene animation tick, so each sound is tied to the same
    /// sampled pose that will be rendered this frame.
    pub(crate) fn flush_character_motion_audio(&mut self) -> Result<(), String> {
        use application_character_audio::{ClipPhase, SoundKind};
        let samples = std::mem::take(&mut self.character_motion_samples);
        for (_, sample) in samples {
            let stable_id = self.scene.runtime_entity_stable_id(&sample.scene_id);
            let clip = stable_id.and_then(|id| self.scene.entity_animation_playback(id))
                .and_then(|v| Some(ClipPhase {
                    name: v.get("clip")?.as_str()?.to_owned(),
                    time: v.get("time_seconds")?.as_f64()? as f32,
                    duration: v.get("duration_seconds")?.as_f64()? as f32,
                    feet: stable_id.and_then(|id| self.scene.entity_animation_feet_positions(id)),
                }));

            for sound in self.character_audio.sample(&sample, clip.as_ref()) {
                let pitch = match sound.kind {
                    SoundKind::Lift => 1.02,
                    _ => 1.0,
                };
                let contact_log = format!(
                    "actor={} gait={:?} speed={:.2} event={:?} source={} cue={} contact={} position={:?}",
                    sample.actor_id, sample.gait, sample.speed, sound.kind, sound.source,
                    sound.cue, sound.seed, sound.position,
                );
                if std::env::var_os("NEWVISO_AUDIO_CONTACT_DIAG").is_some() {
                    host::info("newviso.character.audio", contact_log);
                } else {
                    host::trace("newviso.character.audio", contact_log);
                }
                let mut request = json!({
                    "version": 1,
                    "cue": { "logical_path": sound.cue },
                    "gain": sound.gain,
                    "pitch": pitch,
                    "route": "sfx",
                    "position": sound.position,
                    "seed": sound.seed,
                });
                if let Some(id) = stable_id {
                    request["scope_id"] = json!(id);
                }
                invoke_audio_service(0, "play_cue_json_v1", &request)?;
            }
        }
        Ok(())
    }
}
