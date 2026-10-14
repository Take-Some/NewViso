//! Validate mounted climate data and evaluate every weather/region across midnight.
use newviso_environment::*;
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::PathBuf};

#[derive(Deserialize)]
struct ClockData {
    samples: Vec<TimeSample>,
    celestial: CelestialPolicy,
    weather_tuning: WeatherTuning,
    weather_cycle_seconds: f64,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: verify_catalog Shared/Content/environment/weather")?,
    );
    let read = |file: &str| -> Result<Value, Box<dyn std::error::Error>> {
        Ok(serde_json::from_slice(&fs::read(root.join(file))?)?)
    };
    let clock: ClockData = serde_json::from_value(read("gtav_clock.catalog.json")?)?;
    clock.celestial.validate()?;
    clock.weather_tuning.validate()?;
    let timecycles =
        TimecycleCatalog::parse(&read("gtav_weather.timecycles.json")?, clock.samples)?;
    let cloud: CloudKeyframeCatalog =
        serde_json::from_value(read("gtav_cloudhat.keyframes.json")?)?;
    cloud.validate()?;
    let modifiers: TimecycleModifierCatalog =
        serde_json::from_value(read("gtav_timecycle.modifiers.json")?)?;
    modifiers.validate()?;
    let weather = read("gtav_weather.catalog.json")?;
    let rows = weather["weather_types"].as_array().ok_or("weather_types")?;
    let cycle = weather["weather_cycles"]
        .as_array()
        .ok_or("weather_cycles")?
        .iter()
        .map(|e| WeatherCycleEntry {
            id: e["cycleName"].as_str().unwrap().into(),
            duration_multiplier: e["timeMult"].as_f64().unwrap(),
        })
        .collect();
    let mut machine = WeatherMachine::new(cycle, clock.weather_cycle_seconds, "CLEAR")?;
    machine.step(1_000_000.0)?;
    machine.validate_profiles(|id| {
        if timecycles.contains(id) {
            Ok(())
        } else {
            Err(id.into())
        }
    })?;
    let mut evaluations = 0;
    let mut max_channels = 0;
    for row in rows {
        let id = row["id"].as_str().ok_or("weather id")?;
        let weights = BTreeMap::from([(id.into(), 1.0)]);
        let cloud_id = row["cloudSettingsName"]
            .as_str()
            .ok_or("weather cloudSettingsName")?;
        for region in ["GLOBAL", "URBAN"] {
            for minute in 0..=1440 {
                let hour = f64::from(minute) / 60.0;
                let mut frame = timecycles.evaluate(hour, &weights, region, 1.0)?;
                modifiers.fill_defaults(&mut frame);
                if frame.channels.values().any(|v| !v.is_finite()) {
                    return Err("nonfinite timecycle".into());
                }
                let values = cloud.evaluate(hour, &BTreeMap::from([(cloud_id.into(), 1.0)]))?;
                if values.len() != 11 {
                    return Err("cloud curve count".into());
                }
                max_channels = max_channels.max(frame.channels.len());
                evaluations += 1;
            }
        }
    }
    let mut noon = timecycles.evaluate(
        12.0,
        &BTreeMap::from([("CLEAR".into(), 1.0)]),
        "GLOBAL",
        0.0,
    )?;
    modifiers.fill_defaults(&mut noon);
    for modifier in modifiers.modifiers.values() {
        let mut frame = noon.clone();
        frame.apply_modifier(modifier)?;
        if frame.channels.values().any(|v| !v.is_finite()) {
            return Err(format!("modifier {}", modifier.id).into());
        }
    }
    let mut simulation = WeatherSimulation::new(12345);
    for _ in 0..1800 {
        simulation.step(
            1.0 / 30.0,
            &clock.weather_tuning,
            [1.0, 0.0, 0.0],
            [0.5, 0.5],
            50.0,
            1.0,
            1.0,
            20.0,
        )?;
    }
    let wet = simulation.wetness;
    if wet < 0.9 {
        return Err("rain did not wet the world".into());
    }
    for _ in 0..6000 {
        simulation.step(
            1.0 / 30.0,
            &clock.weather_tuning,
            [0.0; 3],
            [0.5, 0.5],
            1000.0,
            1.0,
            1.0,
            20.0,
        )?;
    }
    if simulation.wetness > 0.001
        || (simulation.wind_speed - 12.0).abs() > 0.001
        || (simulation.temperature - 5.0).abs() > 0.001
    {
        return Err("drying or height adjustment mismatch".into());
    }
    println!(
        "{}",
        serde_json::json!({"weather_types":rows.len(),"cycle_entries":machine.cycle.len(),"clock_samples":timecycles.samples.len(),"cloud_presets":cloud.presets.len(),"modifiers":modifiers.modifiers.len(),"channels":max_channels,"minute_region_evaluations":evaluations,"wetness_after_rain":wet,"wetness_after_drying":simulation.wetness,"high_altitude_wind":simulation.wind_speed,"high_altitude_temperature":simulation.temperature})
    );
    Ok(())
}
