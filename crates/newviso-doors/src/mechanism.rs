use crate::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoorResult {
    Accepted,
    Denied,
    NotClosed,
    Broken,
    Busy,
    OutOfReach,
    Obstructed,
    Cancelled,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Operation {
    Open { target_angle: f32 },
    Close,
    Lock,
    Unlock,
    Release,
    Repair,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Observation {
    pub seconds: f64,
    pub angle: f32,
    pub angular_speed: f32,
    pub edge_radius: f32,
    pub load_nm: f32,
    pub impact_joules: f32,
    pub occupied: bool,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Goal {
    pub angle: f32,
    pub until: f64,
    pub started: f64,
    pub start_angle: f32,
    pub reached: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MechanismState {
    pub locked: bool,
    pub latch_engaged: bool,
    pub latch_broken: bool,
    pub bolt_broken: bool,
    pub latch_damage_joules: f32,
    pub bolt_damage_joules: f32,
    pub handle_until: f64,
    pub jammed_until: f64,
    pub goal: Option<Goal>,
    pub last_result: DoorResult,
    pub revision: u64,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Actuation {
    pub retention_nm: f32,
    pub retention_stiffness: f32,
    pub target: Option<f32>,
    pub motor_stiffness: f32,
    pub motor_nm: f32,
    pub released_joules: f32,
}
#[derive(Clone, Debug)]
pub struct Mechanism {
    pub config: MechanismConfig,
    pub state: MechanismState,
}
impl Mechanism {
    pub fn new(config: MechanismConfig) -> Result<Self, String> {
        config.validate()?;
        let state = MechanismState {
            locked: config.initially_locked,
            latch_engaged: config.latch != LatchKind::None,
            latch_broken: false,
            bolt_broken: false,
            latch_damage_joules: 0.0,
            bolt_damage_joules: 0.0,
            handle_until: 0.0,
            jammed_until: 0.0,
            goal: None,
            last_result: DoorResult::Accepted,
            revision: 1,
        };
        Ok(Self { config, state })
    }
    pub fn closed(&self, o: Observation) -> bool {
        o.angle.sin().abs() * o.edge_radius <= self.config.capture_metres
            && o.angular_speed.abs() * o.edge_radius <= self.config.capture_speed_mps
    }
    pub fn operate(&mut self, op: Operation, o: Observation, authorized: bool) -> DoorResult {
        let result = match op {
            Operation::Unlock if !authorized => DoorResult::Denied,
            Operation::Unlock if self.state.bolt_broken => DoorResult::Broken,
            Operation::Unlock => {
                self.state.locked = false;
                DoorResult::Accepted
            }
            Operation::Lock if !authorized => DoorResult::Denied,
            Operation::Lock if self.state.bolt_broken => DoorResult::Broken,
            Operation::Lock if !self.closed(o) || o.occupied => DoorResult::NotClosed,
            Operation::Lock => {
                self.state.locked = true;
                self.state.goal = None;
                self.state.handle_until = 0.0;
                DoorResult::Accepted
            }
            Operation::Open { .. } if self.state.locked && !authorized => DoorResult::Denied,
            Operation::Open { .. } if o.seconds < self.state.jammed_until => DoorResult::Obstructed,
            Operation::Open { target_angle }
                if target_angle.is_finite() && target_angle.abs() < std::f32::consts::PI =>
            {
                self.state.locked = false;
                self.state.latch_engaged = false;
                self.state.handle_until = o.seconds + self.config.hold_seconds as f64;
                let previous = self
                    .state
                    .goal
                    .filter(|g| (g.angle - target_angle).abs() < f32::EPSILON);
                self.state.goal = Some(Goal {
                    angle: target_angle,
                    until: if previous.is_some_and(|g| g.reached) {
                        self.state.handle_until
                    } else {
                        o.seconds + self.config.operation_timeout_seconds as f64
                    },
                    started: previous.map_or(o.seconds, |g| g.started),
                    start_angle: previous.map_or(o.angle, |g| g.start_angle),
                    reached: previous.is_some_and(|g| g.reached),
                });
                DoorResult::Accepted
            }
            Operation::Open { .. } => DoorResult::Denied,
            Operation::Close if o.occupied => DoorResult::Busy,
            Operation::Close => {
                self.state.goal = Some(Goal {
                    angle: 0.0,
                    until: o.seconds + self.config.operation_timeout_seconds as f64,
                    started: o.seconds,
                    start_angle: o.angle,
                    reached: false,
                });
                DoorResult::Accepted
            }
            Operation::Release => {
                self.state.goal = None;
                self.state.handle_until = 0.0;
                DoorResult::Cancelled
            }
            Operation::Repair if !authorized => DoorResult::Denied,
            Operation::Repair if !self.closed(o) || o.occupied => DoorResult::NotClosed,
            Operation::Repair => {
                self.state.bolt_broken = false;
                self.state.latch_broken = false;
                self.state.bolt_damage_joules = 0.0;
                self.state.latch_damage_joules = 0.0;
                self.state.locked = false;
                self.state.latch_engaged = self.config.latch != LatchKind::None;
                self.state.jammed_until = 0.0;
                DoorResult::Accepted
            }
        };
        self.state.last_result = result;
        self.state.revision = self.state.revision.wrapping_add(1).max(1);
        result
    }
    pub fn tick(&mut self, o: Observation) -> Actuation {
        if let Some(goal) = self.state.goal {
            if !goal.reached
                && o.seconds - goal.started >= self.config.operation_timeout_seconds as f64
                && (goal.angle - o.angle).abs() * o.edge_radius > self.config.capture_metres
            {
                self.state.goal = None;
                self.state.jammed_until = o.seconds + self.config.retry_seconds as f64;
                self.state.last_result = DoorResult::Obstructed;
                self.state.revision = self.state.revision.wrapping_add(1).max(1);
            }
        }
        let before = (
            self.state.locked,
            self.state.latch_engaged,
            self.state.latch_broken,
            self.state.bolt_broken,
        );
        let impact = o.impact_joules.max(0.0);
        let elastic = 0.5 * self.config.retention_stiffness * o.angle * o.angle;
        let had_retention = self.state.locked && !self.state.bolt_broken
            || self.state.latch_engaged && !self.state.latch_broken;
        let mut remaining = impact;
        let mut unspent_elastic = elastic;
        if self.state.locked && !self.state.bolt_broken {
            let needed =
                (self.config.bolt_fracture_joules - self.state.bolt_damage_joules).max(0.0);
            self.state.bolt_damage_joules =
                (self.state.bolt_damage_joules + impact).min(self.config.bolt_fracture_joules);
            if impact + elastic >= needed {
                self.state.bolt_broken = true;
                self.state.bolt_damage_joules = self.config.bolt_fracture_joules;
                self.state.locked = false;
                remaining = (impact + unspent_elastic - needed).max(0.0);
                unspent_elastic = 0.0;
            } else {
                remaining = 0.0;
            }
        }
        if !self.state.locked && self.state.latch_engaged && !self.state.latch_broken {
            let needed =
                (self.config.latch_fracture_joules - self.state.latch_damage_joules).max(0.0);
            self.state.latch_damage_joules =
                (self.state.latch_damage_joules + remaining).min(self.config.latch_fracture_joules);
            if remaining + unspent_elastic >= needed {
                self.state.latch_broken = true;
                self.state.latch_damage_joules = self.config.latch_fracture_joules;
                self.state.latch_engaged = false;
                remaining = (remaining + unspent_elastic - needed).max(0.0);
            } else {
                remaining = 0.0;
            }
        }
        let released = if had_retention && !self.state.locked && !self.state.latch_engaged {
            remaining
        } else {
            0.0
        };
        if !self.state.locked
            && self.config.latch == LatchKind::Roller
            && o.load_nm.abs() > self.config.roller_release_nm
        {
            self.state.latch_engaged = false;
            self.state.handle_until = o.seconds + self.config.hold_seconds as f64;
        }
        if !self.state.latch_broken
            && self.config.latch != LatchKind::None
            && o.seconds >= self.state.handle_until
            && !self
                .state
                .goal
                .is_some_and(|g| g.angle != 0.0 && o.seconds < g.until)
            && self.closed(o)
        {
            self.state.latch_engaged = true;
        }
        if before
            != (
                self.state.locked,
                self.state.latch_engaged,
                self.state.latch_broken,
                self.state.bolt_broken,
            )
        {
            self.state.revision = self.state.revision.wrapping_add(1).max(1);
        }
        let mut a = Actuation {
            released_joules: released,
            ..Default::default()
        };
        if self.state.locked && !self.state.bolt_broken {
            a.retention_nm = self.config.bolt_torque_nm;
        } else if self.state.latch_engaged && !self.state.latch_broken {
            a.retention_nm = self.config.latch_torque_nm;
        }
        if a.retention_nm > 0.0 {
            a.retention_stiffness = self.config.retention_stiffness;
            return a;
        }
        if let Some(mut goal) = self.state.goal {
            if !goal.reached
                && (goal.angle - o.angle).abs() * o.edge_radius <= self.config.capture_metres
            {
                goal.reached = true;
                goal.until = o.seconds + self.config.hold_seconds as f64;
                self.state.goal = Some(goal);
            }
            if !goal.reached
                && o.seconds - goal.started >= self.config.operation_timeout_seconds as f64
                && (goal.angle - o.angle).abs() * o.edge_radius > self.config.capture_metres
            {
                self.state.goal = None;
                self.state.jammed_until = o.seconds + self.config.retry_seconds as f64;
                self.state.last_result = DoorResult::Obstructed;
                self.state.revision = self.state.revision.wrapping_add(1).max(1);
            } else if o.seconds >= goal.until {
                self.state.goal = None;
            } else {
                a.target = Some(goal.angle);
                a.motor_nm = self.config.operating_torque_nm;
                a.motor_stiffness = self.config.operating_stiffness;
            }
        }
        if o.occupied && a.target == Some(0.0) {
            a.target = None;
        }
        if a.target.is_none()
            && !o.occupied
            && self.config.closer_torque_nm > 0.0
            && !self.closed(o)
        {
            a.target = Some(0.0);
            a.motor_nm = self.config.closer_torque_nm;
            a.motor_stiffness = self.config.closer_stiffness;
        }
        a
    }
    pub fn restore(&mut self, state: MechanismState, o: Observation) -> Result<(), String> {
        if ![state.latch_damage_joules, state.bolt_damage_joules]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0)
            || !state.handle_until.is_finite()
            || !state.jammed_until.is_finite()
            || (state.locked
                && (state.bolt_broken
                    || o.angle.sin().abs() * o.edge_radius
                        > self.config.capture_metres
                            + self.config.bolt_torque_nm / self.config.retention_stiffness
                                * o.edge_radius))
            || (state.latch_engaged
                && (state.latch_broken
                    || o.angle.sin().abs() * o.edge_radius
                        > self.config.capture_metres
                            + self.config.latch_torque_nm / self.config.retention_stiffness
                                * o.edge_radius))
        {
            return Err("inconsistent saved door mechanism state".into());
        }
        self.state = state;
        self.state.goal = None;
        self.state.handle_until = o.seconds;
        self.state.jammed_until = 0.0;
        Ok(())
    }
}
