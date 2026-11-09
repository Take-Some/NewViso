mod configure;
mod events;
mod runtime;

use super::*;

impl VehicleAudioFxControls {
    pub(super) fn from_runtime_config(config: Option<&Value>) -> Self {
        let mut controls = Self::default();
        let Some(config) = config else {
            return controls;
        };
        macro_rules! reference {
            ($field:ident) => {
                controls.$field = config
                    .get(stringify!($field))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned);
            };
        }
        reference!(engine_clip);
        reference!(engine_start_clip);
        reference!(engine_shutdown_clip);
        reference!(engine_breakdown_clip);
        reference!(engine_load_clip);
        reference!(engine_high_clip);
        reference!(exhaust_clip);
        reference!(gear_shift_clip);
        reference!(road_clip);
        reference!(brake_disc_clip);
        reference!(brake_release_clip);
        reference!(handbrake_clip);
        reference!(tyre_puncture_clip);
        reference!(tyre_burst_clip);
        reference!(flat_tyre_clip);
        reference!(wheel_rim_clip);
        reference!(suspension_impact_clip);
        reference!(door_open_clip);
        reference!(door_close_clip);
        reference!(door_locked_attempt_clip);
        reference!(damage_oneshot_clip);
        reference!(tyre_skid_clip);
        reference!(steering_scrub_clip);
        reference!(brake_chirp_clip);
        reference!(suspension_clatter_clip);
        reference!(impact_clip);
        reference!(impact_heavy_clip);
        reference!(glass_break_clip);
        reference!(horn_clip);
        reference!(siren_clip);
        reference!(exhaust_effect);
        reference!(tyre_effect);
        reference!(tyre_puncture_effect);
        reference!(tyre_burst_effect);
        reference!(engine_smoke_effect);
        reference!(engine_fire_effect);
        reference!(oil_leak_effect);
        reference!(petrol_leak_effect);
        reference!(petrol_fire_effect);
        reference!(misfire_effect);
        reference!(part_fire_effect);
        reference!(explosion_effect);
        reference!(impact_effect);
        reference!(glass_effect);
        for name in [
            "debris_effect",
            "part_break_effect",
            "scrape_effect",
            "bullet_impact_effect",
            "side_window_effect",
            "windscreen_effect",
            "engine_open_smoke_effect",
            "wreck_fire_effect",
            "explosion_secondary_effect",
            "explosion_post_effect",
        ] {
            if let Some(reference) = config
                .get(name)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|v| !v.is_empty())
            {
                controls
                    .damage_effects
                    .insert(name.to_owned(), reference.to_owned());
            }
        }
        controls.debris_lifetime_seconds = config
            .get("debris_lifetime_seconds")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .map(|v| v.clamp(0.0, 600.0) as f32)
            .unwrap_or(0.0);
        controls.max_debris = config
            .get("max_debris")
            .and_then(Value::as_u64)
            .map(|v| v.clamp(1, 256) as usize)
            .unwrap_or(48);
        controls
    }

    pub(super) fn stop_all_voices(&mut self, audio: &AudioClient) {
        for voice in [
            self.engine_voice.take(),
            self.engine_load_voice.take(),
            self.engine_high_voice.take(),
            self.exhaust_voice.take(),
            self.road_voice.take(),
            self.brake_disc_voice.take(),
            self.flat_tyre_voice.take(),
            self.wheel_rim_voice.take(),
            self.tyre_voice.take(),
            self.steering_voice.take(),
            self.siren_voice.take(),
            self.horn_voice.take(),
            self.radio_voice.take(),
        ]
        .into_iter()
        .flatten()
        {
            let _ = audio.stop(voice);
        }
    }
}

