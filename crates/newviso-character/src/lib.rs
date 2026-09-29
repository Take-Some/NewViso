use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub type Vec3 = [f32; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepHit {
    pub fraction: f32,
    pub normal: Vec3,
    pub entity: Option<u64>,
}

pub trait CharacterCollisionWorld {
    fn sweep_sphere(&self, origin: Vec3, delta: Vec3, radius: f32) -> Option<SweepHit>;

    fn support_velocity(&self, _entity: u64) -> Vec3 {
        [0.0, 0.0, 0.0]
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CharacterConfig {
    pub radius: f32,
    pub half_height: f32,
    pub skin: f32,
    pub max_slope_degrees: f32,
    pub step_height: f32,
    pub ground_snap_distance: f32,
    pub max_iterations: u32,
}

impl Default for CharacterConfig {
    fn default() -> Self {
        Self {
            radius: 0.32,
            half_height: 0.58,
            skin: 0.015,
            max_slope_degrees: 50.0,
            step_height: 0.35,
            ground_snap_distance: 0.18,
            max_iterations: 5,
        }
    }
}

impl CharacterConfig {
    pub fn validate(self) -> Result<(), String> {
        if !self.radius.is_finite()
            || !(0.05..=5.0).contains(&self.radius)
            || !self.half_height.is_finite()
            || !(0.0..=10.0).contains(&self.half_height)
            || !self.skin.is_finite()
            || !(0.0..=0.25).contains(&self.skin)
            || !self.max_slope_degrees.is_finite()
            || !(0.0..89.0).contains(&self.max_slope_degrees)
            || !self.step_height.is_finite()
            || !(0.0..=2.0).contains(&self.step_height)
            || !self.ground_snap_distance.is_finite()
            || !(0.0..=2.0).contains(&self.ground_snap_distance)
            || !(1..=16).contains(&self.max_iterations)
        {
            return Err("invalid character controller config".to_owned());
        }
        Ok(())
    }

    fn walkable_normal_y(self) -> f32 {
        self.max_slope_degrees.to_radians().cos()
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct CharacterState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub grounded: bool,
    pub ground_normal: Vec3,
    pub support_entity: Option<u64>,
}

impl CharacterState {
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            velocity: [0.0; 3],
            grounded: false,
            ground_normal: [0.0, 1.0, 0.0],
            support_entity: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct CharacterMoveInput {
    pub desired_velocity: Vec3,
    pub dt: f32,
    #[serde(default)]
    pub allow_step: bool,
    #[serde(default = "default_true")]
    pub snap_to_ground: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct CharacterMoveResult {
    pub state: CharacterState,
    pub requested_delta: Vec3,
    pub applied_delta: Vec3,
    pub hit_wall: bool,
    pub stepped: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct CharacterController {
    config: CharacterConfig,
}

impl CharacterController {
    pub fn new(config: CharacterConfig) -> Result<Self, String> {
        config.validate()?;
        Ok(Self { config })
    }

    pub fn config(self) -> CharacterConfig {
        self.config
    }

    pub fn move_character<W: CharacterCollisionWorld>(
        self,
        world: &W,
        mut state: CharacterState,
        input: CharacterMoveInput,
    ) -> Result<CharacterMoveResult, String> {
        if !input.dt.is_finite()
            || !(0.0..=0.25).contains(&input.dt)
            || state
                .position
                .iter()
                .chain(state.velocity.iter())
                .any(|v| !v.is_finite())
            || input.desired_velocity.iter().any(|v| !v.is_finite())
        {
            return Err("invalid character movement state/input".to_owned());
        }

        let requested_delta = mul(input.desired_velocity, input.dt);
        let mut motion = requested_delta;
        if let Some(entity) = state.support_entity {
            motion = add(motion, mul(world.support_velocity(entity), input.dt));
        }

        let original = state.position;
        let mut position = state.position;
        let mut hit_wall = false;
        let mut stepped = false;

        if input.allow_step
            && state.grounded
            && self.config.step_height > 0.0
            && horizontal_length(motion) > 1.0e-5
        {
            if let Some(step_position) = self.try_step(world, position, motion) {
                position = step_position;
                motion = [0.0; 3];
                stepped = true;
            }
        }

        for _ in 0..self.config.max_iterations {
            if length(motion) <= 1.0e-6 {
                break;
            }
            let Some(hit) = self.sweep_capsule(world, position, motion) else {
                position = add(position, motion);
                break;
            };

            let travel = (hit.fraction - self.skin_fraction(motion)).clamp(0.0, 1.0);
            position = add(position, mul(motion, travel));
            let remaining = mul(motion, 1.0 - travel);
            if hit.normal[1] < self.config.walkable_normal_y() {
                hit_wall = true;
            }

            if hit.fraction <= 1.0e-5 {
                position = add(position, mul(hit.normal, self.config.skin.max(0.001)));
            }
            motion = reject(remaining, hit.normal);
            if dot(motion, remaining) <= 1.0e-8 {
                break;
            }
        }

        let mut ground_normal = [0.0, 1.0, 0.0];
        let mut grounded = false;
        let mut support_entity = None;

        if input.snap_to_ground {
            let probe = self.config.ground_snap_distance + self.config.skin;
            if probe > 0.0 {
                if let Some(hit) = self.sweep_capsule(world, position, [0.0, -probe, 0.0]) {
                    if hit.normal[1] >= self.config.walkable_normal_y() {
                        let drop = (probe * hit.fraction - self.config.skin).max(0.0);
                        position[1] -= drop;
                        grounded = true;
                        ground_normal = normalize(hit.normal);
                        support_entity = hit.entity;
                    }
                }
            }
        }

        let applied_delta = sub(position, original);
        let velocity = if input.dt > 1.0e-6 {
            mul(applied_delta, 1.0 / input.dt)
        } else {
            [0.0; 3]
        };

        state.position = position;
        state.velocity = velocity;
        state.grounded = grounded;
        state.ground_normal = ground_normal;
        state.support_entity = support_entity;

        Ok(CharacterMoveResult {
            state,
            requested_delta,
            applied_delta,
            hit_wall,
            stepped,
        })
    }

    fn try_step<W: CharacterCollisionWorld>(
        self,
        world: &W,
        position: Vec3,
        motion: Vec3,
    ) -> Option<Vec3> {
        let horizontal = [motion[0], 0.0, motion[2]];
        if self.sweep_capsule(world, position, horizontal).is_none() {
            return None;
        }

        let up = [0.0, self.config.step_height + self.config.skin, 0.0];
        if self.sweep_capsule(world, position, up).is_some() {
            return None;
        }
        let elevated = add(position, up);
        if self.sweep_capsule(world, elevated, horizontal).is_some() {
            return None;
        }
        let across = add(elevated, horizontal);
        let down_distance =
            self.config.step_height + self.config.ground_snap_distance + self.config.skin;
        let hit = self.sweep_capsule(world, across, [0.0, -down_distance, 0.0])?;
        if hit.normal[1] < self.config.walkable_normal_y() {
            return None;
        }
        let drop = (down_distance * hit.fraction - self.config.skin).max(0.0);
        Some([across[0], across[1] - drop, across[2]])
    }

    fn sweep_capsule<W: CharacterCollisionWorld>(
        self,
        world: &W,
        center: Vec3,
        delta: Vec3,
    ) -> Option<SweepHit> {
        let offset = [0.0, self.config.half_height, 0.0];
        let origins = [sub(center, offset), center, add(center, offset)];
        origins
            .into_iter()
            .filter_map(|origin| world.sweep_sphere(origin, delta, self.config.radius))
            .min_by(|a, b| a.fraction.total_cmp(&b.fraction))
    }

    fn skin_fraction(self, motion: Vec3) -> f32 {
        let len = length(motion);
        if len <= 1.0e-6 {
            0.0
        } else {
            (self.config.skin / len).min(0.25)
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CharacterRuntime {
    entries: std::collections::BTreeMap<String, (CharacterController, CharacterState)>,
}

impl CharacterRuntime {
    pub fn upsert(
        &mut self,
        id: impl Into<String>,
        config: CharacterConfig,
        position: Vec3,
    ) -> Result<(), String> {
        let id = id.into();
        if id.trim().is_empty() || id.len() > 128 {
            return Err("character id must be non-empty and <= 128 bytes".to_owned());
        }
        let controller = CharacterController::new(config)?;
        self.entries
            .entry(id)
            .and_modify(|entry| entry.0 = controller)
            .or_insert((controller, CharacterState::new(position)));
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> bool {
        self.entries.remove(id.trim()).is_some()
    }

    pub fn state(&self, id: &str) -> Option<CharacterState> {
        self.entries.get(id.trim()).map(|entry| entry.1)
    }

    pub fn set_state(&mut self, id: &str, state: CharacterState) -> Result<(), String> {
        let Some(entry) = self.entries.get_mut(id.trim()) else {
            return Err(format!("character '{id}' does not exist"));
        };
        entry.1 = state;
        Ok(())
    }

    pub fn step<W: CharacterCollisionWorld>(
        &mut self,
        id: &str,
        world: &W,
        input: CharacterMoveInput,
    ) -> Result<CharacterMoveResult, String> {
        let Some((controller, state)) = self.entries.get_mut(id.trim()) else {
            return Err(format!("character '{id}' does not exist"));
        };
        let result = controller.move_character(world, *state, input)?;
        *state = result.state;
        Ok(result)
    }

    pub fn runtime_state(&self) -> Value {
        json!({
            "characters": self.entries.iter().map(|(id, (controller, state))| {
                json!({
                    "id": id,
                    "config": controller.config(),
                    "state": state,
                })
            }).collect::<Vec<_>>()
        })
    }
}

fn reject(v: Vec3, normal: Vec3) -> Vec3 {
    let n = normalize(normal);
    sub(v, mul(n, dot(v, n).min(0.0)))
}
fn horizontal_length(v: Vec3) -> f32 {
    (v[0] * v[0] + v[2] * v[2]).sqrt()
}
fn length(v: Vec3) -> f32 {
    dot(v, v).sqrt()
}
fn normalize(v: Vec3) -> Vec3 {
    let len = length(v);
    if len <= 1.0e-8 {
        [0.0, 1.0, 0.0]
    } else {
        mul(v, 1.0 / len)
    }
}
fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(v: Vec3, s: f32) -> Vec3 {
    [v[0] * s, v[1] * s, v[2] * s]
}
fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use super::*;
    use newviso_collision::sweep_sphere_aabb_hit;

    struct BoxWorld {
        boxes: Vec<(u64, Vec3, Vec3, Vec3)>,
    }

    impl CharacterCollisionWorld for BoxWorld {
        fn sweep_sphere(&self, origin: Vec3, delta: Vec3, radius: f32) -> Option<SweepHit> {
            self.boxes
                .iter()
                .filter_map(|(entity, min, max, _)| {
                    sweep_sphere_aabb_hit(origin, delta, radius, *min, *max).map(|hit| SweepHit {
                        fraction: hit.fraction,
                        normal: hit.normal,
                        entity: Some(*entity),
                    })
                })
                .min_by(|a, b| a.fraction.total_cmp(&b.fraction))
        }

        fn support_velocity(&self, entity: u64) -> Vec3 {
            self.boxes
                .iter()
                .find(|item| item.0 == entity)
                .map(|item| item.3)
                .unwrap_or([0.0; 3])
        }
    }

    fn floor_world() -> BoxWorld {
        BoxWorld {
            boxes: vec![(1, [-10.0, -1.0, -10.0], [10.0, 0.0, 10.0], [0.0; 3])],
        }
    }

    #[test]
    fn ground_snap_marks_character_grounded() {
        let controller = CharacterController::new(CharacterConfig::default()).unwrap();
        let mut state = CharacterState::new([0.0, 0.92, 0.0]);
        state.grounded = true;
        let result = controller
            .move_character(
                &floor_world(),
                state,
                CharacterMoveInput {
                    desired_velocity: [0.0; 3],
                    dt: 1.0 / 60.0,
                    allow_step: true,
                    snap_to_ground: true,
                },
            )
            .unwrap();
        assert!(result.state.grounded);
        assert!(result.state.ground_normal[1] > 0.99);
    }

    #[test]
    fn wall_collision_slides_instead_of_tunneling() {
        let world = BoxWorld {
            boxes: vec![
                (1, [-10.0, -1.0, -10.0], [10.0, 0.0, 10.0], [0.0; 3]),
                (2, [1.0, 0.0, -10.0], [1.2, 3.0, 10.0], [0.0; 3]),
            ],
        };
        let controller = CharacterController::new(CharacterConfig::default()).unwrap();
        let mut state = CharacterState::new([0.0, 0.92, 0.0]);
        state.grounded = true;
        let result = controller
            .move_character(
                &world,
                state,
                CharacterMoveInput {
                    desired_velocity: [4.0, 0.0, 2.0],
                    dt: 0.5f32.min(0.25),
                    allow_step: false,
                    snap_to_ground: true,
                },
            )
            .unwrap();
        assert!(result.hit_wall);
        assert!(result.state.position[0] < 0.8);
        assert!(result.state.position[2] > 0.1);
    }

    #[test]
    fn low_step_can_be_climbed() {
        let world = BoxWorld {
            boxes: vec![
                (1, [-10.0, -1.0, -10.0], [10.0, 0.0, 10.0], [0.0; 3]),
                (2, [0.6, 0.0, -1.0], [1.4, 0.22, 1.0], [0.0; 3]),
            ],
        };
        let controller = CharacterController::new(CharacterConfig::default()).unwrap();
        let mut state = CharacterState::new([0.0, 0.92, 0.0]);
        state.grounded = true;
        let result = controller
            .move_character(
                &world,
                state,
                CharacterMoveInput {
                    desired_velocity: [3.0, 0.0, 0.0],
                    dt: 0.25,
                    allow_step: true,
                    snap_to_ground: true,
                },
            )
            .unwrap();
        assert!(result.stepped);
        assert!(result.state.position[0] > 0.5);
    }

    #[test]
    fn moving_support_velocity_is_inherited() {
        let world = BoxWorld {
            boxes: vec![(5, [-10.0, -1.0, -10.0], [10.0, 0.0, 10.0], [1.0, 0.0, 0.0])],
        };
        let controller = CharacterController::new(CharacterConfig::default()).unwrap();
        let mut state = CharacterState::new([0.0, 0.92, 0.0]);
        state.grounded = true;
        state.support_entity = Some(5);
        let result = controller
            .move_character(
                &world,
                state,
                CharacterMoveInput {
                    desired_velocity: [0.0; 3],
                    dt: 0.1,
                    allow_step: true,
                    snap_to_ground: true,
                },
            )
            .unwrap();
        assert!(result.state.position[0] > 0.09);
    }
}
