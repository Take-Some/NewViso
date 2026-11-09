use super::*;

impl EngineApplication {
    pub(crate) fn set_vehicle_audio_fx_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let entity = command
            .get("entity")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!(
                    "script command[{index}] vehicle.audio_fx.configure requires resolved entity"
                )
            })?;

        let binding = self
            .vehicle_presentations
            .get_mut(&entity)
            .ok_or_else(|| {
                format!(
                    "script command[{index}] vehicle.audio_fx.configure entity {entity} has no fragment presentation"
                )
            })?;

        let audio = AudioClient::new();
        if command.get("engine_clip").is_some() {
            set_optional_string_and_stop_voice(
                command,
                "engine_clip",
                &mut binding.audio_fx.engine_clip,
                &mut binding.audio_fx.engine_voice,
                &audio,
                index,
            )?;
        }
        for key in [
            "engine_load_clip",
            "engine_high_clip",
            "exhaust_clip",
            "road_clip",
            "brake_disc_clip",
            "flat_tyre_clip",
            "wheel_rim_clip",
            "steering_scrub_clip",
        ] {
            if command.get(key).is_some() {
                let (clip, voice) = match key {
                    "engine_load_clip" => (
                        &mut binding.audio_fx.engine_load_clip,
                        &mut binding.audio_fx.engine_load_voice,
                    ),
                    "engine_high_clip" => (
                        &mut binding.audio_fx.engine_high_clip,
                        &mut binding.audio_fx.engine_high_voice,
                    ),
                    "exhaust_clip" => (
                        &mut binding.audio_fx.exhaust_clip,
                        &mut binding.audio_fx.exhaust_voice,
                    ),
                    "road_clip" => (
                        &mut binding.audio_fx.road_clip,
                        &mut binding.audio_fx.road_voice,
                    ),
                    "brake_disc_clip" => (
                        &mut binding.audio_fx.brake_disc_clip,
                        &mut binding.audio_fx.brake_disc_voice,
                    ),
                    "flat_tyre_clip" => (
                        &mut binding.audio_fx.flat_tyre_clip,
                        &mut binding.audio_fx.flat_tyre_voice,
                    ),
                    "wheel_rim_clip" => (
                        &mut binding.audio_fx.wheel_rim_clip,
                        &mut binding.audio_fx.wheel_rim_voice,
                    ),
                    _ => (
                        &mut binding.audio_fx.steering_scrub_clip,
                        &mut binding.audio_fx.steering_voice,
                    ),
                };
                set_optional_string_and_stop_voice(command, key, clip, voice, &audio, index)?;
            }
        }
        if command.get("tyre_skid_clip").is_some() {
            set_optional_string_and_stop_voice(
                command,
                "tyre_skid_clip",
                &mut binding.audio_fx.tyre_skid_clip,
                &mut binding.audio_fx.tyre_voice,
                &audio,
                index,
            )?;
        }
        if command.get("siren_clip").is_some() {
            set_optional_string_and_stop_voice(
                command,
                "siren_clip",
                &mut binding.audio_fx.siren_clip,
                &mut binding.audio_fx.siren_voice,
                &audio,
                index,
            )?;
        }
        if command.get("engine_gain").is_some() {
            binding.audio_fx.engine_gain =
                optional_number(command, "engine_gain", binding.audio_fx.engine_gain, index)?
                    .clamp(0.0, 4.0);
        }
        if command.get("tyre_gain").is_some() {
            binding.audio_fx.tyre_gain =
                optional_number(command, "tyre_gain", binding.audio_fx.tyre_gain, index)?
                    .clamp(0.0, 4.0);
        }
        for key in ["horn_clip", "radio_clip"] {
            if command.get(key).is_some() {
                let (clip, voice) = if key == "horn_clip" {
                    (
                        &mut binding.audio_fx.horn_clip,
                        &mut binding.audio_fx.horn_voice,
                    )
                } else {
                    (
                        &mut binding.audio_fx.radio_clip,
                        &mut binding.audio_fx.radio_voice,
                    )
                };
                set_optional_string_and_stop_voice(command, key, clip, voice, &audio, index)?;
            }
        }
        if command.get("siren_gain").is_some() {
            binding.audio_fx.siren_gain =
                optional_number(command, "siren_gain", binding.audio_fx.siren_gain, index)?
                    .clamp(0.0, 4.0);
        }
        set_optional_string(
            command,
            "exhaust_effect",
            &mut binding.audio_fx.exhaust_effect,
            index,
        )?;
        set_optional_string(
            command,
            "tyre_effect",
            &mut binding.audio_fx.tyre_effect,
            index,
        )?;
        for key in [
            "tyre_puncture_effect",
            "tyre_burst_effect",
            "engine_smoke_effect",
            "engine_fire_effect",
            "oil_leak_effect",
            "petrol_leak_effect",
            "petrol_fire_effect",
            "misfire_effect",
            "part_fire_effect",
            "explosion_effect",
        ] {
            let target = match key {
                "tyre_puncture_effect" => &mut binding.audio_fx.tyre_puncture_effect,
                "tyre_burst_effect" => &mut binding.audio_fx.tyre_burst_effect,
                "engine_smoke_effect" => &mut binding.audio_fx.engine_smoke_effect,
                "engine_fire_effect" => &mut binding.audio_fx.engine_fire_effect,
                "oil_leak_effect" => &mut binding.audio_fx.oil_leak_effect,
                "petrol_leak_effect" => &mut binding.audio_fx.petrol_leak_effect,
                "petrol_fire_effect" => &mut binding.audio_fx.petrol_fire_effect,
                "misfire_effect" => &mut binding.audio_fx.misfire_effect,
                "part_fire_effect" => &mut binding.audio_fx.part_fire_effect,
                _ => &mut binding.audio_fx.explosion_effect,
            };
            set_optional_string(command, key, target, index)?;
        }
        for key in [
            "engine_start_clip",
            "engine_shutdown_clip",
            "engine_breakdown_clip",
            "gear_shift_clip",
            "brake_release_clip",
            "handbrake_clip",
            "tyre_puncture_clip",
            "tyre_burst_clip",
            "suspension_impact_clip",
            "door_open_clip",
            "door_close_clip",
            "door_locked_attempt_clip",
            "damage_oneshot_clip",
            "brake_chirp_clip",
            "suspension_clatter_clip",
            "impact_clip",
            "impact_heavy_clip",
            "glass_break_clip",
        ] {
            let target = match key {
                "engine_start_clip" => &mut binding.audio_fx.engine_start_clip,
                "engine_shutdown_clip" => &mut binding.audio_fx.engine_shutdown_clip,
                "engine_breakdown_clip" => &mut binding.audio_fx.engine_breakdown_clip,
                "gear_shift_clip" => &mut binding.audio_fx.gear_shift_clip,
                "brake_release_clip" => &mut binding.audio_fx.brake_release_clip,
                "handbrake_clip" => &mut binding.audio_fx.handbrake_clip,
                "tyre_puncture_clip" => &mut binding.audio_fx.tyre_puncture_clip,
                "tyre_burst_clip" => &mut binding.audio_fx.tyre_burst_clip,
                "suspension_impact_clip" => &mut binding.audio_fx.suspension_impact_clip,
                "door_open_clip" => &mut binding.audio_fx.door_open_clip,
                "door_close_clip" => &mut binding.audio_fx.door_close_clip,
                "door_locked_attempt_clip" => &mut binding.audio_fx.door_locked_attempt_clip,
                "damage_oneshot_clip" => &mut binding.audio_fx.damage_oneshot_clip,
                "brake_chirp_clip" => &mut binding.audio_fx.brake_chirp_clip,
                "suspension_clatter_clip" => &mut binding.audio_fx.suspension_clatter_clip,
                "impact_clip" => &mut binding.audio_fx.impact_clip,
                "impact_heavy_clip" => &mut binding.audio_fx.impact_heavy_clip,
                _ => &mut binding.audio_fx.glass_break_clip,
            };
            set_optional_string(command, key, target, index)?;
        }
        set_optional_string(
            command,
            "impact_effect",
            &mut binding.audio_fx.impact_effect,
            index,
        )?;
        set_optional_string(
            command,
            "glass_effect",
            &mut binding.audio_fx.glass_effect,
            index,
        )?;
        set_optional_string(
            command,
            "fallback_catalog",
            &mut binding.audio_fx.fallback_catalog,
            index,
        )?;
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
            if command.get(name).is_some() {
                let mut value = binding.audio_fx.damage_effects.get(name).cloned();
                set_optional_string(command, name, &mut value, index)?;
                if let Some(value) = value {
                    binding
                        .audio_fx
                        .damage_effects
                        .insert(name.to_owned(), value);
                } else {
                    binding.audio_fx.damage_effects.remove(name);
                }
            }
        }
        if command.get("debris_lifetime_seconds").is_some() {
            binding.audio_fx.debris_lifetime_seconds = optional_number(
                command,
                "debris_lifetime_seconds",
                binding.audio_fx.debris_lifetime_seconds,
                index,
            )?
            .clamp(0.0, 600.0);
        }
        binding.audio_fx.audio_retry_after_seconds = 0.0;
        Ok(())
    }
}
