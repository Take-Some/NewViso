use super::*;

// Synchronized locomotion audio is engine-owned. Project controllers supply
// authoritative physics samples, never their own footstep timers.
impl EngineApplication {
    pub(crate) fn apply_character_audio_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        if op == "character.audio.reset" {
            let actor_id = command
                .get("actor_id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!("script command[{index}] character.audio.reset requires actor_id")
                })?;
            self.character_motion_samples.remove(actor_id);
            self.character_audio.remove(actor_id);
            return Ok(());
        }

        use application_character_audio::{CharacterMotionSample, Gait};
        let text = |field: &str| {
            command
                .get(field)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    format!("script command[{index}] character.audio.motion requires {field}")
                })
        };
        let actor_id = text("actor_id")?;
        let scene_id = text("scene_id")?;
        let gait = Gait::parse(text("gait")?)
            .ok_or_else(|| format!("script command[{index}] invalid locomotion gait"))?;
        let surface = command
            .get("surface")
            .and_then(Value::as_str)
            .unwrap_or("stone");
        if !matches!(
            surface,
            "dirt" | "grass" | "metal" | "mud" | "snow" | "stone" | "water" | "wood"
        ) {
            return Err(format!(
                "script command[{index}] invalid character audio surface '{surface}'"
            ));
        }
        // Stone has an actual first-class XVAG library in Shared/Content;
        // surfaces not yet exported retain their old authored YSCD bank.
        let default_source = if surface == "stone" {
            "audio/footsteps"
        } else {
            "audio/footsteps/footsteps.yscd"
        };
        let dictionary = command
            .get("dictionary")
            .and_then(Value::as_str)
            .unwrap_or(default_source);
        if dictionary.trim().is_empty()
            || dictionary.contains('@')
            || dictionary.contains("..")
            || dictionary.contains(':')
            || dictionary.contains('\\')
        {
            return Err(format!("script command[{index}] invalid sound source"));
        }
        let grounded = command
            .get("grounded")
            .and_then(Value::as_bool)
            .ok_or_else(|| format!("script command[{index}] requires grounded boolean"))?;
        let speed = command_number(command, "speed", index)?;
        let delta_seconds = command_number(command, "delta_seconds", index)?;
        let impact_speed = command
            .get("impact_speed")
            .map(|_| command_number(command, "impact_speed", index))
            .transpose()?
            .unwrap_or(0.0);
        if !(0.0..=150.0).contains(&speed) || !(0.0..=0.25).contains(&delta_seconds) {
            return Err(format!(
                "script command[{index}] invalid motion speed or delta_seconds"
            ));
        }
        // Preload the bank before movement's first contact: audio startup and
        // first footstep must not contend for the same streamed cue.
        let bank_key = format!("bank:{dictionary}:{surface}");
        if ((grounded && speed > 0.12)
            || command.get("jumped").and_then(Value::as_bool) == Some(true))
            && !self.character_audio_preloaded.contains(&bank_key)
        {
            for action in ["walk", "run", "sprint", "land", "lift"] {
                if action == "lift"
                    && !matches!(surface, "dirt" | "metal" | "stone" | "water" | "wood")
                {
                    continue;
                }
                let xvag = !dictionary.to_ascii_lowercase().ends_with(".yscd");
                let variations = if xvag {
                    if action == "land" {
                        5u64
                    } else {
                        8u64
                    }
                } else {
                    1u64
                };
                for variant in 0..variations {
                    let asset = application_character_audio::sound_asset(
                        dictionary, surface, action, variant,
                    );
                    if self.character_audio_preloaded.contains(&asset) {
                        continue;
                    }
                    if xvag {
                        let clip_uri = newviso_audio_client::normalize_xvag_uri(&asset)?;
                        invoke_audio_service(
                            index,
                            newviso_audio_client::AUDIO_METHOD_PRELOAD_CLIP_JSON_V1,
                            &json!({"clip":{"uri":clip_uri}}),
                        )?;
                    } else {
                        invoke_audio_service(
                            index,
                            "preload_cue_json_v1",
                            &json!({"cue":{"logical_path":asset}}),
                        )?;
                    }
                    self.character_audio_preloaded.insert(asset);
                }
            }
            self.character_audio_preloaded.insert(bank_key);

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

        self.character_motion_samples.insert(
            actor_id.to_owned(),
            CharacterMotionSample {
                actor_id: actor_id.to_owned(),
                scene_id: scene_id.to_owned(),
                position: command_vec3(command, "position", index)?,
                grounded,
                jumped: command
                    .get("jumped")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                landed: command
                    .get("landed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                gait,
                surface: surface.to_owned(),
                dictionary: dictionary.to_owned(),
                speed,
                delta_seconds,
                impact_speed,
            },
        );
        Ok(())
    }

    /// Runs after the scene animation tick, so each sound is tied to the same
    /// sampled pose that will be rendered this frame.
    pub(crate) fn flush_character_motion_audio(&mut self) -> Result<(), String> {
        use application_character_audio::{ClipPhase, SoundKind};
        let samples = std::mem::take(&mut self.character_motion_samples);
        self.character_audio
            .retain_actors(|id| samples.contains_key(id));
        for (_, sample) in samples {
            let stable_id = self.scene.runtime_entity_stable_id(&sample.scene_id);
            let clip = stable_id
                .and_then(|id| self.scene.entity_animation_playback(id))
                .and_then(|v| {
                    Some(ClipPhase {
                        name: v.get("clip")?.as_str()?.to_owned(),
                        time: v.get("time_seconds")?.as_f64()? as f32,
                        duration: v.get("duration_seconds")?.as_f64()? as f32,
                        feet: stable_id
                            .and_then(|id| self.scene.entity_animation_feet_positions(id)),
                    })
                });

            for sound in self.character_audio.sample(&sample, clip.as_ref()) {
                if sound.kind == SoundKind::Footstep {
                    host::publish_event_json(
                        "character.footstep",
                        "newviso.character",
                        json!({
                            "actor_id":sample.actor_id,"entity":sample.scene_id,
                            "position":sound.position,"surface":sample.surface,
                            "locomotion":sample.gait.action(),"action":sample.gait.action(),
                            "horizontal_speed":sample.speed,"grounded":sample.grounded,
                            "step_index":sound.seed,"contact_source":sound.source,"audio_handled":true
                        }),
                    )?;
                }

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
                if sound.cue.to_ascii_lowercase().ends_with(".xvag") {
                    let mut request = newviso_audio_client::AudioPlayRequest::new(&sound.cue);
                    request.gain = sound.gain;
                    request.speed = pitch;
                    request.route = Some("sfx".to_owned());
                    request.position = Some(sound.position);
                    request.seed = Some(sound.seed);
                    request.scope_id = stable_id;
                    let mut request = request.sanitized()?;
                    request.clip.uri = newviso_audio_client::normalize_xvag_uri(&request.clip.uri)?;
                    invoke_audio_service(
                        0,
                        newviso_audio_client::AUDIO_METHOD_PLAY_CLIP_JSON_V1,
                        &serde_json::to_value(&request).map_err(|error| error.to_string())?,
                    )?;
                } else {
                    // Existing YSCD surfaces remain playable until their XVAG
                    // samples are authored and migrated to Shared/Content.
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
        }
        Ok(())
    }
}
