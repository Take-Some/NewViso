mod characters;
mod loops;
mod physics;

use super::*;
use physics::parse_physics_audio_material;

impl EngineApplication {
    pub(super) fn apply_audio_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
            "audio.endorphin.bind" | "audio.endorphin.unbind" => {
                let region = command
                    .get("region")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("script command[{index}] {op} requires region"))?;
                let material_name = command
                    .get("material")
                    .and_then(Value::as_str)
                    .unwrap_or("generic");
                let material = parse_physics_audio_material(material_name).ok_or_else(|| {
                    format!("script command[{index}] unknown material '{material_name}'")
                })?;
                if op == "audio.endorphin.bind" {
                    if let Some(clips) = command.get("clips") {
                        let clips = clips.as_array()
                            .ok_or_else(|| format!("script command[{index}] audio.endorphin.bind clips must be an array"))?
                            .iter().map(|item| item.as_str()
                                .map(str::to_owned)
                                .ok_or_else(|| format!("script command[{index}] Endorphin clip must be a string")))
                            .collect::<Result<Vec<_>, _>>()?;
                        self.endorphin_audio
                            .bind_variants(region, material, &clips)?;
                    } else {
                        let clip = command.get("clip").and_then(Value::as_str)
                            .ok_or_else(|| format!("script command[{index}] audio.endorphin.bind requires clip or clips"))?;
                        self.endorphin_audio.bind(region, material, clip)?;
                    }
                } else {
                    self.endorphin_audio.unbind(region, material)?;
                }
            }
            "audio.physics.bind" => {
                let material =
                    command
                        .get("material")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!("script command[{index}] audio.physics.bind requires material")
                        })?;
                let material = parse_physics_audio_material(material).ok_or_else(|| {
                    format!("script command[{index}] unsupported acoustic material '{material}'")
                })?;
                let clip = command.get("clip").and_then(Value::as_str).ok_or_else(|| {
                    format!("script command[{index}] audio.physics.bind requires clip")
                })?;
                let clip = newviso_audio_client::normalize_xvag_uri(clip)?;
                self.physics_audio_clips.insert(material, clip);
                self.physics_audio_router.clear();
            }
            "audio.physics.unbind" => {
                let material =
                    command
                        .get("material")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                                "script command[{index}] audio.physics.unbind requires material"
                            )
                        })?;
                let material = parse_physics_audio_material(material).ok_or_else(|| {
                    format!("script command[{index}] unsupported acoustic material '{material}'")
                })?;
                self.physics_audio_clips.remove(&material);
                self.physics_audio_router.clear();
            }
            // Native XVAG is the primary path. Sound-event selection belongs
            // in the engine; the audio backend receives an actual clip URI.
            "audio.clip.preload" => {
                let clip = command.get("clip").and_then(Value::as_str).ok_or_else(|| {
                    format!("script command[{index}] audio.clip.preload requires clip")
                })?;
                let clip = newviso_audio_client::normalize_xvag_uri(clip)?;
                invoke_audio_service(
                    index,
                    newviso_audio_client::AUDIO_METHOD_PRELOAD_CLIP_JSON_V1,
                    &json!({"clip":{"uri":clip}}),
                )?;
            }
            "audio.clip.play" => {
                let clip = command.get("clip").and_then(Value::as_str).ok_or_else(|| {
                    format!("script command[{index}] audio.clip.play requires clip")
                })?;
                let clip = newviso_audio_client::normalize_xvag_uri(clip)?;
                let mut request = newviso_audio_client::AudioPlayRequest::new(clip);
                request.gain = command
                    .get("gain")
                    .map(|_| command_number(command, "gain", index))
                    .transpose()?
                    .unwrap_or(1.0);
                request.speed = command
                    .get("pitch")
                    .map(|_| command_number(command, "pitch", index))
                    .transpose()?
                    .unwrap_or(1.0);
                request.looping = command
                    .get("looping")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                request.paused = command
                    .get("paused")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                request.position = command
                    .get("position")
                    .map(|_| command_vec3(command, "position", index))
                    .transpose()?;
                request.route = command
                    .get("route")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                request.scope_id = command.get("scope_id").and_then(Value::as_u64);
                request.seed = command.get("seed").and_then(Value::as_u64);
                let mut request = request.sanitized()?;
                request.clip.uri = newviso_audio_client::normalize_xvag_uri(&request.clip.uri)?;
                let encoded = serde_json::to_value(&request).map_err(|e| e.to_string())?;
                invoke_audio_service(
                    index,
                    newviso_audio_client::AUDIO_METHOD_PLAY_CLIP_JSON_V1,
                    &encoded,
                )?;
            }
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