pub(super) fn vehicle_audio_fx_runtime_state(audio: &VehicleAudioFxControls) -> Value {
    let mut out = serde_json::Map::new();
    macro_rules! field {
        ($name:literal, $value:expr) => {
            out.insert(
                $name.to_owned(),
                serde_json::to_value($value).expect("vehicle audio diagnostics must serialize"),
            );
        };
    }

    // Preserve the original flat diagnostic API used by project scripts while
    // exposing every newly ported reference event/loop cue.
    field!("engine_clip", &audio.engine_clip);
    field!("engine_start_clip", &audio.engine_start_clip);
    field!("engine_shutdown_clip", &audio.engine_shutdown_clip);
    field!("engine_breakdown_clip", &audio.engine_breakdown_clip);
    field!("engine_load_clip", &audio.engine_load_clip);
    field!("engine_high_clip", &audio.engine_high_clip);
    field!("exhaust_clip", &audio.exhaust_clip);
    field!("gear_shift_clip", &audio.gear_shift_clip);
    field!("road_clip", &audio.road_clip);
    field!("brake_disc_clip", &audio.brake_disc_clip);
    field!("brake_release_clip", &audio.brake_release_clip);
    field!("handbrake_clip", &audio.handbrake_clip);
    field!("tyre_puncture_clip", &audio.tyre_puncture_clip);
    field!("tyre_burst_clip", &audio.tyre_burst_clip);
    field!("flat_tyre_clip", &audio.flat_tyre_clip);
    field!("wheel_rim_clip", &audio.wheel_rim_clip);
    field!("tyre_skid_clip", &audio.tyre_skid_clip);
    field!("steering_scrub_clip", &audio.steering_scrub_clip);
    field!("suspension_impact_clip", &audio.suspension_impact_clip);
    field!("door_open_clip", &audio.door_open_clip);
    field!("door_close_clip", &audio.door_close_clip);
    field!("door_locked_attempt_clip", &audio.door_locked_attempt_clip);
    field!("damage_oneshot_clip", &audio.damage_oneshot_clip);
    field!("brake_chirp_clip", &audio.brake_chirp_clip);
    field!("suspension_clatter_clip", &audio.suspension_clatter_clip);
    field!("impact_clip", &audio.impact_clip);
    field!("impact_heavy_clip", &audio.impact_heavy_clip);
    field!("glass_break_clip", &audio.glass_break_clip);
    field!("siren_clip", &audio.siren_clip);
    field!("horn_clip", &audio.horn_clip);
    field!("radio_clip", &audio.radio_clip);

    field!("engine_voice", audio.engine_voice);
    field!("engine_load_voice", audio.engine_load_voice);
    field!("engine_high_voice", audio.engine_high_voice);
    field!("exhaust_voice", audio.exhaust_voice);
    field!("road_voice", audio.road_voice);
    field!("brake_disc_voice", audio.brake_disc_voice);
    field!("flat_tyre_voice", audio.flat_tyre_voice);
    field!("wheel_rim_voice", audio.wheel_rim_voice);
    field!("tyre_voice", audio.tyre_voice);
    field!("steering_voice", audio.steering_voice);
    field!("siren_voice", audio.siren_voice);
    field!("horn_voice", audio.horn_voice);
    field!("radio_voice", audio.radio_voice);

    field!("audio_events_emitted", audio.audio_events_emitted);
    field!("exhaust_effect", &audio.exhaust_effect);
    field!("impact_effect", &audio.impact_effect);
    field!("tyre_effect", &audio.tyre_effect);
    field!("tyre_puncture_effect", &audio.tyre_puncture_effect);
    field!("tyre_burst_effect", &audio.tyre_burst_effect);
    field!("engine_smoke_effect", &audio.engine_smoke_effect);
    field!("engine_fire_effect", &audio.engine_fire_effect);
    field!("oil_leak_effect", &audio.oil_leak_effect);
    field!("petrol_leak_effect", &audio.petrol_leak_effect);
    field!("petrol_fire_effect", &audio.petrol_fire_effect);
    field!("misfire_effect", &audio.misfire_effect);
    field!("part_fire_effect", &audio.part_fire_effect);
    field!("explosion_effect", &audio.explosion_effect);
    field!("exhaust_emitted", audio.exhaust_emitted);
    field!("impact_emitted", audio.impact_emitted);
    field!("damage_emitted", audio.damage_emitted);
    field!("damage_effects", &audio.damage_effects);
    field!("debris_lifetime_seconds", audio.debris_lifetime_seconds);
    field!("max_debris", audio.max_debris);
    field!("physical_debris_persistent", true);

    Value::Object(out)
}

