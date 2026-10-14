//! weather state.
use super::*;

impl Scene3dRuntime {
    pub fn set_sky_time_seconds(&mut self, seconds: f32) -> Result<(), String> {
        if !seconds.is_finite() {
            return Err("scene sky time must be finite".to_owned());
        }
        self.sky_time_seconds = seconds.rem_euclid(self.timecycle_backend.duration_seconds);
        self.sky_cloud_cycle_time_days =
            self.sky_time_seconds / self.timecycle_backend.duration_seconds;
        Ok(())
    }

    pub fn set_sky_time_scale(&mut self, scale: f32) -> Result<(), String> {
        if !scale.is_finite() || scale.abs() > 100_000.0 {
            return Err("scene sky time scale is invalid".to_owned());
        }
        self.sky_time_scale = scale;
        Ok(())
    }

    pub fn timecycle_backend(&self) -> &TimeCycleBackendState {
        &self.timecycle_backend
    }

    pub fn set_timecycle_backend(
        &mut self,
        cycle_seconds: f32,
        phase: f32,
        rate: f32,
        duration_seconds: f32,
    ) -> Result<(), String> {
        if !cycle_seconds.is_finite()
            || cycle_seconds < 0.0
            || !phase.is_finite()
            || !(0.0..=1.0).contains(&phase)
            || !rate.is_finite()
            || rate.abs() > 100_000.0
            || !duration_seconds.is_finite()
            || duration_seconds <= 0.0
        {
            return Err("invalid generic timecycle backend state".to_owned());
        }

        self.timecycle_backend = TimeCycleBackendState {
            cycle_seconds,
            phase,
            rate,
            duration_seconds,
        };
        Ok(())
    }

    /// The application owns clock integration; the scene must not advance it twice.
    pub fn set_environment_time(
        &mut self,
        seconds: f32,
        elapsed_days: f32,
        rate: f32,
    ) -> Result<(), String> {
        if !seconds.is_finite() || !elapsed_days.is_finite() {
            return Err("invalid environment clock".into());
        }
        self.set_timecycle_backend(
            seconds.rem_euclid(86_400.0),
            seconds.rem_euclid(86_400.0) / 86_400.0,
            rate,
            86_400.0,
        )?;
        self.sky_time_seconds = seconds.rem_euclid(86_400.0);
        self.sky_time_scale = 0.0;
        self.sky_cloud_cycle_time_days = elapsed_days;
        Ok(())
    }

    pub fn set_weather_surface_wetness(&mut self, wetness: f32) -> Result<(), String> {
        if !wetness.is_finite() || !(0.0..=1.0).contains(&wetness) {
            return Err("invalid world wetness".into());
        }
        self.weather_wetness = wetness;
        self.weather_wetness_external = true;
        Ok(())
    }

    pub fn set_keyframed_sky(&mut self, sky: KeyframedSkyDesc) -> Result<(), String> {
        let colors = [
            sky.east,
            sky.west,
            sky.transition,
            sky.zenith,
            sky.zenith_transition,
            sky.sun_color,
        ];
        if colors
            .iter()
            .flatten()
            .chain(sky.zenith_params.iter())
            .chain(
                [
                    sky.azimuth_position,
                    sky.hdr,
                    sky.lunar_phase,
                    sky.mie_phase,
                    sky.mie_scatter,
                    sky.mie_intensity,
                ]
                .iter(),
            )
            .any(|v| !v.is_finite())
        {
            return Err("non-finite keyframed sky".into());
        }
        if let Some(clouds) = &sky.clouds {
            let vectors = [
                clouds.generation, clouds.density, clouds.lighting, clouds.detail,
                clouds.small, clouds.small_color,
            ];
            if vectors.iter().flatten()
                .chain(clouds.base_color.iter())
                .chain(clouds.mid_color.iter())
                .chain(clouds.shadow_color.iter())
                .any(|v| !v.is_finite() || v.abs() > 65536.0)
                || clouds.generation[3] <= 0.0
            {
                return Err("invalid keyframed sky cloud parameters".into());
            }
        }
        self.keyframed_sky = Some(sky);
        Ok(())
    }

    pub fn weather_backend(&self) -> &WeatherBackendState {
        &self.weather_backend
    }

    pub fn weather_effects(&self) -> &WeatherEffectsState {
        &self.weather_effects
    }

    pub fn surface_weather_state(&self) -> (f32, f32) {
        (
            self.weather_wetness.clamp(0.0, 1.0),
            self.weather_effects
                .snow
                .max(self.weather_effects.snow_mist)
                .clamp(0.0, 1.0),
        )
    }

