use crate::{Vec3, VehicleClass, VehicleInput};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehiclePowertrainKind {
    #[default]
    Automatic,
    Combustion,
    Electric,
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleLeakPolicy {
    Disabled,
    PlayerOnly,
    #[default]
    Enabled,
}

impl VehicleLeakPolicy {
    pub fn permits(self, player_driver: bool) -> bool {
        self == Self::Enabled || self == Self::PlayerOnly && player_driver
    }
}

/// An authored sphere in engine-local coordinates; an explicit component hit
/// remains authoritative. Missing regions retain the legacy spatial fallback.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VehicleDamageRegion {
    pub center: Vec3,
    pub radius: f32,
}

impl VehicleDamageRegion {
    pub fn contains(self, point: Vec3) -> bool {
        (0..3)
            .map(|i| (point[i] - self.center[i]).powi(2))
            .sum::<f32>()
            <= self.radius * self.radius
    }

    fn validate(self) -> Result<(), String> {
        if !self.radius.is_finite()
            || self.radius <= 0.0
            || self.center.iter().any(|v| !v.is_finite())
        {
            return Err(
                "vehicle damage region must have a finite center and positive radius".into(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VehicleDamageFeatures {
    pub indestructible: bool,
    pub bullet_proof: bool,
    pub tyres_can_burst: bool,
    pub wheels_can_break: bool,
    pub engine_damage: bool,
    pub petrol_tank_damage: bool,
    pub engine_fires: bool,
    pub engine_misfires: bool,
    pub oil_leaks: VehicleLeakPolicy,
    pub petrol_leaks: VehicleLeakPolicy,
    pub body_damage_scale: f32,
    pub engine_damage_scale: f32,
    pub petrol_tank_damage_scale: f32,
    pub engine_region: Option<VehicleDamageRegion>,
    pub petrol_tank_region: Option<VehicleDamageRegion>,
}

impl Default for VehicleDamageFeatures {
    fn default() -> Self {
        Self {
            indestructible: false,
            bullet_proof: false,
            tyres_can_burst: true,
            wheels_can_break: true,
            engine_damage: true,
            petrol_tank_damage: true,
            engine_fires: true,
            engine_misfires: true,
            oil_leaks: VehicleLeakPolicy::Enabled,
            petrol_leaks: VehicleLeakPolicy::Enabled,
            body_damage_scale: 1.0,
            engine_damage_scale: 1.0,
            petrol_tank_damage_scale: 1.0,
            engine_region: None,
            petrol_tank_region: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleSteeringMode {
    #[default]
    Configured,
    Rear,
    All,
    HandbrakeRear,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VehicleControlFeatures {
    pub reverse: bool,
    pub handbrake: bool,
    pub steering: VehicleSteeringMode,
}

impl Default for VehicleControlFeatures {
    fn default() -> Self {
        Self {
            reverse: true,
            handbrake: true,
            steering: VehicleSteeringMode::Configured,
        }
    }
}

impl VehicleControlFeatures {
    pub fn constrain(self, input: VehicleInput) -> VehicleInput {
        let mut input = input.sanitized();
        if !self.reverse {
            input.throttle = input.throttle.max(0.0);
        }
        if !self.handbrake {
            input.handbrake = 0.0;
        }
        input
    }
}

/// Source metadata is retained for diagnostics; only named capabilities execute.
/// Unknown source flags never silently become approximated runtime behavior.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VehicleSpecification {
    pub model_name: String,
    pub handling_id: String,
    pub layout: String,
    pub vehicle_type: String,
    pub vehicle_class: String,
    pub source_model_flags: u32,
    pub source_handling_flags: u32,
    pub source_door_damage_flags: u32,
    pub source_flags: Vec<String>,
    pub powertrain: VehiclePowertrainKind,
    /// Tanker trailers carry fuel independently from having a motor.
    pub carries_petrol: bool,
    pub exhaust: bool,
    pub damage: VehicleDamageFeatures,
    pub controls: VehicleControlFeatures,
}

impl Default for VehicleSpecification {
    fn default() -> Self {
        Self {
            model_name: String::new(),
            handling_id: String::new(),
            layout: String::new(),
            vehicle_type: String::new(),
            vehicle_class: String::new(),
            source_model_flags: 0,
            source_handling_flags: 0,
            source_door_damage_flags: 0,
            source_flags: Vec::new(),
            powertrain: VehiclePowertrainKind::Automatic,
            carries_petrol: false,
            exhaust: true,
            damage: VehicleDamageFeatures::default(),
            controls: VehicleControlFeatures::default(),
        }
    }
}

impl VehicleSpecification {
    pub fn has_engine(&self, class: VehicleClass) -> bool {
        match self.powertrain {
            VehiclePowertrainKind::Automatic => class != VehicleClass::Trailer,
            VehiclePowertrainKind::None => false,
            _ => true,
        }
    }

    pub fn uses_combustion(&self, class: VehicleClass) -> bool {
        self.has_engine(class) && !matches!(self.powertrain, VehiclePowertrainKind::Electric)
    }

    pub fn has_petrol_tank(&self, class: VehicleClass) -> bool {
        self.uses_combustion(class) || self.carries_petrol
    }

    pub fn validate(&self) -> Result<(), String> {
        for scale in [
            self.damage.body_damage_scale,
            self.damage.engine_damage_scale,
            self.damage.petrol_tank_damage_scale,
        ] {
            if !scale.is_finite() || !(0.0..=1000.0).contains(&scale) {
                return Err("vehicle damage scales must be finite and between 0 and 1000".into());
            }
        }
        for region in [self.damage.engine_region, self.damage.petrol_tank_region]
            .into_iter()
            .flatten()
        {
            region.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VehiclePowertrainState {
    Off,
    Starting,
    Running,
    FuelStarved,
    Disabled,
    Wrecked,
    Unpowered,
}
