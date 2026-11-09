use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LatchKind {
    None,
    #[default]
    Handle,
    Roller,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MechanismConfig {
    pub latch: LatchKind,
    pub initially_locked: bool,
    pub key_item: Option<String>,
    pub free_egress_side: Option<i8>,
    pub capture_metres: f32,
    pub capture_speed_mps: f32,
    pub roller_release_nm: f32,
    pub latch_torque_nm: f32,
    pub bolt_torque_nm: f32,
    pub retention_stiffness: f32,
    pub latch_fracture_joules: f32,
    pub bolt_fracture_joules: f32,
    pub operating_torque_nm: f32,
    pub operating_stiffness: f32,
    pub closer_torque_nm: f32,
    pub closer_stiffness: f32,
    pub hold_seconds: f32,
    pub operation_timeout_seconds: f32,
    pub reservation_seconds: f32,
    pub retry_seconds: f32,
    pub interaction_reach: f32,
    pub open_angle_radians: f32,
    pub passage_margin: f32,
}
impl Default for MechanismConfig {
    fn default() -> Self {
        Self {
            latch: LatchKind::Handle,
            initially_locked: false,
            key_item: None,
            free_egress_side: None,
            capture_metres: 0.008,
            capture_speed_mps: 0.35,
            roller_release_nm: 5.0,
            latch_torque_nm: 500.0,
            bolt_torque_nm: 2500.0,
            retention_stiffness: 80000.0,
            latch_fracture_joules: 120.0,
            bolt_fracture_joules: 450.0,
            operating_torque_nm: 45.0,
            operating_stiffness: 100.0,
            closer_torque_nm: 8.0,
            closer_stiffness: 12.0,
            hold_seconds: 2.0,
            operation_timeout_seconds: 6.0,
            reservation_seconds: 8.0,
            retry_seconds: 3.0,
            interaction_reach: 1.6,
            open_angle_radians: 1.5,
            passage_margin: 0.06,
        }
    }
}
impl MechanismConfig {
    pub fn validate(&self) -> Result<(), String> {
        let positive = [
            self.capture_metres,
            self.capture_speed_mps,
            self.roller_release_nm,
            self.latch_torque_nm,
            self.bolt_torque_nm,
            self.retention_stiffness,
            self.latch_fracture_joules,
            self.bolt_fracture_joules,
            self.operating_torque_nm,
            self.operating_stiffness,
            self.closer_stiffness,
            self.hold_seconds,
            self.operation_timeout_seconds,
            self.reservation_seconds,
            self.retry_seconds,
            self.interaction_reach,
            self.open_angle_radians,
        ];
        if positive.iter().any(|v| !v.is_finite() || *v <= 0.0)
            || !self.closer_torque_nm.is_finite()
            || self.closer_torque_nm < 0.0
            || !self.passage_margin.is_finite()
            || self.passage_margin < 0.0
            || self.open_angle_radians >= std::f32::consts::PI
            || self.free_egress_side.is_some_and(|v| v != -1 && v != 1)
            || self
                .key_item
                .as_ref()
                .is_some_and(|s| s.trim().is_empty() || s.len() > 128)
        {
            return Err("invalid door mechanism dimensions, efforts, access or timing".into());
        }
        Ok(())
    }
    pub fn access(&self, side: f32, has_key: bool) -> bool {
        has_key || self.free_egress_side.is_some_and(|s| side * s as f32 > 0.0)
    }
}