pub(super) fn sync_loop_voice(
    audio: &AudioClient,
    clip: Option<&str>,
    voice_id: &mut Option<u64>,
    active: bool,
    gain: f32,
    speed: f32,
    entity: u64,
    position: [f32; 3],
) -> Result<(), String> {
    let Some(clip) = clip.map(str::trim).filter(|value| !value.is_empty()) else {
        if let Some(existing) = voice_id.take() {
            let _ = audio.stop(existing);
        }
        return Ok(());
    };

    if !active {
        if let Some(existing) = voice_id.take() {
            let _ = audio.stop(existing);
        }
        return Ok(());
    }

    if let Some(existing) = *voice_id {
        match audio.update_voice(&AudioVoiceUpdateRequest {
            voice_id: existing,
            gain: Some(gain.clamp(0.0, 4.0)),
            speed: Some(speed.clamp(0.05, 4.0)),
            paused: Some(!active),
        }) {
            Ok(ack) if ack.accepted => return Ok(()),
            Ok(ack) => {
                *voice_id = None;
                return Err(format!(
                    "looping vehicle voice {existing} update rejected: {}",
                    ack.message
                ));
            }
            Err(error) => {
                *voice_id = None;
                return Err(error);
            }
        }
    }

    if !active {
        return Ok(());
    }
    // New runtime convention: XVAG holds the playable clip. AssetManager
    // owns bytes; AudioClient / engine.audio owns voice creation. The legacy
    // YSCD route remains only for old projects with unmigrated authored cues.
    if clip.to_ascii_lowercase().ends_with(".xvag") {
        let mut request = newviso_audio_client::AudioPlayRequest::new(clip);
        request.looping = true;
        request.gain = gain;
        request.speed = speed;
        request.position = Some(position);
        request.scope_id = Some(entity);
        let ack = audio.play_xvag(&request)?;
        if !ack.accepted {
            return Err(format!(
                "audio provider rejected XVAG vehicle loop '{clip}': {}",
                ack.message
            ));
        }
        *voice_id = Some(ack.voice_id.ok_or_else(|| {
            format!("audio provider accepted XVAG loop '{clip}' without a voice id")
        })?);
        return Ok(());
    }
    // Compatibility path for existing YSCD configurations; new sound assets
    // must be authored as XVAG clips instead.
    let address = AssetAddress::parse(clip)?;
    let is_cue = newviso_assets_client::AssetClient::new()
        .resolve_type(address.logical_path())
        .ok()
        .and_then(|info| {
            info.get("descriptor")
                .and_then(|d| d.get("asset_kind"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .is_some_and(|kind| kind == "sound_cue_dictionary");
    if !is_cue {
        if let Some(existing) = voice_id.take() {
            let _ = audio.stop(existing);
        }
        return Err(format!(
            "vehicle audio requires a final YSCD cue; raw/fallback clip '{clip}' is rejected"
        ));
    }
    let mut request = newviso_audio_client::AudioCuePlayRequest::new(clip);
    request.gain = gain;
    request.pitch = speed;
    request.position = Some(position);
    request.scope_id = Some(entity);
    let ack = audio.play_cue(&request)?;
    if !ack.accepted {
        return Err(format!(
            "audio provider rejected looping vehicle clip '{clip}': {}",
            ack.message
        ));
    }
    *voice_id = ack.voice_id;
    if voice_id.is_none() {
        return Err(format!(
            "audio provider accepted looping vehicle clip '{clip}' without a voice id"
        ));
    }
    Ok(())
}

pub(super) fn play_vehicle_one_shot(
    audio: &AudioClient,
    clip: &str,
    gain: f32,
    pitch: f32,
    entity: u64,
    position: [f32; 3],
) -> Result<(), String> {
    let clip = clip.trim();
    if clip.is_empty() {
        return Ok(());
    }
    if clip.to_ascii_lowercase().ends_with(".xvag") {
        let mut request = newviso_audio_client::AudioPlayRequest::new(clip);
        request.gain = gain;
        request.speed = pitch;
        request.position = Some(position);
        request.scope_id = Some(entity);
        request.seed = Some(
            entity.wrapping_mul(0x9E3779B185EBCA87)
                ^ u64::from(position[0].to_bits()).rotate_left(7)
                ^ u64::from(position[1].to_bits()).rotate_left(23)
                ^ u64::from(position[2].to_bits()).rotate_left(41)
                ^ u64::from(gain.to_bits())
                ^ u64::from(pitch.to_bits()).rotate_left(17),
        );
        let ack = audio.play_xvag(&request)?;
        if !ack.accepted {
            return Err(format!(
                "audio provider rejected XVAG vehicle impact '{clip}': {}",
                ack.message
            ));
        }
        return Ok(());
    }
    // Legacy YSCD cue dictionaries remain supported during migration only.
    let address = AssetAddress::parse(clip)?;
    let is_cue = newviso_assets_client::AssetClient::new()
        .resolve_type(address.logical_path())
        .ok()
        .and_then(|info| {
            info.get("descriptor")
                .and_then(|d| d.get("asset_kind"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .is_some_and(|kind| kind == "sound_cue_dictionary");
    if !is_cue {
        return Err(format!(
            "vehicle one-shot requires a final YSCD cue; raw/fallback clip '{clip}' is rejected"
        ));
    }
    let mut request = newviso_audio_client::AudioCuePlayRequest::new(clip);
    request.gain = gain;
    request.pitch = pitch;
    request.position = Some(position);
    request.scope_id = Some(entity);
    request.seed = Some(
        entity.wrapping_mul(0x9E3779B185EBCA87)
            ^ u64::from(position[0].to_bits()).rotate_left(7)
            ^ u64::from(position[1].to_bits()).rotate_left(23)
            ^ u64::from(position[2].to_bits()).rotate_left(41)
            ^ u64::from(gain.to_bits())
            ^ u64::from(pitch.to_bits()).rotate_left(17),
    );
    let ack = audio.play_cue(&request)?;
    if !ack.accepted {
        return Err(format!(
            "audio provider rejected vehicle cue '{clip}': {}",
            ack.message
        ));
    }
    Ok(())
}

pub(super) fn set_optional_string(
    command: &Value,
    key: &str,
    target: &mut Option<String>,
    index: usize,
) -> Result<(), String> {
    let Some(raw) = command.get(key) else {
        return Ok(());
    };
    if raw.is_null() {
        *target = None;
        return Ok(());
    }
    let value = raw
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            format!(
                "script command[{index}] vehicle presentation field '{key}' must be non-empty string or null"
            )
        })?;
    *target = Some(value.to_owned());
    Ok(())
}

pub(super) fn set_optional_string_and_stop_voice(
    command: &Value,
    key: &str,
    target: &mut Option<String>,
    voice_id: &mut Option<u64>,
    audio: &AudioClient,
    index: usize,
) -> Result<(), String> {
    let previous = target.clone();
    set_optional_string(command, key, target, index)?;
    if previous != *target {
        if let Some(existing) = voice_id.take() {
            let _ = audio.stop(existing);
        }
    }
    Ok(())
}
