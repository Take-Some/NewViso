use super::*;
use newviso_environment::{
    CalendarDate, CelestialPolicy, CloudHatSelection, CloudKeyframeCatalog, GameClock, TimeSample,
    TimecycleCatalog, TimecycleFrame, TimecycleModifier, TimecycleModifierCatalog,
    WeatherCycleEntry, WeatherMachine, WeatherSimulation, WeatherTuning,
};
use serde::{Deserialize, Serialize};
mod commands;
mod presentation;
mod rendering;
mod cloud_rendering;

#[derive(Clone, Debug, Deserialize)]
struct ClimatePolicy {
    #[serde(default)]
    reference_render: bool,
    #[serde(default)]
    weather_lens_enabled: bool,
    #[serde(default)]
    cloud_rendering: cloud_rendering::CloudRenderingPolicy,
    enabled: bool,
    clock_catalog: String,
    weather_catalog: String,
    timecycle_catalog: String,
    cloud_keyframe_catalog: String,
    modifier_catalog: String,
    cloudhat_environment: Option<String>,
    initial_hour: f64,
    initial_date: CalendarDate,
    milliseconds_per_game_minute: f64,
    time_scale: f64,
    dynamic_weather: bool,
    region: String,
    region_blend: f32,
    seed: u64,
    sun_visual_id: String,
    moon_visual_id: String,
    light_id: String,
    sun_radius_degrees: f32,
    moon_radius_degrees: f32,
    light_scale: f32,
    ambient_scale: f32,
    sky_scale: f32,
    shadow_distance: f32,
    #[serde(default)]
    cycle: Option<Vec<WeatherCycleEntry>>,
}
#[derive(Deserialize)]
struct ClockCatalog {
    #[serde(default = "rendering::reference_exposure_defaults")]
    exposure_tuning: newviso_render_client::EnvironmentExposureParams,
    samples: Vec<TimeSample>,
    celestial: CelestialPolicy,
    weather_tuning: WeatherTuning,
    weather_cycle_seconds: f64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClimateWindForcing {
    wind_speed: f32,
    wind_direction: [f32; 2],
    snow_mist: f32,
    /// Optional authoritative sky precipitation. Zero still permits drifting mist.
    #[serde(default)]
    snow_precipitation: Option<f32>,
    #[serde(default)]
    snow_mist_setting: Option<String>,
}
impl ClimateWindForcing {
    fn validate(&self) -> Result<(), String> {
        if self.snow_mist_setting.as_ref().is_some_and(|name|name.is_empty()||name.len()>128) {
            return Err("invalid snow mist layer name".into());
        }
        if self.snow_precipitation.is_some_and(|v|!v.is_finite()||!(0.0..=1.0).contains(&v)){
            return Err("invalid climate snow precipitation".into());
        }
        let norm=self.wind_direction[0].hypot(self.wind_direction[1]);
        if !self.wind_speed.is_finite() || !(0.0..=200.0).contains(&self.wind_speed) ||
            !self.wind_direction.iter().all(|v|v.is_finite()) || !(0.95..=1.05).contains(&norm) ||
            !self.snow_mist.is_finite() || !(0.0..=1.0).contains(&self.snow_mist) {
            return Err("invalid climate wind forcing".into());
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct ClimateCheckpoint {
    clock: GameClock,
    weather: WeatherMachine,
    simulation: WeatherSimulation,
    region: String,
    region_blend: f32,
    modifiers: BTreeMap<String, TimecycleModifier>,
    #[serde(default)]
    cloud_selection: CloudHatSelection,
    #[serde(default)]
    wind_forcing: Option<ClimateWindForcing>,
}

pub(super) struct ClimateRuntime {
    exposure_tuning: newviso_render_client::EnvironmentExposureParams,
    policy: ClimatePolicy,
    pub(super) clock: GameClock,
    pub(super) weather: WeatherMachine,
    simulation: WeatherSimulation,
    celestial: CelestialPolicy,
    tuning: WeatherTuning,
    timecycles: TimecycleCatalog,
    weather_catalog: Value,
    cloud_keyframes: CloudKeyframeCatalog,
    cloud_selection: CloudHatSelection,
    cloud_layers: Vec<(String, usize)>,
    wind_forcing: Option<ClimateWindForcing>,
    cloud_targets_installed: bool,
    cloud_transition_seconds: f32,
    modifier_catalog: TimecycleModifierCatalog,
    modifiers: BTreeMap<String, TimecycleModifier>,
    pub(super) frame: TimecycleFrame,
}
impl ClimateRuntime {
    pub(super) fn load(
        settings: &ProjectRuntimeSettings,
        scene: &mut Scene3dRuntime,
        checkpoint: &Value,
    ) -> Result<Option<Self>, String> {
        let Some(value) = settings
            .variables
            .get("engine_world")
            .and_then(|v| v.get("climate"))
        else {
            return Ok(None);
        };
        if value.get("enabled").and_then(Value::as_bool) != Some(true) {
            return Ok(None);
        }
        let policy: ClimatePolicy = serde_json::from_value(value.clone())
            .map_err(|e| format!("Shared climate settings: {e}"))?;
        if !policy.enabled {
            return Ok(None);
        }
        if !policy.region_blend.is_finite()
            || !(0.0..=1.0).contains(&policy.region_blend)
            || [
                policy.light_scale,
                policy.ambient_scale,
                policy.sky_scale,
                policy.shadow_distance,
                policy.sun_radius_degrees,
                policy.moon_radius_degrees,
            ]
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0)
            || [
                &policy.sun_visual_id,
                &policy.moon_visual_id,
                &policy.light_id,
            ]
            .iter()
            .any(|s| s.trim().is_empty())
        {
            return Err("invalid Shared climate policy".into());
        }
        policy.cloud_rendering.validate()?;
        scene.set_weather_lens_enabled(policy.weather_lens_enabled);
        let assets = AssetClient::new();
        let clock_data: ClockCatalog = serde_json::from_value(assets.json(&policy.clock_catalog)?)
            .map_err(|e| format!("clock catalog: {e}"))?;
        clock_data.celestial.validate()?;
        clock_data.weather_tuning.validate()?;
        let weather_catalog = assets.json(&policy.weather_catalog)?;
        let timecycles =
            TimecycleCatalog::parse(&assets.json(&policy.timecycle_catalog)?, clock_data.samples)?;
        let cloud_keyframes: CloudKeyframeCatalog =
            serde_json::from_value(assets.json(&policy.cloud_keyframe_catalog)?)
                .map_err(|e| e.to_string())?;
        cloud_keyframes.validate()?;
        let modifier_catalog: TimecycleModifierCatalog =
            serde_json::from_value(assets.json(&policy.modifier_catalog)?)
                .map_err(|e| e.to_string())?;
        modifier_catalog.validate()?;
        let entries = weather_catalog
            .get("weather_cycles")
            .and_then(Value::as_array)
            .ok_or("weather catalog requires weather_cycles")?;
        let cycle = if let Some(cycle) = &policy.cycle {
            cycle.clone()
        } else {
            entries
                .iter()
                .map(|e| {
                    Ok(WeatherCycleEntry {
                        id: e
                            .get("cycleName")
                            .and_then(Value::as_str)
                            .ok_or("weather cycle requires cycleName")?
                            .to_owned(),
                        duration_multiplier: e
                            .get("timeMult")
                            .and_then(Value::as_f64)
                            .unwrap_or(1.0),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?
        };
        if cycle.is_empty() {
            return Err("climate weather cycle is empty".into());
        }
        for e in &cycle {
            if !timecycles.contains(&e.id) {
                return Err(format!(
                    "weather cycle references missing timecycle '{}'",
                    e.id
                ));
            }
        }
        let current = scene.weather_backend().current.clone();
        let initial = if current.is_empty()
            || (policy.cycle.is_some()
                && !cycle.iter().any(|e| e.id.eq_ignore_ascii_case(&current)))
        {
            cycle[0].id.clone()
        } else {
            current
        };
        let mut weather = WeatherMachine::new(cycle, clock_data.weather_cycle_seconds, &initial)?;
        weather.dynamic = policy.dynamic_weather;
        if !weather.dynamic {
            weather.set(&initial, &initial, 0.0)?;
        }
        let mut clock = GameClock::new(
            policy.initial_date,
            policy.initial_hour,
            policy.milliseconds_per_game_minute,
        )?;
        clock.time_scale = policy.time_scale;
        clock.validate()?;
        let mut cloud_layers = Vec::new();
        let mut cloud_transition_seconds = 0.0;
        if let Some(path) = policy.cloudhat_environment.as_deref() {
            let source = assets.json(path)?;
            let layers = source
                .get("atmospheric_clouds")
                .ok_or("cloudhat catalog requires atmospheric_clouds")?;
            let environment = ProjectEnvironment::from_value(
                json!({"schema":"newviso.environment.v1","atmospheric_clouds":layers}),
            )
            .map_err(|e| e.to_string())?;
            if !environment.atmospheric_clouds.layers.is_empty() {
                cloud_transition_seconds =
                    environment.atmospheric_clouds.layers[0].transition_seconds;
                for layer in &environment.atmospheric_clouds.layers {
                    if let Some(index) = layer
                        .id
                        .strip_prefix("gtav.")
                        .and_then(|s| s.split('.').next())
                        .and_then(|s| s.parse::<usize>().ok())
                    {
                        if index >= cloud_keyframes.cloud_hat_count {
                            return Err("CloudHat layer has invalid fragment index".into());
                        }
                        cloud_layers.push((layer.id.clone(), index));
                    }
                }
                scene.set_atmospheric_clouds(load_environment_atmospheric_clouds(
                    &environment.atmospheric_clouds,
                )?)?;
            }
        }
        let mut runtime = Self {
            exposure_tuning: clock_data.exposure_tuning,
            simulation: WeatherSimulation::new(policy.seed),
            cloud_selection: CloudHatSelection::new(policy.seed ^ 0x434C4F5544484154),
            cloud_layers,
            wind_forcing: None,
            cloud_targets_installed: false,
            cloud_transition_seconds,
            policy,
            clock,
            weather,
            celestial: clock_data.celestial,
            tuning: clock_data.weather_tuning,
            timecycles,
            weather_catalog,
            cloud_keyframes,
            modifier_catalog,
            modifiers: BTreeMap::new(),
            frame: TimecycleFrame::default(),
        };
        if !checkpoint.is_null() {
            runtime.restore(checkpoint.clone())?;
        }
        host::info("newviso.weather",format!("Shared climate ready weather_types={} cycle_entries={} timecycles={} samples={} calendar={:?}",runtime.weather_catalog["weather_types"].as_array().map_or(0,Vec::len),runtime.weather.cycle.len(),runtime.timecycles.cycle_count(),runtime.timecycles.samples.len(),runtime.clock.date));
        Ok(Some(runtime))
    }
    pub(super) fn validate_weather(&self, id: &str) -> Result<(), String> {
        if self.timecycles.contains(id)
            && self.weather_catalog["weather_types"]
                .as_array()
                .is_some_and(|rows| {
                    rows.iter()
                        .any(|r| r["id"].as_str().is_some_and(|v| v.eq_ignore_ascii_case(id)))
                })
        {
            Ok(())
        } else {
            Err(format!("unknown weather profile '{id}'"))
        }
    }
    fn checkpoint(&self) -> Value {
        json!({"schema":"newviso.climate.save.v1","state":ClimateCheckpoint{clock:self.clock.clone(),weather:self.weather.clone(),simulation:self.simulation.clone(),region:self.policy.region.clone(),region_blend:self.policy.region_blend,modifiers:self.modifiers.clone(),cloud_selection:self.cloud_selection.clone(),wind_forcing:self.wind_forcing.clone()}})
    }
    fn restore(&mut self, value: Value) -> Result<(), String> {
        if value["schema"].as_str() != Some("newviso.climate.save.v1") {
            return Err("unknown climate checkpoint schema".into());
        }
        let state: ClimateCheckpoint =
            serde_json::from_value(value["state"].clone()).map_err(|e| e.to_string())?;
        state.clock.validate()?;
        state.weather.validate()?;
        state.simulation.validate()?;
        state
            .cloud_selection
            .validate(self.cloud_keyframes.cloud_hat_count)?;
        if !state.region_blend.is_finite() || !(0.0..=1.0).contains(&state.region_blend) {
            return Err("invalid saved climate region blend".into());
        }
        state
            .weather
            .validate_profiles(|id| self.validate_weather(id))?;
        let mut f = TimecycleFrame::default();
        for m in state.modifiers.values() {
            f.apply_modifier(m)?;
        }
        if let Some(forcing)=&state.wind_forcing {forcing.validate()?;}
        self.wind_forcing=state.wind_forcing;
        self.clock = state.clock;
        self.weather = state.weather;
        self.simulation = state.simulation;
        self.cloud_selection = state.cloud_selection;
        self.modifiers = state.modifiers;
        self.policy.region = state.region;
        self.policy.region_blend = state.region_blend;
        Ok(())
    }
    fn snapshot(&self, full: bool) -> Value {
        json!({"schema":"newviso.climate.runtime.v1","enabled":true,"clock":self.clock.snapshot(),"weather":self.weather.snapshot(),"wetness":self.simulation.wetness,"temperature":self.simulation.temperature,"wind_speed":self.simulation.wind_speed,"cloud_hat":{"index":self.cloud_selection.index,"key":self.cloud_selection.key},"day_night_ratio":day_night_ratio(self.clock.hours()),"region":self.policy.region,"region_blend":self.policy.region_blend,"timecycle":{"before":self.frame.before,"after":self.frame.after,"blend":self.frame.time_blend,"channel_count":self.frame.channels.len(),"channels":if full {serde_json::to_value(&self.frame.channels).unwrap_or(Value::Null)}else{Value::Null}},"celestial":self.celestial.evaluate(self.clock.hours(),self.clock.elapsed_days)})
    }
}
fn day_night_ratio(hours: f64) -> f32 {
    let h = hours.rem_euclid(24.0);
    if h < 6.0 {
        1.0
    } else if h < 7.0 {
        (7.0 - h) as f32
    } else if h < 19.0 {
        0.0
    } else if h < 20.0 {
        (h - 19.0) as f32
    } else {
        1.0
    }
}

impl EngineApplication {
    pub(super) fn advance_climate_clock(&mut self, dt: f32) -> Result<(), String> {
        if let Some(c) = self.climate.as_mut() {
            c.clock.step(f64::from(dt))?;
            c.weather.step(f64::from(dt))?;
        }
        Ok(())
    }
    pub(super) fn update_climate(&mut self, dt: f32) -> Result<(), String> {
        if let Some(c) = self.climate.as_mut() {
            c.present(&mut self.scene, dt)?;
        }
        Ok(())
    }
    pub(super) fn climate_state(&self, full: bool) -> Value {
        self.climate
            .as_ref()
            .map(|c| c.snapshot(full))
            .unwrap_or_else(|| json!({"enabled":false}))
    }
    pub(super) fn climate_checkpoint(&self) -> Value {
        self.climate
            .as_ref()
            .map(ClimateRuntime::checkpoint)
            .unwrap_or(Value::Null)
    }
}

#[cfg(test)]
mod precipitation_forcing_tests {
    use super::*;
    #[test]
    fn old_wind_checkpoints_keep_the_profile_precipitation(){
        let f:ClimateWindForcing=serde_json::from_value(json!({
            "wind_speed":28,"wind_direction":[0.6,0.8],"snow_mist":0.7})).unwrap();
        f.validate().unwrap();assert_eq!(f.snow_precipitation,None);
    }
    #[test]
    fn dry_wind_and_sky_snow_have_independent_authoritative_forcing(){
        let mut f:ClimateWindForcing=serde_json::from_value(json!({
            "wind_speed":28,"wind_direction":[0.6,0.8],"snow_mist":0.7,
            "snow_precipitation":0})).unwrap();
        f.validate().unwrap();assert_eq!(f.snow_precipitation,Some(0.0));assert_eq!(f.snow_mist,0.7);
        f.snow_precipitation=Some(1.1);assert!(f.validate().is_err());
        f.snow_precipitation=Some(f32::NAN);assert!(f.validate().is_err());
    }
}
