use serde::{Deserialize, Serialize};

pub type VehicleEntity = u64;
pub type Vec3 = [f32; 3];
pub type Quat = [f32; 4];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleClass {
    #[default]
    Automobile,
    Bike,
    Boat,
    Plane,
    Helicopter,
    Submarine,
    Train,
    Trailer,
}

impl VehicleClass {
    pub fn uses_wheel_probes(self) -> bool {
        matches!(
            self,
            Self::Automobile | Self::Bike | Self::Train | Self::Trailer
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ReferenceHandlingData {
    #[serde(alias = "m_fMass")]
    pub mass: f32,
    #[serde(alias = "m_fInitialDragCoeff")]
    pub initial_drag_coeff: f32,
    #[serde(alias = "m_fPercentSubmerged")]
    pub percent_submerged: f32,
    #[serde(alias = "m_vecCentreOfMassOffset")]
    pub center_of_mass_offset: Vec3,
    #[serde(alias = "m_vecInertiaMultiplier")]
    pub inertia_multiplier: Vec3,
    #[serde(alias = "m_fDriveBiasFront")]
    pub drive_bias_front: f32,
    #[serde(alias = "m_nInitialDriveGears")]
    pub initial_drive_gears: u8,
    #[serde(alias = "m_fInitialDriveForce")]
    pub initial_drive_force: f32,
    #[serde(alias = "m_fDriveInertia")]
    pub drive_inertia: f32,
    #[serde(alias = "m_fClutchChangeRateScaleUpShift")]
    pub clutch_change_rate_up_shift: f32,
    #[serde(alias = "m_fClutchChangeRateScaleDownShift")]
    pub clutch_change_rate_down_shift: f32,
    #[serde(alias = "m_fInitialDriveMaxFlatVel")]
    pub initial_drive_max_flat_vel: f32,
    #[serde(alias = "m_fBrakeForce")]
    pub brake_force: f32,
    #[serde(alias = "m_fBrakeBiasFront")]
    pub brake_bias_front: f32,
    #[serde(alias = "m_fHandBrakeForce")]
    pub handbrake_force: f32,
    #[serde(alias = "m_fSteeringLock")]
    pub steering_lock: f32,
    #[serde(alias = "m_fTractionCurveMax")]
    pub traction_curve_max: f32,
    #[serde(alias = "m_fTractionCurveMin")]
    pub traction_curve_min: f32,
    #[serde(alias = "m_fTractionCurveLateral")]
    pub traction_curve_lateral: f32,
    #[serde(alias = "m_fTractionSpringDeltaMax")]
    pub traction_spring_delta_max: f32,
    #[serde(alias = "m_fLowSpeedTractionLossMult")]
    pub low_speed_traction_loss_mult: f32,
    #[serde(alias = "m_fCamberStiffnesss")]
    pub camber_stiffness: f32,
    #[serde(alias = "m_fTractionBiasFront")]
    pub traction_bias_front: f32,
    #[serde(alias = "m_fTractionLossMult")]
    pub traction_loss_mult: f32,
    #[serde(alias = "m_fSuspensionForce")]
    pub suspension_force: f32,
    #[serde(alias = "m_fSuspensionCompDamp")]
    pub suspension_comp_damp: f32,
    #[serde(alias = "m_fSuspensionReboundDamp")]
    pub suspension_rebound_damp: f32,
    #[serde(alias = "m_fSuspensionUpperLimit")]
    pub suspension_upper_limit: f32,
    #[serde(alias = "m_fSuspensionLowerLimit")]
    pub suspension_lower_limit: f32,
    #[serde(alias = "m_fSuspensionRaise")]
    pub suspension_raise: f32,
    #[serde(alias = "m_fSuspensionBiasFront")]
    pub suspension_bias_front: f32,
    #[serde(alias = "m_fAntiRollBarForce")]
    pub anti_roll_bar_force: f32,
    #[serde(alias = "m_fAntiRollBarBiasFront")]
    pub anti_roll_bar_bias_front: f32,
    #[serde(alias = "m_fRollCentreHeightFront")]
    pub roll_centre_height_front: f32,
    #[serde(alias = "m_fRollCentreHeightRear")]
    pub roll_centre_height_rear: f32,
    #[serde(alias = "m_fDownforceModifier")]
    pub downforce_modifier: f32,
    #[serde(alias = "m_fCollisionDamageMult")]
    pub collision_damage_multiplier: f32,
    #[serde(alias = "m_fWeaponDamageMult")]
    pub weapon_damage_multiplier: f32,
    #[serde(alias = "m_fDeformationDamageMult")]
    pub deformation_damage_multiplier: f32,
    #[serde(alias = "m_fEngineDamageMult")]
    pub engine_damage_multiplier: f32,
    #[serde(alias = "m_fPetrolTankVolume")]
    pub petrol_tank_volume: f32,
    #[serde(alias = "m_fPetrolConsumptionRate")]
    pub petrol_consumption_rate: f32,
    #[serde(alias = "m_fOilVolume")]
    pub oil_volume: f32,
}

impl Default for ReferenceHandlingData {
    fn default() -> Self {
        Self {
            mass: 1500.0,
            initial_drag_coeff: 8.0,
            percent_submerged: 85.0,
            center_of_mass_offset: [0.0, -0.08, 0.0],
            inertia_multiplier: [1.0, 1.2, 1.4],
            drive_bias_front: 0.0,
            initial_drive_gears: 6,
            initial_drive_force: 0.31,
            drive_inertia: 1.0,
            clutch_change_rate_up_shift: 2.4,
            clutch_change_rate_down_shift: 2.2,
            initial_drive_max_flat_vel: 210.0,
            brake_force: 0.9,
            brake_bias_front: 0.58,
            handbrake_force: 0.8,
            steering_lock: 35.0,
            traction_curve_max: 2.45,
            traction_curve_min: 2.15,
            traction_curve_lateral: 22.5,
            traction_spring_delta_max: 0.15,
            low_speed_traction_loss_mult: 1.0,
            camber_stiffness: 0.0,
            traction_bias_front: 0.49,
            traction_loss_mult: 1.0,
            suspension_force: 2.2,
            suspension_comp_damp: 1.3,
            suspension_rebound_damp: 2.2,
            suspension_upper_limit: 0.12,
            suspension_lower_limit: -0.14,
            suspension_raise: 0.0,
            suspension_bias_front: 0.52,
            anti_roll_bar_force: 0.7,
            anti_roll_bar_bias_front: 0.55,
            roll_centre_height_front: 0.25,
            roll_centre_height_rear: 0.25,
            downforce_modifier: 1.0,
            collision_damage_multiplier: 1.0,
            weapon_damage_multiplier: 1.0,
            deformation_damage_multiplier: 0.8,
            engine_damage_multiplier: 1.5,
            petrol_tank_volume: 30.0,
            // handlingMgr initializes this to zero; scripts may opt into fuel
            // consumption with SET_PETROL_CONSUMPTION_RATE.
            petrol_consumption_rate: 0.0,
            oil_volume: 5.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct HandlingData {
    pub mass: f32,
    pub drag_coefficient: f32,
    pub percent_submerged: f32,
    pub center_of_mass_offset: Vec3,
    pub inertia_multiplier: Vec3,
    pub drive_bias_front: f32,
    pub initial_drive_gears: u8,
    pub initial_drive_force: f32,
    pub drive_inertia: f32,
    pub clutch_change_rate_up_shift: f32,
    pub clutch_change_rate_down_shift: f32,
    pub max_flat_velocity_mps: f32,
    pub max_gearing_velocity_mps: f32,
    pub brake_force: f32,
    pub brake_bias_front: f32,
    pub handbrake_force: f32,
    pub steering_lock_rad: f32,
    pub traction_curve_max: f32,
    pub traction_curve_min: f32,
    pub traction_curve_lateral_rad: f32,
    pub traction_spring_delta_max: f32,
    pub low_speed_traction_loss_mult: f32,
    pub camber_stiffness: f32,
    pub traction_bias_front: f32,
    pub traction_loss_mult: f32,
    pub suspension_force: f32,
    pub suspension_comp_damp: f32,
    pub suspension_rebound_damp: f32,
    pub suspension_upper_limit: f32,
    pub suspension_lower_limit: f32,
    pub suspension_raise: f32,
    pub suspension_bias_front: f32,
    pub anti_roll_bar_force: f32,
    pub anti_roll_bar_bias_front: f32,
    pub roll_centre_height_front: f32,
    pub roll_centre_height_rear: f32,
    pub downforce_modifier: f32,
    pub collision_damage_multiplier: f32,
    pub weapon_damage_multiplier: f32,
    pub deformation_damage_multiplier: f32,
    pub engine_damage_multiplier: f32,
    pub petrol_tank_volume: f32,
    pub petrol_consumption_rate: f32,
    pub oil_volume: f32,
}

impl Default for HandlingData {
    fn default() -> Self {
        Self::from_reference_units(ReferenceHandlingData::default())
    }
}

impl HandlingData {
    pub fn from_reference_units(source: ReferenceHandlingData) -> Self {
        let max_flat_velocity_mps = source.initial_drive_max_flat_vel.max(0.0) / 3.6;
        Self {
            mass: source.mass.max(1.0),
            drag_coefficient: (source.initial_drag_coeff.max(0.0) * 0.0001).max(0.00001),
            percent_submerged: source.percent_submerged.clamp(1.0, 100.0),
            center_of_mass_offset: source.center_of_mass_offset,
            inertia_multiplier: source.inertia_multiplier.map(|v| v.max(0.01)),
            drive_bias_front: source.drive_bias_front.clamp(0.0, 1.0),
            initial_drive_gears: source.initial_drive_gears.clamp(1, 10),
            initial_drive_force: source.initial_drive_force.max(0.0),
            drive_inertia: source.drive_inertia.max(0.01),
            clutch_change_rate_up_shift: source.clutch_change_rate_up_shift.max(0.01),
            clutch_change_rate_down_shift: source.clutch_change_rate_down_shift.max(0.01),
            max_flat_velocity_mps,
            max_gearing_velocity_mps: max_flat_velocity_mps * 1.2,
            brake_force: source.brake_force.max(0.0),
            brake_bias_front: source.brake_bias_front.clamp(0.0, 1.0),
            handbrake_force: source.handbrake_force.max(0.0),
            steering_lock_rad: source.steering_lock.to_radians().abs(),
            traction_curve_max: source.traction_curve_max.max(0.01),
            traction_curve_min: source
                .traction_curve_min
                .clamp(0.01, source.traction_curve_max.max(0.01)),
            traction_curve_lateral_rad: source.traction_curve_lateral.to_radians().abs().max(0.01),
            traction_spring_delta_max: source.traction_spring_delta_max.abs().max(0.001),
            low_speed_traction_loss_mult: source.low_speed_traction_loss_mult.max(0.0),
            camber_stiffness: source.camber_stiffness.max(0.0),
            traction_bias_front: source.traction_bias_front.clamp(0.0, 1.0),
            traction_loss_mult: source.traction_loss_mult.max(0.0),
            suspension_force: source.suspension_force.max(0.01),
            suspension_comp_damp: source.suspension_comp_damp.max(0.0) * 0.1,
            suspension_rebound_damp: source.suspension_rebound_damp.max(0.0) * 0.1,
            suspension_upper_limit: source.suspension_upper_limit.abs(),
            suspension_lower_limit: source.suspension_lower_limit.abs(),
            suspension_raise: source.suspension_raise,
            suspension_bias_front: source.suspension_bias_front.clamp(0.0, 1.0),
            anti_roll_bar_force: source.anti_roll_bar_force.max(0.0),
            anti_roll_bar_bias_front: source.anti_roll_bar_bias_front.clamp(0.0, 1.0),
            roll_centre_height_front: source.roll_centre_height_front,
            roll_centre_height_rear: source.roll_centre_height_rear,
            downforce_modifier: source.downforce_modifier.max(0.0),
            collision_damage_multiplier: source.collision_damage_multiplier.max(0.0),
            weapon_damage_multiplier: source.weapon_damage_multiplier.max(0.0),
            deformation_damage_multiplier: source.deformation_damage_multiplier.max(0.0),
            engine_damage_multiplier: source.engine_damage_multiplier.max(0.0),
            petrol_tank_volume: source.petrol_tank_volume.max(0.0),
            petrol_consumption_rate: source.petrol_consumption_rate.max(0.0),
            oil_volume: source.oil_volume.max(0.0),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let scalars = [
            self.mass,
            self.drag_coefficient,
            self.percent_submerged,
            self.drive_bias_front,
            self.initial_drive_force,
            self.drive_inertia,
            self.clutch_change_rate_up_shift,
            self.clutch_change_rate_down_shift,
            self.max_flat_velocity_mps,
            self.max_gearing_velocity_mps,
            self.brake_force,
            self.brake_bias_front,
            self.handbrake_force,
            self.steering_lock_rad,
            self.traction_curve_max,
            self.traction_curve_min,
            self.traction_curve_lateral_rad,
            self.traction_spring_delta_max,
            self.low_speed_traction_loss_mult,
            self.camber_stiffness,
            self.traction_bias_front,
            self.traction_loss_mult,
            self.suspension_force,
            self.suspension_comp_damp,
            self.suspension_rebound_damp,
            self.suspension_upper_limit,
            self.suspension_lower_limit,
            self.suspension_raise,
            self.suspension_bias_front,
            self.anti_roll_bar_force,
            self.anti_roll_bar_bias_front,
            self.roll_centre_height_front,
            self.roll_centre_height_rear,
            self.downforce_modifier,
            self.collision_damage_multiplier,
            self.weapon_damage_multiplier,
            self.deformation_damage_multiplier,
            self.engine_damage_multiplier,
            self.petrol_tank_volume,
            self.petrol_consumption_rate,
            self.oil_volume,
        ];
        if scalars.iter().any(|v| !v.is_finite())
            || self
                .center_of_mass_offset
                .iter()
                .chain(self.inertia_multiplier.iter())
                .any(|v| !v.is_finite())
        {
            return Err("vehicle handling contains non-finite values".to_owned());
        }
        if self.mass <= 0.0 || self.initial_drive_gears == 0 || self.max_gearing_velocity_mps <= 0.0
        {
            return Err("vehicle handling contains invalid positive-domain values".to_owned());
        }
        Ok(())
    }

    pub fn front_drive_weight(&self) -> f32 {
        if self.drive_bias_front <= 0.1 {
            0.0
        } else if self.drive_bias_front >= 0.9 {
            1.0
        } else {
            self.drive_bias_front
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WheelConfig {
    pub name: String,
    pub mount_local: Vec3,
    pub radius: f32,
    pub width: f32,
    pub rest_length: f32,
    pub travel_up: f32,
    pub travel_down: f32,
    pub steered: bool,
    pub driven: bool,
    pub handbrake: bool,
    pub front: bool,
    pub left: bool,
    pub opposite_index: Option<usize>,
    pub grip_multiplier: f32,
}

impl Default for WheelConfig {
    fn default() -> Self {
        Self {
            name: "wheel".to_owned(),
            mount_local: [0.0, -0.22, 0.0],
            radius: 0.34,
            width: 0.24,
            rest_length: 0.32,
            travel_up: 0.16,
            travel_down: 0.16,
            steered: false,
            driven: true,
            handbrake: false,
            front: false,
            left: false,
            opposite_index: None,
            grip_multiplier: 1.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AeroHandling {
    pub thrust_multiplier: f32,
    pub lift_multiplier: f32,
    pub side_slip_multiplier: f32,
    pub pitch_multiplier: f32,
    pub roll_multiplier: f32,
    pub yaw_multiplier: f32,
    pub pitch_stabilize: f32,
    pub roll_stabilize: f32,
    pub yaw_stabilize: f32,
}

impl Default for AeroHandling {
    fn default() -> Self {
        Self {
            thrust_multiplier: 1.0,
            lift_multiplier: 1.0,
            side_slip_multiplier: 1.0,
            pitch_multiplier: 1.0,
            roll_multiplier: 1.0,
            yaw_multiplier: 1.0,
            pitch_stabilize: 0.7,
            roll_stabilize: 0.9,
            yaw_stabilize: 0.5,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WaterHandling {
    pub thrust_multiplier: f32,
    pub rudder_force: f32,
    pub move_resistance: Vec3,
    pub turn_resistance: Vec3,
    pub buoyancy_ratio: f32,
}

impl Default for WaterHandling {
    fn default() -> Self {
        Self {
            thrust_multiplier: 1.0,
            rudder_force: 0.8,
            move_resistance: [1.2, 0.8, 1.8],
            turn_resistance: [1.0, 1.0, 1.0],
            buoyancy_ratio: 1.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VehicleDefinition {
    pub class: VehicleClass,
    pub specification: crate::VehicleSpecification,
    pub handling: HandlingData,
    pub chassis_half_extents: Vec3,
    pub wheels: Vec<WheelConfig>,
    pub forward_local: Vec3,
    pub up_local: Vec3,
    pub aero: AeroHandling,
    pub water: WaterHandling,
}

impl Default for VehicleDefinition {
    fn default() -> Self {
        Self::automobile()
    }
}

impl VehicleDefinition {
    pub fn automobile() -> Self {
        Self {
            class: VehicleClass::Automobile,
            specification: crate::VehicleSpecification::default(),
            handling: HandlingData::default(),
            chassis_half_extents: [0.92, 0.48, 2.05],
            wheels: vec![
                WheelConfig {
                    name: "wheel_lf".into(),
                    mount_local: [-0.82, -0.28, -1.22],
                    steered: true,
                    driven: false,
                    front: true,
                    left: true,
                    opposite_index: Some(1),
                    ..WheelConfig::default()
                },
                WheelConfig {
                    name: "wheel_rf".into(),
                    mount_local: [0.82, -0.28, -1.22],
                    steered: true,
                    driven: false,
                    front: true,
                    left: false,
                    opposite_index: Some(0),
                    ..WheelConfig::default()
                },
                WheelConfig {
                    name: "wheel_lr".into(),
                    mount_local: [-0.82, -0.28, 1.22],
                    driven: true,
                    handbrake: true,
                    front: false,
                    left: true,
                    opposite_index: Some(3),
                    ..WheelConfig::default()
                },
                WheelConfig {
                    name: "wheel_rr".into(),
                    mount_local: [0.82, -0.28, 1.22],
                    driven: true,
                    handbrake: true,
                    front: false,
                    left: false,
                    opposite_index: Some(2),
                    ..WheelConfig::default()
                },
            ],
            forward_local: [0.0, 0.0, -1.0],
            up_local: [0.0, 1.0, 0.0],
            aero: AeroHandling::default(),
            water: WaterHandling::default(),
        }
    }

    pub fn bike() -> Self {
        let mut value = Self::automobile();
        value.class = VehicleClass::Bike;
        value.handling.mass = 230.0;
        value.chassis_half_extents = [0.36, 0.55, 1.05];
        value.wheels = vec![
            WheelConfig {
                name: "wheel_f".into(),
                mount_local: [0.0, -0.38, -0.78],
                radius: 0.33,
                width: 0.12,
                steered: true,
                driven: false,
                front: true,
                ..WheelConfig::default()
            },
            WheelConfig {
                name: "wheel_r".into(),
                mount_local: [0.0, -0.38, 0.78],
                radius: 0.34,
                width: 0.14,
                steered: false,
                driven: true,
                handbrake: true,
                front: false,
                ..WheelConfig::default()
            },
        ];
        value
    }

    pub fn validate(&self) -> Result<(), String> {
        self.handling.validate()?;
        self.specification.validate()?;
        if self
            .chassis_half_extents
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0)
        {
            return Err("vehicle chassis half extents must be finite and positive".to_owned());
        }
        if self
            .forward_local
            .iter()
            .chain(self.up_local.iter())
            .any(|v| !v.is_finite())
        {
            return Err("vehicle basis contains non-finite values".to_owned());
        }
        if self.class.uses_wheel_probes() && self.wheels.is_empty() {
            return Err("ground vehicle requires at least one wheel".to_owned());
        }
        for (index, wheel) in self.wheels.iter().enumerate() {
            if wheel.mount_local.iter().any(|v| !v.is_finite())
                || [
                    wheel.radius,
                    wheel.width,
                    wheel.rest_length,
                    wheel.travel_up,
                    wheel.travel_down,
                    wheel.grip_multiplier,
                ]
                .iter()
                .any(|v| !v.is_finite())
                || wheel.radius <= 0.0
                || wheel.width <= 0.0
                || wheel.rest_length <= 0.0
                || wheel.travel_up < 0.0
                || wheel.travel_down < 0.0
                || wheel.grip_multiplier <= 0.0
            {
                return Err(format!("vehicle wheel[{index}] is invalid"));
            }
            if wheel
                .opposite_index
                .is_some_and(|other| other >= self.wheels.len() || other == index)
            {
                return Err(format!("vehicle wheel[{index}] has invalid opposite_index"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VehicleSurfaceClass {
    #[default]
    Default,
    Asphalt,
    Concrete,
    Gravel,
    Dirt,
    Grass,
    Snow,
    Ice,
    Sand,
    Mud,
    Metal,
    Water,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VehicleSurfaceProfile {
    pub class: VehicleSurfaceClass,
    /// Dry tyre-force multiplier.
    pub dry_grip: f32,
    /// Tyre-force multiplier at fully accumulated wetness.
    pub wet_grip: f32,
    /// Tyre-force multiplier at full snow coverage.
    pub snow_grip: f32,
    /// Rolling-resistance multiplier exposed for higher-level drivetrain/audio.
    pub rolling_resistance: f32,
    /// Noise/particle response scalar for presentation systems.
    pub fx_response: f32,
}

impl Default for VehicleSurfaceProfile {
    fn default() -> Self {
        Self {
            class: VehicleSurfaceClass::Default,
            dry_grip: 1.0,
            wet_grip: 0.82,
            snow_grip: 0.58,
            rolling_resistance: 1.0,
            fx_response: 1.0,
        }
    }
}

impl VehicleSurfaceProfile {
    pub fn for_class(class: VehicleSurfaceClass) -> Self {
        let (dry_grip, wet_grip, snow_grip, rolling_resistance, fx_response) = match class {
            VehicleSurfaceClass::Default => (1.00, 0.82, 0.58, 1.00, 1.00),
            VehicleSurfaceClass::Asphalt => (1.00, 0.84, 0.58, 1.00, 1.00),
            VehicleSurfaceClass::Concrete => (0.98, 0.80, 0.56, 1.02, 0.95),
            VehicleSurfaceClass::Gravel => (0.76, 0.66, 0.52, 1.25, 1.20),
            VehicleSurfaceClass::Dirt => (0.69, 0.58, 0.48, 1.35, 1.15),
            VehicleSurfaceClass::Grass => (0.60, 0.46, 0.42, 1.45, 0.90),
            VehicleSurfaceClass::Snow => (0.54, 0.48, 0.46, 1.30, 1.25),
            VehicleSurfaceClass::Ice => (0.25, 0.16, 0.18, 0.82, 0.55),
            VehicleSurfaceClass::Sand => (0.51, 0.45, 0.42, 1.85, 1.35),
            VehicleSurfaceClass::Mud => (0.43, 0.34, 0.37, 1.95, 1.45),
            VehicleSurfaceClass::Metal => (0.72, 0.48, 0.50, 0.96, 1.25),
            VehicleSurfaceClass::Water => (0.12, 0.08, 0.10, 2.20, 1.50),
        };
        Self {
            class,
            dry_grip,
            wet_grip,
            snow_grip,
            rolling_resistance,
            fx_response,
        }
    }

    pub fn validate(self) -> Result<Self, String> {
        let values = [
            self.dry_grip,
            self.wet_grip,
            self.snow_grip,
            self.rolling_resistance,
            self.fx_response,
        ];
        if values
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0 || *value > 8.0)
        {
            return Err("vehicle surface profile contains invalid coefficient".to_owned());
        }
        Ok(self)
    }

    pub fn grip(self, wetness: f32, snow: f32) -> f32 {
        let wetness = wetness.clamp(0.0, 1.0);
        let snow = snow.clamp(0.0, 1.0);
        let wet = self.dry_grip + (self.wet_grip - self.dry_grip) * wetness;
        (wet + (self.snow_grip - wet) * snow).max(0.0)
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VehicleInput {
    pub throttle: f32,
    pub brake: f32,
    pub steer: f32,
    pub handbrake: f32,
    pub pitch: f32,
    pub roll: f32,
    pub yaw: f32,
    pub collective: f32,
}

impl VehicleInput {
    pub fn sanitized(mut self) -> Self {
        for value in [
            &mut self.throttle,
            &mut self.brake,
            &mut self.steer,
            &mut self.handbrake,
            &mut self.pitch,
            &mut self.roll,
            &mut self.yaw,
            &mut self.collective,
        ] {
            if !value.is_finite() {
                *value = 0.0;
            }
        }
        self.throttle = self.throttle.clamp(-1.0, 1.0);
        self.brake = self.brake.clamp(0.0, 1.0);
        self.steer = self.steer.clamp(-1.0, 1.0);
        self.handbrake = self.handbrake.clamp(0.0, 1.0);
        self.pitch = self.pitch.clamp(-1.0, 1.0);
        self.roll = self.roll.clamp(-1.0, 1.0);
        self.yaw = self.yaw.clamp(-1.0, 1.0);
        self.collective = self.collective.clamp(0.0, 1.0);
        self
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VehicleBodyState {
    pub position: Vec3,
    pub rotation: Quat,
    pub linear_velocity: Vec3,
    pub angular_velocity: Vec3,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct VehicleProbe {
    pub seq: u64,
    pub vehicle: VehicleEntity,
    pub wheel_index: usize,
    pub origin: Vec3,
    pub direction: Vec3,
    pub max_distance: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct VehicleProbeHit {
    pub seq: u64,
    pub position: Vec3,
    pub normal: Vec3,
    pub distance: f32,
    pub surface_entity: Option<u64>,
    /// Opaque collision-surface id. Interpretation is supplied by project/shared
    /// vehicle policy and never by the source-format-neutral physics contract.
    pub surface_id: Option<u32>,
}

#[derive(Clone, Copy, Debug)]
pub struct VehicleImpulse {
    pub vehicle: VehicleEntity,
    pub impulse: Vec3,
    pub point: Vec3,
}

#[derive(Clone, Copy, Debug)]
pub struct VehicleAngularVelocityDelta {
    pub vehicle: VehicleEntity,
    pub delta: Vec3,
}

#[derive(Clone, Debug, Default)]
pub struct VehicleFramePlan {
    pub probes: Vec<VehicleProbe>,
    pub impulses: Vec<VehicleImpulse>,
    pub angular_velocity_deltas: Vec<VehicleAngularVelocityDelta>,
}

/// Independent tyre condition. Missing wheels provide no suspension support.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TireCondition {
    #[default]
    Intact,
    Punctured,
    Rim,
    Missing,
}

impl TireCondition {
    pub fn radius_multiplier(self) -> f32 {
        self.radius_multiplier_with_rubber(1.0)
    }

    pub fn radius_multiplier_with_rubber(self, rubber_remaining: f32) -> f32 {
        let rubber = rubber_remaining.clamp(0.0, 1.0);
        match self {
            Self::Intact => 1.0,
            // A flat tyre keeps its rim. Only the rubber envelope collapses as it
            // is scrubbed away, so the effective radius approaches the bare rim.
            Self::Punctured => 0.65 + (0.82 - 0.65) * rubber,
            Self::Rim => 0.65,
            Self::Missing => 0.0,
        }
    }

    pub fn grip_multiplier(self) -> f32 {
        self.grip_multiplier_with_rubber(1.0)
    }

    pub fn grip_multiplier_with_rubber(self, rubber_remaining: f32) -> f32 {
        let rubber = rubber_remaining.clamp(0.0, 1.0);
        match self {
            Self::Intact => 1.0,
            Self::Punctured => 0.22 + (0.48 - 0.22) * rubber,
            Self::Rim => 0.22,
            Self::Missing => 0.0,
        }
    }

    pub fn rolling_resistance(self) -> f32 {
        self.rolling_resistance_with_rubber(1.0)
    }

    pub fn rolling_resistance_with_rubber(self, rubber_remaining: f32) -> f32 {
        let rubber = rubber_remaining.clamp(0.0, 1.0);
        match self {
            Self::Intact => 0.0,
            Self::Punctured => 0.12 - (0.12 - 0.075) * rubber,
            Self::Rim => 0.12,
            Self::Missing => 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct WheelTelemetry {
    pub tire_condition: TireCondition,
    /// Fraction of the tyre carcass still surrounding the rim. Intact/flat starts
    /// at 1.0; driving on a flat scrubs it toward 0.0, where only the rim remains.
    pub tire_rubber_remaining: f32,
    pub effective_radius: f32,
    pub tire_grip_multiplier: f32,
    pub contact: bool,
    pub contact_position: Option<Vec3>,
    pub contact_normal: Option<Vec3>,
    pub compression: f32,
    pub suspension_velocity: f32,
    pub normal_force: f32,
    pub longitudinal_slip: f32,
    pub lateral_slip_angle: f32,
    /// Dimensionless combined tyre slip. 1.0 is approximately the authored
    /// traction peak; values above one mean the contact patch is sliding.
    pub slip_intensity: f32,
    pub angular_velocity: f32,
    pub rotation_angle: f32,
    pub steer_angle: f32,
    pub surface_entity: Option<u64>,
    pub surface_id: Option<u32>,
    pub surface_class: VehicleSurfaceClass,
    pub surface_grip_multiplier: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct VehicleTelemetry {
    pub entity: VehicleEntity,
    pub specification: crate::VehicleSpecification,
    pub powertrain_state: crate::VehiclePowertrainState,
    pub driveable_player: bool,
    pub driveable_ai: bool,
    pub engine_output_multiplier: f32,
    pub enabled: bool,
    pub player_driver: bool,
    pub alarm: crate::systems::VehicleAlarmState,
    pub thermal: crate::systems::VehicleThermalState,
    pub damage_policy: crate::systems::VehicleDamagePolicy,
    pub class: VehicleClass,
    pub speed_mps: f32,
    pub speed_forward_mps: f32,
    pub gear: i8,
    pub engine_speed: f32,
    pub engine_running: bool,
    pub engine_starting: bool,
    pub engine_start_remaining: f32,
    pub failed_engine_start_attempts: u8,
    pub engine_condition: f32,
    pub engine_smoke_level: f32,
    pub engine_fire_level: f32,
    pub engine_misfiring: bool,
    pub petrol_leak_level: f32,
    pub petrol_fire_level: f32,
    pub petrol_tank_level: f32,
    pub petrol_tank_capacity: f32,
    pub fuel_fraction: f32,
    pub oil_level: f32,
    pub oil_capacity: f32,
    pub exploded: bool,
    pub manual_gear: Option<i8>,
    pub clutch: f32,
    pub input: VehicleInput,
    pub wheels: Vec<WheelTelemetry>,
}
