//! Renderer-independent timecycle and weather state descriptors.

#[derive(Clone, Debug, PartialEq)]
pub struct TimeCycleBackendState {
    pub cycle_seconds: f32,
    pub phase: f32,
    pub rate: f32,
    pub duration_seconds: f32,
}

impl Default for TimeCycleBackendState {
    fn default() -> Self {
        Self {
            cycle_seconds: 0.0,
            phase: 0.0,
            rate: 1.0,
            duration_seconds: 86_400.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeatherBackendState {
    pub current: String,
    pub next: String,
    pub blend: f32,
}

impl Default for WeatherBackendState {
    fn default() -> Self {
        Self {
            current: String::new(),
            next: String::new(),
            blend: 0.0,
        }
    }
}

/// Resolved physical/output state of the active weather blend.
///
/// The fields mirror the data-driven channels used by GTA V weather.xml so
/// downstream systems (clouds, precipitation, wind and water) consume one
/// authoritative state rather than reimplementing weather logic independently.
#[derive(Clone, Debug, PartialEq)]
pub struct WeatherEffectsState {
    pub current_cloud_settings: String,
    pub next_cloud_settings: String,
    pub current_timecycle: String,
    pub next_timecycle: String,
    pub current_drop_setting: String,
    pub next_drop_setting: String,
    pub current_mist_setting: String,
    pub next_mist_setting: String,
    pub current_ground_setting: String,
    pub next_ground_setting: String,
    pub current_cloud_variant: String,
    pub next_cloud_variant: String,
    pub sun: f32,
    pub cloud: f32,
    pub wind_min: f32,
    pub wind_max: f32,
    pub wind_speed: f32,
    pub wind_direction: [f32; 2],
    pub rain: f32,
    pub snow: f32,
    pub snow_mist: f32,
    pub fog: f32,
    pub ripple_bumpiness: f32,
    pub ripple_min_bumpiness: f32,
    pub ripple_max_bumpiness: f32,
    pub ripple_bumpiness_wind_scale: f32,
    pub ripple_scale: f32,
    pub ripple_speed: f32,
    pub ripple_velocity_transfer: f32,
    pub ocean_bumpiness: f32,
    pub deep_ocean_scale: f32,
    pub ocean_noise_min_amplitude: f32,
    pub ocean_wave_amplitude: f32,
    pub shore_wave_amplitude: f32,
    pub ocean_wave_wind_scale: f32,
    pub shore_wave_wind_scale: f32,
    pub ocean_wave_min_amplitude: f32,
    pub shore_wave_min_amplitude: f32,
    pub ocean_wave_max_amplitude: f32,
    pub shore_wave_max_amplitude: f32,
    pub ocean_foam_intensity: f32,
    pub ocean_foam_scale: f32,
    pub ripple_disturb: f32,
    pub lightning: f32,
    pub sandstorm: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudHatKeyframeState {
    pub enabled: bool,
    pub cloud_color: [f32; 4],
    pub cloud_light_color: [f32; 4],
    /// Direction toward the active solar/lunar sprite; independent of its HDR disc.
    pub light_direction: Option<[f32; 3]>,
    pub cloud_ambient_color: [f32; 4],
    pub cloud_sky_color: [f32; 4],
    pub cloud_bounce_color: [f32; 4],
    pub cloud_east_color: [f32; 4],
    pub cloud_west_color: [f32; 4],
    pub scale_fill_colors: [f32; 4],
    pub density_shift_scale_scattering: [f32; 4],
    pub piercing_light: [f32; 4],
    pub scale_diffuse_fill_ambient_wrap: [f32; 4],
}

impl Default for CloudHatKeyframeState {
    fn default() -> Self {
        Self {
            enabled: false,
            cloud_color: [1.0; 4],
            cloud_light_color: [1.0; 4],
            light_direction: None,
            cloud_ambient_color: [0.0, 0.0, 0.0, 1.0],
            cloud_sky_color: [0.0, 0.0, 0.0, 1.0],
            cloud_bounce_color: [0.0, 0.0, 0.0, 1.0],
            cloud_east_color: [0.0, 0.0, 0.0, 1.0],
            cloud_west_color: [0.0, 0.0, 0.0, 1.0],
            scale_fill_colors: [1.0; 4],
            density_shift_scale_scattering: [0.0, 1.0, 1.0, 1.0],
            piercing_light: [1.0; 4],
            scale_diffuse_fill_ambient_wrap: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

impl Default for WeatherEffectsState {
    fn default() -> Self {
        Self {
            current_cloud_settings: String::new(),
            next_cloud_settings: String::new(),
            current_timecycle: String::new(),
            next_timecycle: String::new(),
            current_drop_setting: String::new(),
            next_drop_setting: String::new(),
            current_mist_setting: String::new(),
            next_mist_setting: String::new(),
            current_ground_setting: String::new(),
            next_ground_setting: String::new(),
            current_cloud_variant: String::new(),
            next_cloud_variant: String::new(),
            sun: 1.0,
            cloud: 0.0,
            wind_min: 0.0,
            wind_max: 0.0,
            wind_speed: 0.0,
            wind_direction: [1.0, 0.0],
            rain: 0.0,
            snow: 0.0,
            snow_mist: 0.0,
            fog: 0.0,
            ripple_bumpiness: 0.0,
            ripple_min_bumpiness: 0.0,
            ripple_max_bumpiness: 0.0,
            ripple_bumpiness_wind_scale: 0.0,
            ripple_scale: 0.0,
            ripple_speed: 0.0,
            ripple_velocity_transfer: 0.0,
            ocean_bumpiness: 0.0,
            deep_ocean_scale: 0.0,
            ocean_noise_min_amplitude: 0.0,
            ocean_wave_amplitude: 0.0,
            shore_wave_amplitude: 0.0,
            ocean_wave_wind_scale: 0.0,
            shore_wave_wind_scale: 0.0,
            ocean_wave_min_amplitude: 0.0,
            shore_wave_min_amplitude: 0.0,
            ocean_wave_max_amplitude: 0.0,
            shore_wave_max_amplitude: 0.0,
            ocean_foam_intensity: 0.0,
            ocean_foam_scale: 0.0,
            ripple_disturb: 0.0,
            lightning: 0.0,
            sandstorm: 0.0,
        }
    }
}
