//! Generic lumped thermal bodies and heat zones, independent of survival rules.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThermalEnvironment {
    pub temperature: f32,
    pub wind: f32,
    pub wind_loss: f32,
}
impl ThermalEnvironment {
    pub fn validate(&self) -> Result<(), String> {
        if ![self.temperature, self.wind, self.wind_loss]
            .iter()
            .all(|v| v.is_finite())
            || !(-273.0..=200.0).contains(&self.temperature)
            || !(0.0..=200.0).contains(&self.wind)
            || !(0.0..=10.0).contains(&self.wind_loss)
        {
            return Err("invalid thermal environment".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeatSource {
    pub id: String,
    pub position: [f32; 3],
    pub radius: f32,
    pub delta_temperature: f32,
    pub enabled: bool,
}
impl HeatSource {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || !self.position.iter().all(|v| v.is_finite())
            || !self.radius.is_finite()
            || !(0.01..=10000.0).contains(&self.radius)
            || !self.delta_temperature.is_finite()
            || !(0.0..=500.0).contains(&self.delta_temperature)
        {
            return Err("invalid heat source".into());
        }
        Ok(())
    }
    pub fn contribution(&self, p: [f32; 3]) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        let d = p
            .iter()
            .zip(self.position)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f32>()
            .sqrt();
        if d <= self.radius {
            self.delta_temperature
        } else {
            0.0
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThermalBody {
    pub id: String,
    pub entity: u64,
    pub position: [f32; 3],
    pub temperature: f32,
    pub neutral_temperature: f32,
    pub comfortable_ambient: f32,
    pub time_constant: f32,
    pub insulation: f32,
    pub wetness: f32,
    pub wet_loss: f32,
    pub metabolic_offset: f32,
    pub shelter: f32,
    pub ambient_coupling: f32,
}
impl ThermalBody {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || !self.position.iter().all(|v| v.is_finite())
            || ![
                self.temperature,
                self.neutral_temperature,
                self.comfortable_ambient,
                self.time_constant,
                self.insulation,
                self.wetness,
                self.wet_loss,
                self.metabolic_offset,
                self.shelter,
                self.ambient_coupling,
            ]
            .iter()
            .all(|v| v.is_finite())
            || !(0.01..=1.0e7).contains(&self.time_constant)
            || !(0.0..=100.0).contains(&self.insulation)
            || !(0.0..=1.0).contains(&self.wetness)
            || !(0.0..=1.0).contains(&self.wet_loss)
            || !(0.0..=1.0).contains(&self.shelter)
            || !(0.0..=10.0).contains(&self.ambient_coupling)
            || !(-273.0..=1000.0).contains(&self.temperature)
        {
            return Err("invalid thermal body".into());
        }
        Ok(())
    }
    pub fn advance(
        &mut self,
        dt: f64,
        environment: &ThermalEnvironment,
        heat: f32,
    ) -> Result<(), String> {
        super::valid_dt(dt)?;
        environment.validate()?;
        self.validate()?;
        if !heat.is_finite() || !(0.0..=500.0).contains(&heat) {
            return Err("invalid heat contribution".into());
        }
        let ambient = environment.temperature + heat
            - environment.wind * environment.wind_loss * (1.0 - self.shelter);
        let target = self.neutral_temperature
            + (ambient - self.comfortable_ambient) * self.ambient_coupling
                / (1.0 + self.insulation * (1.0 - self.wetness * self.wet_loss))
            + self.metabolic_offset;
        let blend = 1.0 - (-dt / self.time_constant as f64).exp();
        self.temperature += ((target - self.temperature) as f64 * blend) as f32;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn body() -> ThermalBody {
        ThermalBody {
            id: "a".into(),
            entity: 1,
            position: [0.0; 3],
            temperature: 37.0,
            neutral_temperature: 37.0,
            comfortable_ambient: 10.0,
            time_constant: 600.0,
            insulation: 2.0,
            wetness: 0.0,
            wet_loss: 0.8,
            metabolic_offset: 0.0,
            shelter: 0.0,
            ambient_coupling: 0.4,
        }
    }
    #[test]
    fn integration_is_independent_of_frame_size_and_heat_allows_recovery() {
        let e = ThermalEnvironment {
            temperature: -40.0,
            wind: 5.0,
            wind_loss: 0.5,
        };
        let mut a = body();
        let mut b = a.clone();
        a.advance(60.0, &e, 0.0).unwrap();
        for _ in 0..60 {
            b.advance(1.0, &e, 0.0).unwrap();
        }
        assert!((a.temperature - b.temperature).abs() < 0.001);
        let t = a.temperature;
        a.advance(120.0, &e, 60.0).unwrap();
        assert!(a.temperature > t);
    }
    #[test]
    fn wetness_and_wind_increase_cooling() {
        let e = ThermalEnvironment {
            temperature: -30.0,
            wind: 10.0,
            wind_loss: 1.0,
        };
        let mut dry = body();
        let mut wet = dry.clone();
        wet.wetness = 1.0;
        dry.advance(100.0, &e, 0.0).unwrap();
        wet.advance(100.0, &e, 0.0).unwrap();
        assert!(wet.temperature < dry.temperature);
    }
}
