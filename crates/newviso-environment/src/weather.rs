use super::{approach, valid_dt};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WeatherCycleEntry {
    pub id: String,
    pub duration_multiplier: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Transition {
    source: BTreeMap<String, f32>,
    target: String,
    elapsed: f64,
    duration: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WeatherMachine {
    pub cycle: Vec<WeatherCycleEntry>,
    pub base_duration_seconds: f64,
    pub cursor_seconds: f64,
    pub dynamic: bool,
    weights: BTreeMap<String, f32>,
    transition: Option<Transition>,
    pub cycle_index: usize,
    pub current: String,
    pub next: String,
    pub blend: f32,
}
impl WeatherMachine {
    pub fn new(
        cycle: Vec<WeatherCycleEntry>,
        duration: f64,
        initial: &str,
    ) -> Result<Self, String> {
        let mut m = Self {
            cycle,
            base_duration_seconds: duration,
            cursor_seconds: 0.0,
            dynamic: true,
            weights: BTreeMap::from([(initial.to_ascii_uppercase(), 1.0)]),
            transition: None,
            cycle_index: 0,
            current: initial.to_ascii_uppercase(),
            next: initial.to_ascii_uppercase(),
            blend: 0.0,
        };
        m.validate()?;
        if let Some(i) = m
            .cycle
            .iter()
            .position(|e| e.id.eq_ignore_ascii_case(initial))
        {
            m.cursor_seconds = m.cycle[..i]
                .iter()
                .map(|e| e.duration_multiplier * duration)
                .sum();
            m.cycle_index = i;
        }
        Ok(m)
    }
    pub fn validate(&self) -> Result<(), String> {
        let total = self
            .cycle
            .iter()
            .map(|e| e.duration_multiplier * self.base_duration_seconds)
            .sum::<f64>();
        let weight_total = self.weights.values().sum::<f32>();
        if !total.is_finite()
            || total <= 0.0
            || !weight_total.is_finite()
            || self.cursor_seconds >= total
            || self.cycle_index >= self.cycle.len()
            || self.current.is_empty()
            || self.next.is_empty()
            || !self.blend.is_finite()
            || !(0.0..=1.0).contains(&self.blend)
            || self.cycle.is_empty()
            || self.cycle.len() > 100000
            || !self.base_duration_seconds.is_finite()
            || self.base_duration_seconds <= 0.0
            || !self.cursor_seconds.is_finite()
            || self.cursor_seconds < 0.0
            || self.cycle.iter().any(|e| {
                e.id.is_empty()
                    || !e.duration_multiplier.is_finite()
                    || e.duration_multiplier <= 0.0
            })
            || self.weights.is_empty()
            || self.weights.values().any(|w| !w.is_finite() || *w < 0.0)
            || self.weights.values().sum::<f32>() <= 0.0
        {
            return Err("invalid weather cycle state".into());
        }
        if let Some(t) = &self.transition {
            if t.target.is_empty()
                || !t.elapsed.is_finite()
                || t.elapsed < 0.0
                || !t.duration.is_finite()
                || t.duration <= 0.0
                || t.elapsed > t.duration
                || !t.source.values().sum::<f32>().is_finite()
                || t.source.values().sum::<f32>() <= 0.0
                || t.source.is_empty()
                || t.source.values().any(|w| !w.is_finite() || *w < 0.0)
            {
                return Err("invalid weather transition checkpoint".into());
            }
        }
        Ok(())
    }
    pub fn validate_profiles(
        &self,
        mut validate: impl FnMut(&str) -> Result<(), String>,
    ) -> Result<(), String> {
        for id in self.weights.keys().chain(self.cycle.iter().map(|e| &e.id)) {
            validate(id)?;
        }
        validate(&self.current)?;
        validate(&self.next)?;
        if let Some(t) = &self.transition {
            validate(&t.target)?;
            for id in t.source.keys() {
                validate(id)?;
            }
        }
        Ok(())
    }
    pub fn weights(&self) -> &BTreeMap<String, f32> {
        &self.weights
    }
    pub fn set(&mut self, current: &str, next: &str, blend: f32) -> Result<(), String> {
        if current.is_empty()
            || next.is_empty()
            || !blend.is_finite()
            || !(0.0..=1.0).contains(&blend)
        {
            return Err("invalid weather blend".into());
        }
        self.current = current.to_ascii_uppercase();
        self.next = next.to_ascii_uppercase();
        self.blend = blend;
        self.weights.clear();
        *self
            .weights
            .entry(current.to_ascii_uppercase())
            .or_default() += 1.0 - blend;
        *self.weights.entry(next.to_ascii_uppercase()).or_default() += blend;
        self.transition = None;
        self.dynamic = false;
        Ok(())
    }
    pub fn force(&mut self, target: &str, seconds: f64) -> Result<(), String> {
        if target.is_empty() || !seconds.is_finite() || seconds < 0.0 {
            return Err("invalid forced weather transition".into());
        }
        if seconds == 0.0 {
            return self.set(target, target, 0.0);
        }
        self.next = target.to_ascii_uppercase();
        self.blend = 0.0;
        self.transition = Some(Transition {
            source: self.weights.clone(),
            target: target.to_ascii_uppercase(),
            elapsed: 0.0,
            duration: seconds,
        });
        self.dynamic = false;
        Ok(())
    }
    pub fn freeze(&mut self) {
        self.transition = None;
        self.dynamic = false;
    }
    pub fn resume(&mut self) {
        self.transition = None;
        self.dynamic = true;
    }
    pub fn step(&mut self, dt: f64) -> Result<(), String> {
        valid_dt(dt)?;
        self.validate()?;
        if let Some(t) = self.transition.as_mut() {
            t.elapsed = (t.elapsed + dt).min(t.duration);
            let blend = (t.elapsed / t.duration) as f32;
            self.blend = blend;
            self.weights = t
                .source
                .iter()
                .map(|(k, w)| (k.clone(), w * (1.0 - blend)))
                .collect();
            *self.weights.entry(t.target.clone()).or_default() += blend;
            if blend >= 1.0 {
                let target = t.target.clone();
                self.current = target.clone();
                self.next = target.clone();
                self.blend = 0.0;
                self.weights = BTreeMap::from([(target, 1.0)]);
                self.transition = None;
            }
            return Ok(());
        }
        if !self.dynamic {
            return Ok(());
        }
        let total = self
            .cycle
            .iter()
            .map(|e| e.duration_multiplier * self.base_duration_seconds)
            .sum::<f64>();
        self.cursor_seconds = (self.cursor_seconds + dt).rem_euclid(total);
        let mut cursor = self.cursor_seconds;
        for (i, entry) in self.cycle.iter().enumerate() {
            let duration = entry.duration_multiplier * self.base_duration_seconds;
            if cursor < duration {
                let next = &self.cycle[(i + 1) % self.cycle.len()];
                let t = (cursor / duration) as f32;
                self.current = entry.id.to_ascii_uppercase();
                self.next = next.id.to_ascii_uppercase();
                self.blend = t;
                self.weights.clear();
                *self
                    .weights
                    .entry(entry.id.to_ascii_uppercase())
                    .or_default() += 1.0 - t;
                *self
                    .weights
                    .entry(next.id.to_ascii_uppercase())
                    .or_default() += t;
                self.cycle_index = i;
                break;
            }
            cursor -= duration;
        }
        Ok(())
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"dynamic":self.dynamic,"cycle_index":self.cycle_index,"cursor_seconds":self.cursor_seconds,"weights":self.weights,"current":self.current,"next":self.next,"blend":self.blend,"transition":self.transition})
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WeatherTuning {
    pub precipitation_speed: f32,
    pub precipitation_random_min: f32,
    pub precipitation_random_span: f32,
    pub wetness_rain_multiplier: f32,
    pub wetness_increase_speed: f32,
    pub wetness_dry_speed: f32,
    pub wind_change_rate: f32,
    pub wind_interp_step: f32,
    pub wind_direction_step: f32,
    pub wind_speed_max: f32,
    pub wind_height_min: f32,
    pub wind_height_max: f32,
    pub wind_height_multiplier: f32,
    pub temperature_height_min: f32,
    pub temperature_height_max: f32,
    pub temperature_height_drop: f32,
}
impl WeatherTuning {
    pub fn validate(&self) -> Result<(), String> {
        let values = [
            self.precipitation_speed,
            self.precipitation_random_min,
            self.precipitation_random_span,
            self.wetness_rain_multiplier,
            self.wetness_increase_speed,
            self.wetness_dry_speed,
            self.wind_change_rate,
            self.wind_interp_step,
            self.wind_direction_step,
            self.wind_speed_max,
            self.wind_height_min,
            self.wind_height_max,
            self.wind_height_multiplier,
            self.temperature_height_min,
            self.temperature_height_max,
            self.temperature_height_drop,
        ];
        if values.iter().any(|v| !v.is_finite() || *v < 0.0)
            || self.wind_height_max <= self.wind_height_min
            || self.temperature_height_max <= self.temperature_height_min
        {
            return Err("invalid weather simulation tuning".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WeatherSimulation {
    pub rain: f32,
    pub snow: f32,
    pub snow_mist: f32,
    pub wetness: f32,
    pub wind_interp: f32,
    pub wind_target: f32,
    pub wind_direction: f32,
    pub wind_direction_target: f32,
    pub wind_speed: f32,
    pub temperature: f32,
    pub random_state: u64,
}
impl WeatherSimulation {
    pub fn new(seed: u64) -> Self {
        Self {
            rain: 0.0,
            snow: 0.0,
            snow_mist: 0.0,
            wetness: 0.0,
            wind_interp: 0.5,
            wind_target: 0.5,
            wind_direction: 0.0,
            wind_direction_target: 0.0,
            wind_speed: 0.0,
            temperature: 0.0,
            random_state: seed.max(1),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        let v = [
            self.rain,
            self.snow,
            self.snow_mist,
            self.wetness,
            self.wind_interp,
            self.wind_target,
            self.wind_direction,
            self.wind_direction_target,
            self.wind_speed,
            self.temperature,
        ];
        if v.iter().any(|x| !x.is_finite())
            || !(0.0..=1.0).contains(&self.wetness)
            || [self.rain, self.snow, self.snow_mist, self.wind_speed]
                .iter()
                .any(|v| *v < 0.0)
            || !(0.0..=1.0).contains(&self.wind_interp)
            || !(0.0..=1.0).contains(&self.wind_target)
            || self.random_state == 0
        {
            return Err("invalid weather simulation checkpoint".into());
        }
        Ok(())
    }
    fn random(&mut self) -> f32 {
        let mut x = self.random_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.random_state = x;
        (x >> 40) as f32 / (1u32 << 24) as f32
    }
    pub fn step(
        &mut self,
        dt: f32,
        tuning: &WeatherTuning,
        targets: [f32; 3],
        wind_range: [f32; 2],
        height: f32,
        drying: f32,
        wind_mult: f32,
        temperature: f32,
    ) -> Result<(), String> {
        valid_dt(f64::from(dt))?;
        tuning.validate()?;
        self.validate()?;
        if targets
            .iter()
            .chain(wind_range.iter())
            .chain([height, drying, wind_mult, temperature].iter())
            .any(|v| !v.is_finite())
        {
            return Err("non-finite weather simulation input".into());
        }
        if dt > 0.0 {
            let random =
                tuning.precipitation_random_min + tuning.precipitation_random_span * self.random();
            self.rain = approach(
                self.rain,
                targets[0].max(0.0) * random,
                dt * tuning.precipitation_speed,
            );
            self.snow = approach(
                self.snow,
                targets[1].max(0.0) * random,
                dt * tuning.precipitation_speed,
            );
            self.snow_mist = approach(
                self.snow_mist,
                targets[2].max(0.0),
                dt * tuning.precipitation_speed,
            );
            let probability = 1.0 - (-tuning.wind_change_rate * dt).exp();
            if (self.wind_target - self.wind_interp).abs() < 0.001 && self.random() < probability {
                self.wind_target = self.random();
            }
            if (self.wind_direction_target - self.wind_direction).abs() < 0.001
                && self.random() < probability
            {
                self.wind_direction_target = self.random() * std::f32::consts::TAU;
            }
            self.wind_interp = approach(
                self.wind_interp,
                self.wind_target,
                dt * tuning.wind_interp_step,
            );
            self.wind_direction = approach(
                self.wind_direction,
                self.wind_direction_target,
                dt * tuning.wind_direction_step,
            );
            let target = (self.rain * tuning.wetness_rain_multiplier).clamp(0.0, 1.0);
            let step = if target > self.wetness {
                dt * tuning.wetness_increase_speed * self.rain
            } else {
                dt * tuning.wetness_dry_speed * drying.max(0.0) * (1.0 - self.rain).max(0.0)
            };
            self.wetness = approach(self.wetness, target, step).clamp(0.0, 1.0);
        }
        let wind = wind_range[0] + (wind_range[1] - wind_range[0]) * self.wind_interp;
        let height_blend = ((height - tuning.wind_height_min)
            / (tuning.wind_height_max - tuning.wind_height_min))
            .clamp(0.0, 1.0);
        self.wind_speed = (wind * (1.0 + height_blend * (tuning.wind_height_multiplier - 1.0)))
            .clamp(0.0, 1.0)
            * tuning.wind_speed_max
            * wind_mult.max(0.0);
        self.temperature = temperature
            - ((height - tuning.temperature_height_min)
                / (tuning.temperature_height_max - tuning.temperature_height_min))
                .clamp(0.0, 1.0)
                * tuning.temperature_height_drop;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn machine() -> WeatherMachine {
        WeatherMachine::new(
            vec![
                WeatherCycleEntry {
                    id: "CLEAR".into(),
                    duration_multiplier: 2.0,
                },
                WeatherCycleEntry {
                    id: "RAIN".into(),
                    duration_multiplier: 1.0,
                },
            ],
            120.0,
            "CLEAR",
        )
        .unwrap()
    }
    #[test]
    fn long_frame_cycle_catches_up() {
        let mut m = machine();
        m.step(300.0).unwrap();
        assert_eq!(m.cycle_index, 1);
        assert!((m.weights()["RAIN"] - 0.5).abs() < 1e-6);
        m.step(720.0).unwrap();
        assert_eq!(m.cycle_index, 1);
    }
    #[test]
    fn interrupted_transition_does_not_jump() {
        let mut m = machine();
        m.force("RAIN", 20.0).unwrap();
        m.step(10.0).unwrap();
        let before = m.weights().clone();
        m.force("SNOW", 10.0).unwrap();
        m.step(0.0).unwrap();
        for (k, v) in before {
            assert_eq!(m.weights()[&k], v);
        }
        m.step(10.0).unwrap();
        assert_eq!(m.weights()["SNOW"], 1.0);
        m.step(99.0).unwrap();
        assert_eq!(m.weights()["SNOW"], 1.0);
    }
    #[test]
    fn resume_and_checkpoint() {
        let mut m = machine();
        m.set("RAIN", "RAIN", 0.0).unwrap();
        m.step(20.0).unwrap();
        assert_eq!(m.weights()["RAIN"], 1.0);
        m.resume();
        m.step(60.0).unwrap();
        assert_eq!(m.weights()["RAIN"], 0.25);
        let copy: WeatherMachine =
            serde_json::from_value(serde_json::to_value(&m).unwrap()).unwrap();
        copy.validate().unwrap();
        assert_eq!(copy.weights(), m.weights());
    }
}
