use crate::{VehicleClass, VehicleDamageType};
use serde::{Deserialize, Serialize};

/// Model-independent damage gates. Script health setters remain authoritative.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VehicleDamagePolicy {
    pub invincible: bool,
    pub bullet_proof: bool,
    pub collision_proof: bool,
    pub explosion_proof: bool,
    pub fire_proof: bool,
    pub melee_proof: bool,
    pub water_proof: bool,
}

impl VehicleDamagePolicy {
    pub fn protects(self, kind: VehicleDamageType) -> bool {
        self.invincible
            || match kind {
                VehicleDamageType::Bullet => self.bullet_proof,
                VehicleDamageType::Collision => self.collision_proof,
                VehicleDamageType::Explosive => self.explosion_proof,
                VehicleDamageType::Fire => self.fire_proof,
                VehicleDamageType::Melee => self.melee_proof,
                VehicleDamageType::Water => self.water_proof,
                VehicleDamageType::Script => false,
            }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct VehicleAlarmState {
    pub armed: bool,
    pub remaining_seconds: f32,
}

impl VehicleAlarmState {
    pub fn active(self) -> bool {
        self.remaining_seconds > 0.0
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VehicleThermalState {
    pub temperature_celsius: f32,
    pub has_cooling_fan: bool,
    pub cooling_fan_on: bool,
}

impl Default for VehicleThermalState {
    fn default() -> Self {
        Self {
            temperature_celsius: 20.0,
            has_cooling_fan: false,
            cooling_fan_on: false,
        }
    }
}

impl VehicleThermalState {
    /// CVehicle::UpdateEngineTemperature: ambient cooling, airflow, then fan hysteresis.
    pub fn advance(
        &mut self,
        class: VehicleClass,
        running: bool,
        engine_health: f32,
        on_fire: bool,
        speed_ratio: f32,
        ambient: f32,
        dt: f32,
    ) {
        let mut cooling = if self.has_cooling_fan {
            if self.cooling_fan_on {
                0.4
            } else {
                0.1
            }
        } else {
            0.2
        };
        if on_fire {
            cooling = 0.0;
        } else if engine_health < crate::ENGINE_DAMAGE_RADBURST {
            cooling *= 0.5;
        }
        self.temperature_celsius += (ambient - self.temperature_celsius) * 0.07 * cooling * dt;
        if running {
            let airflow = if matches!(class, VehicleClass::Plane | VehicleClass::Helicopter) {
                0.0
            } else {
                speed_ratio.clamp(0.0, 1.0)
            };
            let core_temperature = 120.0 - airflow * 60.0;
            self.temperature_celsius += (core_temperature - self.temperature_celsius) * 0.07 * dt;
        }
        if !self.has_cooling_fan {
            self.cooling_fan_on = false;
        } else if !self.cooling_fan_on && self.temperature_celsius > 107.0 {
            self.cooling_fan_on = true;
        } else if self.cooling_fan_on && self.temperature_celsius < 80.1 {
            self.cooling_fan_on = false;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VehicleHealthUpdate {
    pub overall_health: Option<f32>,
    pub body_health: Option<f32>,
    pub engine_health: Option<f32>,
    pub petrol_tank_health: Option<f32>,
    pub oil_level: Option<f32>,
}