    pub fn set_weather_effects(&mut self, state: WeatherEffectsState) -> Result<(), String> {
        let strings = [
            &state.current_cloud_settings,
            &state.next_cloud_settings,
            &state.current_timecycle,
            &state.next_timecycle,
            &state.current_drop_setting,
            &state.next_drop_setting,
            &state.current_mist_setting,
            &state.next_mist_setting,
            &state.current_ground_setting,
            &state.next_ground_setting,
            &state.current_cloud_variant,
            &state.next_cloud_variant,
        ];
        if strings
            .iter()
            .any(|value| value.len() > 256 || value.chars().any(char::is_control))
        {
            return Err("invalid generic weather effects string field".to_owned());
        }

        let scalars = [
            state.sun,
            state.cloud,
            state.wind_min,
            state.wind_max,
            state.wind_speed,
            state.rain,
            state.snow,
            state.snow_mist,
            state.fog,
            state.ripple_bumpiness,
            state.ripple_min_bumpiness,
            state.ripple_max_bumpiness,
            state.ripple_bumpiness_wind_scale,
            state.ripple_scale,
            state.ripple_speed,
            state.ripple_velocity_transfer,
            state.ocean_bumpiness,
            state.deep_ocean_scale,
            state.ocean_noise_min_amplitude,
            state.ocean_wave_amplitude,
            state.shore_wave_amplitude,
            state.ocean_wave_wind_scale,
            state.shore_wave_wind_scale,
            state.ocean_wave_min_amplitude,
            state.shore_wave_min_amplitude,
            state.ocean_wave_max_amplitude,
            state.shore_wave_max_amplitude,
            state.ocean_foam_intensity,
            state.ocean_foam_scale,
            state.ripple_disturb,
            state.lightning,
            state.sandstorm,
        ];
        if scalars.iter().any(|value| !value.is_finite())
            || state.wind_direction.iter().any(|value| !value.is_finite())
            || state.wind_min < 0.0
            || state.wind_max < state.wind_min
            || state.wind_speed < 0.0
            || [
                state.sun,
                state.cloud,
                state.rain,
                state.snow,
                state.snow_mist,
                state.fog,
                state.lightning,
                state.sandstorm,
            ]
            .iter()
            .any(|value| *value < 0.0 || *value > 4.0)
        {
            return Err("invalid generic weather effects numeric field".to_owned());
        }

        self.weather_effects = state;
        Ok(())
    }

    pub fn cloudhat_keyframe(&self) -> CloudHatKeyframeState {
        self.cloudhat_keyframe
    }

    pub fn set_cloudhat_keyframe(&mut self, state: CloudHatKeyframeState) -> Result<(), String> {
        let vectors = [
            state.cloud_color,
            state.cloud_light_color,
            state.cloud_ambient_color,
            state.cloud_sky_color,
            state.cloud_bounce_color,
            state.cloud_east_color,
            state.cloud_west_color,
            state.scale_fill_colors,
            state.density_shift_scale_scattering,
            state.piercing_light,
            state.scale_diffuse_fill_ambient_wrap,
        ];
        if vectors
            .iter()
            .flatten()
            .any(|value| !value.is_finite() || value.abs() > 256.0)
        {
            return Err("invalid CloudHat keyframe value".to_owned());
        }
        if let Some(direction) = state.light_direction {
            if direction.iter().any(|v| !v.is_finite())
                || direction.iter().map(|v| v * v).sum::<f32>() < 1.0e-8
            {
                return Err("invalid CloudHat light direction".into());
            }
        }
        self.cloudhat_keyframe = state;
        Ok(())
    }

    pub fn set_weather_backend(
        &mut self,
        current: &str,
        next: &str,
        blend: f32,
    ) -> Result<(), String> {
        let current = current.trim();
        let next = next.trim();
        if current.is_empty()
            || next.is_empty()
            || current.len() > 128
            || next.len() > 128
            || !blend.is_finite()
            || !(0.0..=1.0).contains(&blend)
        {
            return Err("invalid generic weather backend state".to_owned());
        }

        self.weather_backend = WeatherBackendState {
            current: current.to_owned(),
            next: next.to_owned(),
            blend,
        };
        Ok(())
    }

    pub(super) fn weather_effects_json(&self) -> Value {
        let state = &self.weather_effects;
        json!({
            "current_cloud_settings": state.current_cloud_settings,
            "next_cloud_settings": state.next_cloud_settings,
            "current_timecycle": state.current_timecycle,
            "next_timecycle": state.next_timecycle,
            "current_drop_setting": state.current_drop_setting,
            "next_drop_setting": state.next_drop_setting,
            "current_mist_setting": state.current_mist_setting,
            "next_mist_setting": state.next_mist_setting,
            "current_ground_setting": state.current_ground_setting,
            "next_ground_setting": state.next_ground_setting,
            "current_cloud_variant": state.current_cloud_variant,
            "next_cloud_variant": state.next_cloud_variant,
            "sun": state.sun,
            "cloud": state.cloud,
            "wind_min": state.wind_min,
            "wind_max": state.wind_max,
            "wind_speed": state.wind_speed,
            "wind_direction": state.wind_direction,
            "rain": state.rain,
            "snow": state.snow,
            "snow_mist": state.snow_mist,
            "fog": state.fog,
            "water": {
                "ripple_bumpiness": state.ripple_bumpiness,
                "ripple_min_bumpiness": state.ripple_min_bumpiness,
                "ripple_max_bumpiness": state.ripple_max_bumpiness,
                "ripple_bumpiness_wind_scale": state.ripple_bumpiness_wind_scale,
                "ripple_scale": state.ripple_scale,
                "ripple_speed": state.ripple_speed,
                "ripple_velocity_transfer": state.ripple_velocity_transfer,
                "ocean_bumpiness": state.ocean_bumpiness,
                "deep_ocean_scale": state.deep_ocean_scale,
                "ocean_noise_min_amplitude": state.ocean_noise_min_amplitude,
                "ocean_wave_amplitude": state.ocean_wave_amplitude,
                "shore_wave_amplitude": state.shore_wave_amplitude,
                "ocean_wave_wind_scale": state.ocean_wave_wind_scale,
                "shore_wave_wind_scale": state.shore_wave_wind_scale,
                "ocean_wave_min_amplitude": state.ocean_wave_min_amplitude,
                "shore_wave_min_amplitude": state.shore_wave_min_amplitude,
                "ocean_wave_max_amplitude": state.ocean_wave_max_amplitude,
                "shore_wave_max_amplitude": state.shore_wave_max_amplitude,
                "ocean_foam_intensity": state.ocean_foam_intensity,
                "ocean_foam_scale": state.ocean_foam_scale,
                "ripple_disturb": state.ripple_disturb
            },
            "lightning": state.lightning,
            "sandstorm": state.sandstorm
        })
    }
}
