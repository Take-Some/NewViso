use super::*;

impl EngineApplication {
    pub(crate) fn apply_climate_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        let c = self.climate.as_mut().ok_or_else(|| {
            format!("script command[{index}] requires enabled engine_world.climate")
        })?;
        let number = |key: &str| {
            command
                .get(key)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("script command[{index}] {op} requires finite '{key}'"))
        };
        let text = |key: &str| {
            command
                .get(key)
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| format!("script command[{index}] {op} requires string '{key}'"))
        };
        let boolean = |key: &str| {
            command
                .get(key)
                .and_then(Value::as_bool)
                .ok_or_else(|| format!("script command[{index}] {op} requires boolean '{key}'"))
        };
        match op {
            "scene.clock.set" => c.clock.set_time(number("seconds")?)?,
            "scene.clock.add" => c.clock.add_time(number("seconds")?)?,
            "scene.clock.advance_to" => c.clock.advance_to(number("seconds")?)?,
            "scene.clock.date.set" => {
                let date: CalendarDate = serde_json::from_value(
                    command
                        .get("date")
                        .cloned()
                        .ok_or("clock date command requires date")?,
                )
                .map_err(|e| e.to_string())?;
                date.validate()?;
                c.clock.set_date(date)?;
            }
            "scene.clock.pause" => c.clock.paused = boolean("paused")?,
            "scene.clock.fixed.set" => c.clock.fixed = boolean("fixed")?,
            "scene.clock.rate.set" => {
                let mut clock = c.clock.clone();
                if command.get("milliseconds_per_game_minute").is_some() {
                    clock.milliseconds_per_game_minute = number("milliseconds_per_game_minute")?;
                }
                if command.get("time_scale").is_some() {
                    clock.time_scale = number("time_scale")?;
                }
                clock.validate()?;
                c.clock = clock;
            }
            "scene.weather.force" | "scene.weather.transition" => {
                let id = text("weather")?;
                c.validate_weather(id)?;
                let seconds = if op == "scene.weather.force" {
                    0.0
                } else {
                    number("duration_seconds")?
                };
                c.weather.force(id, seconds)?;
                if seconds == 0.0 {
                    c.cloud_targets_installed = false;
                }
            }
            "scene.weather.wind.set" => {
                let mut forcing:ClimateWindForcing=serde_json::from_value(
                    command.get("forcing").cloned().ok_or("wind command requires forcing")?)
                    .map_err(|e|format!("climate wind: {e}"))?;
                forcing.validate()?;
                let norm=forcing.wind_direction[0].hypot(forcing.wind_direction[1]);
                forcing.wind_direction=forcing.wind_direction.map(|v|v/norm);
                c.wind_forcing=Some(forcing);
            }
            "scene.weather.wind.clear" => c.wind_forcing=None,
            "scene.weather.resume" => c.weather.resume(),
            "scene.weather.dynamic.set" => {
                if boolean("enabled")? {
                    c.weather.resume();
                } else {
                    c.weather.freeze();
                }
            }
            "scene.timecycle.region.set" => {
                let region = text("region")?.to_owned();
                let blend = number("blend")? as f32;
                if !(0.0..=1.0).contains(&blend) {
                    return Err("timecycle region blend must be in 0..=1".into());
                }
                c.policy.region = region;
                c.policy.region_blend = blend;
            }
            "scene.timecycle.modifier.set" => {
                let modifier = TimecycleModifier {
                    id: text("id")?.to_owned(),
                    strength: number("strength")? as f32,
                    operations: BTreeMap::new(),
                    channels: serde_json::from_value(
                        command
                            .get("channels")
                            .cloned()
                            .ok_or("timecycle modifier requires channels")?,
                    )
                    .map_err(|e| e.to_string())?,
                };
                if modifier
                    .channels
                    .keys()
                    .any(|k| !c.frame.channels.contains_key(k))
                {
                    return Err("timecycle modifier references an unknown channel".into());
                }
                let mut frame = c.frame.clone();
                frame.apply_modifier(&modifier)?;
                c.modifiers.insert(modifier.id.clone(), modifier);
            }
            "scene.timecycle.modifier.use" => {
                let modifier = c
                    .modifier_catalog
                    .modifier(text("name")?, number("strength")? as f32)?;
                let slot = command
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("script");
                c.modifiers.insert(slot.to_owned(), modifier);
            }
            "scene.timecycle.modifier.remove" => {
                c.modifiers.remove(text("id")?);
            }
            "scene.timecycle.modifiers.clear" => c.modifiers.clear(),
            _ => return Err(format!("unsupported climate command '{op}'")),
        }
        c.present(&mut self.scene, 0.0)
    }
}
