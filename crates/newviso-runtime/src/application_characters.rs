use super::*;
use newviso_character::{
    CharacterCollisionWorld, CharacterConfig, CharacterMoveInput, CharacterRuntime, CharacterState,
    SweepHit,
};
use newviso_navigation::{
    DynamicObstacle, NavBuildConfig, NavTileSource, NavigationRuntime, OffMeshLink, PathStatus,
};
use newviso_steering::{choose_velocity, SteeringInput, SteeringNeighbor, SteeringObstacle};

const NAV_REQUESTS_PER_FRAME: usize = 8;
const NAV_TARGET_EPSILON: f32 = 0.12;
const FACING_SPEED_EPSILON: f32 = 0.05;
const MAX_FACING_TURN_RATE_RADIANS_PER_SECOND: f32 = std::f32::consts::TAU;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub(super) struct PhysicalCharacterBinding {
    pub actor_id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub controller: CharacterConfig,
    #[serde(default)]
    pub body_center_offset_y: Option<f32>,
    #[serde(default = "default_arrival_radius")]
    pub arrival_radius: f32,
    #[serde(default = "default_time_horizon")]
    pub steering_time_horizon: f32,
    #[serde(default = "default_separation_weight")]
    pub separation_weight: f32,
    #[serde(default = "default_gravity")]
    pub gravity: f32,
}

fn default_true() -> bool {
    true
}
fn default_arrival_radius() -> f32 {
    0.18
}
fn default_time_horizon() -> f32 {
    1.5
}
fn default_separation_weight() -> f32 {
    2.0
}
fn default_gravity() -> f32 {
    9.81
}

impl PhysicalCharacterBinding {
    fn validate(&self) -> Result<(), String> {
        if self.actor_id.trim().is_empty()
            || self.actor_id.len() > 128
            || !self.arrival_radius.is_finite()
            || !(0.01..=5.0).contains(&self.arrival_radius)
            || !self.steering_time_horizon.is_finite()
            || !(0.05..=30.0).contains(&self.steering_time_horizon)
            || !self.separation_weight.is_finite()
            || !(0.0..=100.0).contains(&self.separation_weight)
            || !self.gravity.is_finite()
            || !(0.0..=1000.0).contains(&self.gravity)
            || self
                .body_center_offset_y
                .is_some_and(|value| !value.is_finite() || !(-10.0..=10.0).contains(&value))
        {
            return Err("invalid physical world-actor character binding".to_owned());
        }
        self.controller.validate()
    }

    fn center_offset_y(&self) -> f32 {
        self.body_center_offset_y
            .unwrap_or(self.controller.radius + self.controller.half_height)
    }

    fn actor_to_center(&self, actor_position: [f32; 3]) -> [f32; 3] {
        [
            actor_position[0],
            actor_position[1] + self.center_offset_y(),
            actor_position[2],
        ]
    }

    fn center_to_actor(&self, center: [f32; 3]) -> [f32; 3] {
        [center[0], center[1] - self.center_offset_y(), center[2]]
    }
}

#[derive(Clone, Debug, Default)]
struct PhysicalPathState {
    target: Option<[f32; 3]>,
    request_id: Option<u64>,
    waypoints: Vec<[f32; 3]>,
    waypoint_index: usize,
    blocked: bool,
    last_status: Option<PathStatus>,
}

impl PhysicalPathState {
    fn invalidate(&mut self) {
        self.target = None;
        self.request_id = None;
        self.waypoints.clear();
        self.waypoint_index = 0;
        self.blocked = false;
        self.last_status = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CharacterAirPhase {
    Grounded,
    Jump,
    Fall,
}

#[derive(Debug, Default)]
pub(super) struct PhysicalCharacterRuntime {
    navigation: NavigationRuntime,
    characters: CharacterRuntime,
    bindings: BTreeMap<String, PhysicalCharacterBinding>,
    paths: BTreeMap<String, PhysicalPathState>,
    facing_headings: BTreeMap<String, f32>,
    active: BTreeSet<String>,
    air_phases: BTreeMap<String, CharacterAirPhase>,
    motion_events: Vec<(String, Value)>,
}

struct RuntimeCollisionWorld<'a> {
    physics: &'a PhysicsRuntime,
    scene_solids: &'a [([f32; 3], [f32; 3])],
}

impl CharacterCollisionWorld for RuntimeCollisionWorld<'_> {
    fn sweep_sphere(&self, origin: [f32; 3], delta: [f32; 3], radius: f32) -> Option<SweepHit> {
        self.physics
            .character_sweep_sphere(origin, delta, radius, None, self.scene_solids)
    }

    fn support_velocity(&self, entity: u64) -> [f32; 3] {
        self.physics.body_linear_velocity(entity)
    }
}

impl PhysicalCharacterRuntime {
    pub(super) fn bind(&mut self, binding: PhysicalCharacterBinding) -> Result<(), String> {
        binding.validate()?;
        let actor_id = binding.actor_id.trim().to_owned();
        self.bindings.insert(actor_id.clone(), binding);
        self.paths.entry(actor_id).or_default().invalidate();
        Ok(())
    }

    pub(super) fn unbind(&mut self, actor_id: &str) -> bool {
        let actor_id = actor_id.trim();
        self.characters.remove(actor_id);
        self.paths.remove(actor_id);
        self.facing_headings.remove(actor_id);
        self.air_phases.remove(actor_id);
        self.active.remove(actor_id);
        self.bindings.remove(actor_id).is_some()
    }

    pub(super) fn state(&self, actor_id: &str) -> Option<CharacterState> {
        self.characters.state(actor_id.trim())
    }

    pub(super) fn jump(&mut self, actor_id: &str, jump_speed: f32) -> Result<bool, String> {
        let actor_id = actor_id.trim();
        if actor_id.is_empty() {
            return Err("character jump actor id must not be empty".to_owned());
        }
        if !jump_speed.is_finite() || !(0.1..=50.0).contains(&jump_speed) {
            return Err("character jump speed must be finite and in 0.1..=50".to_owned());
        }
        let binding = self
            .bindings
            .get(actor_id)
            .ok_or_else(|| format!("physical character '{actor_id}' is not bound"))?
            .clone();
        if !binding.enabled {
            return Ok(false);
        }
        let Some(mut state) = self.characters.state(actor_id) else {
            return Ok(false);
        };
        if !state.grounded {
            return Ok(false);
        }

        let support_entity = state.support_entity;
        state.velocity[1] = jump_speed;
        state.grounded = false;
        state.support_entity = None;
        self.characters.set_state(actor_id, state)?;
        self.air_phases
            .insert(actor_id.to_owned(), CharacterAirPhase::Jump);

        self.motion_events.push((
            "character.jump".to_owned(),
            json!({
                "actor_id": actor_id,
                "entity": actor_id,
                "position": binding.center_to_actor(state.position),
                "velocity": state.velocity,
                "vertical_velocity": jump_speed,
                "horizontal_speed": (state.velocity[0] * state.velocity[0] + state.velocity[2] * state.velocity[2]).sqrt(),
                "grounded": false,
                "support_entity": support_entity
            }),
        ));
        self.motion_events.push((
            "character.locomotion.jump".to_owned(),
            json!({
                "actor_id": actor_id,
                "entity": actor_id,
                "position": binding.center_to_actor(state.position),
                "velocity": state.velocity,
                "vertical_velocity": jump_speed,
                "grounded": false
            }),
        ));
        Ok(true)
    }

    pub(super) fn drain_motion_events(&mut self) -> Vec<(String, Value)> {
        std::mem::take(&mut self.motion_events)
    }

    pub(super) fn configure_navigation(&mut self, config: NavBuildConfig) -> Result<(), String> {
        self.navigation.set_config(config)?;
        self.invalidate_paths();
        Ok(())
    }

    pub(super) fn upsert_navigation_tile(
        &mut self,
        source: NavTileSource,
    ) -> Result<usize, String> {
        let count = self.navigation.upsert_tile(source)?;
        self.invalidate_paths();
        Ok(count)
    }

    pub(super) fn remove_navigation_tile(&mut self, tile_id: &str) -> bool {
        let removed = self.navigation.remove_tile(tile_id);
        if removed {
            self.invalidate_paths();
        }
        removed
    }

    pub(super) fn upsert_obstacle(&mut self, obstacle: DynamicObstacle) -> Result<(), String> {
        self.navigation.upsert_obstacle(obstacle)?;
        self.invalidate_paths();
        Ok(())
    }

    pub(super) fn remove_obstacle(&mut self, id: &str) -> bool {
        let removed = self.navigation.remove_obstacle(id);
        if removed {
            self.invalidate_paths();
        }
        removed
    }

    pub(super) fn upsert_off_mesh_link(&mut self, link: OffMeshLink) -> Result<(), String> {
        self.navigation.upsert_off_mesh_link(link)?;
        self.invalidate_paths();
        Ok(())
    }

    pub(super) fn remove_off_mesh_link(&mut self, id: &str) -> bool {
        let removed = self.navigation.remove_off_mesh_link(id);
        if removed {
            self.invalidate_paths();
        }
        removed
    }

    fn invalidate_paths(&mut self) {
        for path in self.paths.values_mut() {
            path.invalidate();
        }
    }

    pub(super) fn interest_bounds(&self, world: &LivingWorldRuntime) -> Vec<([f32; 3], [f32; 3])> {
        let views = world.actor_runtime_views();
        self.bindings
            .values()
            .filter_map(|binding| {
                let actor = views.iter().find(|actor| actor.id == binding.actor_id)?;
                (binding.enabled && actor.enabled && actor.simulation_tier == "full").then(|| {
                    let r = 5.0_f32.max(binding.controller.radius * 4.0);
                    (
                        [
                            actor.position[0] - r,
                            actor.position[1] - 2.0,
                            actor.position[2] - r,
                        ],
                        [
                            actor.position[0] + r,
                            actor.position[1] + 4.0,
                            actor.position[2] + r,
                        ],
                    )
                })
            })
            .collect()
    }

    pub(super) fn release_inactive_authorities(&mut self, world: &mut LivingWorldRuntime) {
        let active = std::mem::take(&mut self.active);
        for actor_id in active {
            world.release_actor_external_motion(&actor_id);
        }
    }

    pub(super) fn tick(
        &mut self,
        dt: f32,
        world: &mut LivingWorldRuntime,
        physics: &PhysicsRuntime,
        scene_solids: &[([f32; 3], [f32; 3])],
    ) -> Result<(), String> {
        if !dt.is_finite() || dt < 0.0 {
            return Err(
                "physical character frame delta must be finite and non-negative".to_owned(),
            );
        }

        self.navigation.pump(NAV_REQUESTS_PER_FRAME);
        let views = world.actor_runtime_views();
        let view_map = views
            .iter()
            .map(|view| (view.id.as_str(), view))
            .collect::<BTreeMap<_, _>>();

        let previous_active = self.active.clone();
        let mut next_active = BTreeSet::new();

        // Snapshot neighbors before movement so the solver is order-independent within this frame.
        let neighbor_snapshot = self
            .bindings
            .values()
            .filter_map(|binding| {
                let actor = view_map.get(binding.actor_id.as_str())?;
                if !binding.enabled || !actor.enabled || actor.simulation_tier != "full" {
                    return None;
                }
                let state = self.characters.state(&binding.actor_id)?;
                Some((binding.actor_id.clone(), state, binding.controller.radius))
            })
            .collect::<Vec<_>>();

        let steering_obstacles = self
            .navigation
            .active_obstacles()
            .into_iter()
            .map(|obstacle| SteeringObstacle {
                position: [obstacle.center[0], obstacle.center[2]],
                radius: obstacle.radius,
            })
            .collect::<Vec<_>>();
        let actor_ids = self.bindings.keys().cloned().collect::<Vec<_>>();
        let collision_world = RuntimeCollisionWorld {
            physics,
            scene_solids,
        };

        for actor_id in actor_ids {
            let binding = self
                .bindings
                .get(&actor_id)
                .expect("actor id came from bindings")
                .clone();
            let Some(actor) = view_map.get(actor_id.as_str()).copied() else {
                self.characters.remove(&actor_id);
                self.paths.remove(&actor_id);
                self.facing_headings.remove(&actor_id);
                self.air_phases.remove(&actor_id);
                continue;
            };

            let physical = binding.enabled && actor.enabled && actor.simulation_tier == "full";

            if !physical {
                if previous_active.contains(&actor_id) {
                    world.release_actor_external_motion(&actor_id);
                }
                self.paths.entry(actor_id.clone()).or_default().invalidate();
                continue;
            }

            next_active.insert(actor_id.clone());
            let center = binding.actor_to_center(actor.position);
            if !previous_active.contains(&actor_id) || self.characters.state(&actor_id).is_none() {
                self.characters
                    .upsert(actor_id.clone(), binding.controller, center)?;
                let mut state = CharacterState::new(center);
                state.velocity = actor.velocity;
                self.characters.set_state(&actor_id, state)?;
            } else if let Some(mut state) = self.characters.state(&actor_id) {
                // LivingWorld remains authoritative across tier transitions/background travel.
                if horizontal_distance(binding.center_to_actor(state.position), actor.position)
                    > binding.arrival_radius * 4.0
                {
                    state.position = center;
                    state.velocity = actor.velocity;
                    state.grounded = false;
                    state.support_entity = None;
                    self.characters.set_state(&actor_id, state)?;
                }
            }

            let travel = actor.travel_target_position.zip(actor.travel_speed);
            let path = self.paths.entry(actor_id.clone()).or_default();

            if let Some((target, _)) = travel {
                if path
                    .target
                    .is_none_or(|old| distance(old, target) > NAV_TARGET_EPSILON)
                {
                    path.invalidate();
                    path.target = Some(target);
                }
            } else if path.target.is_some()
                || path.request_id.is_some()
                || !path.waypoints.is_empty()
            {
                path.invalidate();
            }

            if let Some(request_id) = path.request_id {
                if let Some(result) = self.navigation.take_result(request_id) {
                    path.request_id = None;
                    path.last_status = Some(result.status.clone());
                    if result.status == PathStatus::Found {
                        path.waypoints = result.waypoints;
                        path.waypoint_index = 0;
                        path.blocked = false;
                    } else {
                        path.waypoints.clear();
                        path.waypoint_index = 0;
                        path.blocked = true;
                    }
                }
            }

            let current_actor_position = self
                .characters
                .state(&actor_id)
                .map(|state| binding.center_to_actor(state.position))
                .unwrap_or(actor.position);

            if let Some((target, _)) = travel {
                if path.request_id.is_none() && path.waypoints.is_empty() && !path.blocked {
                    path.request_id = Some(self.navigation.request_path(
                        current_actor_position,
                        target,
                        binding.controller.radius,
                    )?);
                }
            }

            while path.waypoint_index < path.waypoints.len()
                && distance_xz(current_actor_position, path.waypoints[path.waypoint_index])
                    <= binding.arrival_radius
            {
                path.waypoint_index += 1;
            }

            let state = self
                .characters
                .state(&actor_id)
                .unwrap_or_else(|| CharacterState::new(center));
            let (desired_horizontal, max_speed) =
                if let Some(waypoint) = path.waypoints.get(path.waypoint_index).copied() {
                    let speed = travel.map(|(_, speed)| speed).unwrap_or(0.0);
                    let direction = normalize_xz([
                        waypoint[0] - current_actor_position[0],
                        0.0,
                        waypoint[2] - current_actor_position[2],
                    ]);
                    (
                        [direction[0] * speed, direction[2] * speed],
                        speed.max(0.01),
                    )
                } else {
                    ([0.0, 0.0], actor.travel_speed.unwrap_or(0.01).max(0.01))
                };
            let neighbors = neighbor_snapshot
                .iter()
                .filter(|(other_id, _, _)| other_id != &actor_id)
                .map(|(other_id, other, radius)| SteeringNeighbor {
                    id: stable_actor_id(other_id),
                    position: [other.position[0], other.position[2]],
                    velocity: [other.velocity[0], other.velocity[2]],
                    radius: *radius,
                })
                .collect::<Vec<_>>();

            let steering = choose_velocity(
                SteeringInput {
                    position: [state.position[0], state.position[2]],
                    velocity: [state.velocity[0], state.velocity[2]],
                    desired_velocity: desired_horizontal,
                    radius: binding.controller.radius,
                    max_speed,
                    time_horizon: binding.steering_time_horizon,
                    separation_weight: binding.separation_weight,
                },
                &neighbors,
                &steering_obstacles,
            )?;

            let steering_speed_sq = steering.velocity[0] * steering.velocity[0]
                + steering.velocity[1] * steering.velocity[1];
            if steering_speed_sq >= FACING_SPEED_EPSILON * FACING_SPEED_EPSILON {
                let desired_heading = steering.velocity[0].atan2(steering.velocity[1]);
                let heading = self
                    .facing_headings
                    .entry(actor_id.clone())
                    .or_insert(desired_heading);
                *heading = turn_towards_heading(
                    *heading,
                    desired_heading,
                    MAX_FACING_TURN_RATE_RADIANS_PER_SECOND * dt.min(0.25),
                );
            }

            let vertical_velocity = if state.grounded {
                0.0
            } else {
                state.velocity[1] - binding.gravity * dt
            };
            let result = self.characters.step(
                &actor_id,
                &collision_world,
                CharacterMoveInput {
                    desired_velocity: [
                        steering.velocity[0],
                        vertical_velocity,
                        steering.velocity[1],
                    ],
                    dt: dt.min(0.25),
                    allow_step: state.grounded,
                    // Never snap an ascending character back to the floor.
                    snap_to_ground: vertical_velocity <= 0.0,
                },
            )?;

            let actor_position = binding.center_to_actor(result.state.position);
            let next_phase = if result.state.grounded {
                CharacterAirPhase::Grounded
            } else if result.state.velocity[1] > 0.10 {
                CharacterAirPhase::Jump
            } else {
                CharacterAirPhase::Fall
            };
            match self.air_phases.get(&actor_id).copied() {
                None => {
                    // Establish initial contact/air state without synthesizing
                    // a landing event during first materialization.
                }
                Some(previous) if previous != next_phase => match next_phase {
                    CharacterAirPhase::Jump => {
                        self.motion_events.push((
                            "character.jump".to_owned(),
                            json!({
                                "actor_id": actor_id,
                                "entity": actor_id,
                                "position": actor_position,
                                "velocity": result.state.velocity,
                                "vertical_velocity": result.state.velocity[1],
                                "grounded": false
                            }),
                        ));
                        self.motion_events.push((
                            "character.locomotion.jump".to_owned(),
                            json!({
                                "actor_id": actor_id,
                                "entity": actor_id,
                                "position": actor_position,
                                "velocity": result.state.velocity,
                                "vertical_velocity": result.state.velocity[1],
                                "grounded": false
                            }),
                        ));
                    }
                    CharacterAirPhase::Fall => {
                        self.motion_events.push((
                            "character.fall".to_owned(),
                            json!({
                                "actor_id": actor_id,
                                "entity": actor_id,
                                "position": actor_position,
                                "velocity": result.state.velocity,
                                "vertical_velocity": result.state.velocity[1],
                                "grounded": false
                            }),
                        ));
                        self.motion_events.push((
                            "character.locomotion.fall".to_owned(),
                            json!({
                                "actor_id": actor_id,
                                "entity": actor_id,
                                "position": actor_position,
                                "velocity": result.state.velocity,
                                "vertical_velocity": result.state.velocity[1],
                                "grounded": false
                            }),
                        ));
                    }
                    CharacterAirPhase::Grounded => {
                        self.motion_events.push((
                            "character.land".to_owned(),
                            json!({
                                "actor_id": actor_id,
                                "entity": actor_id,
                                "position": actor_position,
                                "velocity": result.state.velocity,
                                "vertical_velocity": vertical_velocity,
                                "grounded": true,
                                "support_entity": result.state.support_entity,
                                "support_normal": result.state.ground_normal
                            }),
                        ));
                        self.motion_events.push((
                            "character.landing.impact".to_owned(),
                            json!({
                                "actor_id": actor_id,
                                "entity": actor_id,
                                "position": actor_position,
                                "velocity": result.state.velocity,
                                "vertical_velocity": vertical_velocity,
                                "grounded": true,
                                "support_entity": result.state.support_entity,
                                "support_normal": result.state.ground_normal
                            }),
                        ));
                    }
                },
                _ => {}
            }
            self.air_phases.insert(actor_id.clone(), next_phase);

            world.set_actor_external_motion(&actor_id, actor_position, result.state.velocity)?;
        }

        for actor_id in previous_active.difference(&next_active) {
            world.release_actor_external_motion(actor_id);
        }
        self.active = next_active;
        Ok(())
    }

    pub(super) fn presentation_facing_velocity(&self, actor_id: &str) -> Option<[f32; 3]> {
        let heading = *self.facing_headings.get(actor_id.trim())?;
        Some([heading.sin(), 0.0, heading.cos()])
    }

    pub(super) fn runtime_state(&self) -> Value {
        json!({
            "navigation": self.navigation.runtime_state(),
            "characters": self.characters.runtime_state(),
            "bindings": self.bindings.values().map(|binding| {
                let path = self.paths.get(&binding.actor_id);
                json!({
                    "actor_id": binding.actor_id,
                    "enabled": binding.enabled,
                    "controller": binding.controller,
                    "body_center_offset_y": binding.center_offset_y(),
                    "arrival_radius": binding.arrival_radius,
                    "steering_time_horizon": binding.steering_time_horizon,
                    "separation_weight": binding.separation_weight,
                    "gravity": binding.gravity,
                    "active": self.active.contains(&binding.actor_id),
                    "path_target": path.and_then(|path| path.target),
                    "path_request_id": path.and_then(|path| path.request_id),
                    "waypoints": path.map_or(0, |path| path.waypoints.len()),
                    "waypoint_index": path.map_or(0, |path| path.waypoint_index),
                    "blocked": path.is_some_and(|path| path.blocked),
                    "path_status": path.and_then(|path| path.last_status.clone()),
                    "facing_heading_degrees": self.facing_headings
                        .get(&binding.actor_id)
                        .map(|heading| heading.to_degrees()),
                    "air_phase": self.air_phases
                        .get(&binding.actor_id)
                        .map(|phase| match phase {
                            CharacterAirPhase::Grounded => "grounded",
                            CharacterAirPhase::Jump => "jump",
                            CharacterAirPhase::Fall => "fall",
                        }),
                })
            }).collect::<Vec<_>>()
        })
    }
}

impl EngineApplication {
    pub(super) fn bind_physical_character_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let actor_id = command
            .get("actor_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!(
                    "script command[{index}] character.world_actor.bind requires string 'actor_id'"
                )
            })?;
        let controller = command
            .get("controller")
            .filter(|value| !value.is_null())
            .map(|value| {
                serde_json::from_value::<CharacterConfig>(value.clone()).map_err(|error| {
                    format!(
                        "script command[{index}] character.world_actor.bind invalid controller: {error}"
                    )
                })
            })
            .transpose()?
            .unwrap_or_default();
        let enabled = command
            .get("enabled")
            .map(|value| {
                value.as_bool().ok_or_else(|| {
                    format!(
                        "script command[{index}] character.world_actor.bind 'enabled' must be boolean"
                    )
                })
            })
            .transpose()?
            .unwrap_or(true);
        let body_center_offset_y = command
            .get("body_center_offset_y")
            .filter(|value| !value.is_null())
            .map(|_| command_number(command, "body_center_offset_y", index))
            .transpose()?;
        let arrival_radius = command
            .get("arrival_radius")
            .map(|_| command_number(command, "arrival_radius", index))
            .transpose()?
            .unwrap_or_else(default_arrival_radius);
        let steering_time_horizon = command
            .get("steering_time_horizon")
            .map(|_| command_number(command, "steering_time_horizon", index))
            .transpose()?
            .unwrap_or_else(default_time_horizon);
        let separation_weight = command
            .get("separation_weight")
            .map(|_| command_number(command, "separation_weight", index))
            .transpose()?
            .unwrap_or_else(default_separation_weight);
        let gravity = command
            .get("gravity")
            .map(|_| command_number(command, "gravity", index))
            .transpose()?
            .unwrap_or_else(default_gravity);

        self.physical_characters.bind(PhysicalCharacterBinding {
            actor_id: actor_id.to_owned(),
            enabled,
            controller,
            body_center_offset_y,
            arrival_radius,
            steering_time_horizon,
            separation_weight,
            gravity,
        })
    }

    pub(super) fn configure_navigation_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let settings = command.get("settings").unwrap_or(command);
        let config =
            serde_json::from_value::<NavBuildConfig>(settings.clone()).map_err(|error| {
                format!("script command[{index}] navigation.configure invalid settings: {error}")
            })?;
        self.physical_characters.configure_navigation(config)
    }

    pub(super) fn upsert_navigation_obstacle_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
            format!("script command[{index}] navigation.obstacle.upsert requires string 'id'")
        })?;
        let enabled = command
            .get("enabled")
            .map(|value| {
                value.as_bool().ok_or_else(|| {
                    format!(
                        "script command[{index}] navigation.obstacle.upsert 'enabled' must be boolean"
                    )
                })
            })
            .transpose()?
            .unwrap_or(true);
        self.physical_characters.upsert_obstacle(DynamicObstacle {
            id: id.to_owned(),
            center: command_vec3(command, "center", index)?,
            radius: command_number(command, "radius", index)?,
            enabled,
        })
    }

    pub(super) fn upsert_off_mesh_link_from_script(
        &mut self,
        command: &Value,
        index: usize,
    ) -> Result<(), String> {
        let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
            format!("script command[{index}] navigation.off_mesh_link.upsert requires string 'id'")
        })?;
        let bool_field = |key: &str, default: bool| -> Result<bool, String> {
            command
                .get(key)
                .map(|value| {
                    value.as_bool().ok_or_else(|| {
                        format!(
                            "script command[{index}] navigation.off_mesh_link.upsert '{key}' must be boolean"
                        )
                    })
                })
                .transpose()
                .map(|value| value.unwrap_or(default))
        };
        self.physical_characters.upsert_off_mesh_link(OffMeshLink {
            id: id.to_owned(),
            start: command_vec3(command, "start", index)?,
            end: command_vec3(command, "end", index)?,
            bidirectional: bool_field("bidirectional", true)?,
            cost_scale: command
                .get("cost_scale")
                .map(|_| command_number(command, "cost_scale", index))
                .transpose()?
                .unwrap_or(1.0),
            kind: command
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("generic")
                .to_owned(),
            enabled: bool_field("enabled", true)?,
        })
    }

    pub(super) fn tick_physical_characters(&mut self, dt: f32) -> Result<(), String> {
        let interests = self.physical_characters.interest_bounds(&self.living_world);
        let scene_solids = if interests.is_empty() {
            Vec::new()
        } else {
            self.scene.physics_static_solid_aabbs_near(&interests)
        };

        let Some(physics) = self.physics.as_ref() else {
            self.physical_characters
                .release_inactive_authorities(&mut self.living_world);
            return Ok(());
        };

        self.physical_characters
            .tick(dt, &mut self.living_world, physics, &scene_solids)?;

        for (topic, payload) in self.physical_characters.drain_motion_events() {
            host::publish_event_json(&topic, "newviso.character", payload)?;
        }
        Ok(())
    }
}

fn stable_actor_id(id: &str) -> u64 {
    let hash = blake3::hash(id.as_bytes());
    u64::from_le_bytes(hash.as_bytes()[0..8].try_into().expect("eight bytes"))
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn distance_xz(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dz = a[2] - b[2];
    (dx * dx + dz * dz).sqrt()
}

fn horizontal_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    distance_xz(a, b)
}

fn wrap_angle_radians(value: f32) -> f32 {
    (value + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn turn_towards_heading(current: f32, target: f32, max_delta: f32) -> f32 {
    if !current.is_finite() || !target.is_finite() || !max_delta.is_finite() || max_delta <= 0.0 {
        return current;
    }
    let delta = wrap_angle_radians(target - current).clamp(-max_delta, max_delta);
    wrap_angle_radians(current + delta)
}

fn normalize_xz(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[2] * v[2]).sqrt();
    if len <= 1.0e-6 {
        [0.0, 0.0, 0.0]
    } else {
        [v[0] / len, 0.0, v[2] / len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use newviso_world::{
        WorldActorDesc, WorldNavEdgeDesc, WorldNavNodeDesc, WorldTravelRequestDesc,
    };

    #[test]
    fn facing_heading_turns_on_shortest_arc_without_instant_flip() {
        let current = 170.0f32.to_radians();
        let target = (-170.0f32).to_radians();
        let next = turn_towards_heading(current, target, 5.0f32.to_radians());
        assert!((wrap_angle_radians(next - current).to_degrees() - 5.0).abs() < 1.0e-4);

        let next = turn_towards_heading(0.0, std::f32::consts::PI, 15.0f32.to_radians());
        assert!((wrap_angle_radians(next).abs().to_degrees() - 15.0).abs() < 1.0e-4);
    }

    #[test]
    fn generic_character_jump_applies_impulse_and_emits_lifecycle_event() {
        let mut physical = PhysicalCharacterRuntime::default();
        let binding = PhysicalCharacterBinding {
            actor_id: "jumper".to_owned(),
            enabled: true,
            controller: CharacterConfig::default(),
            body_center_offset_y: None,
            arrival_radius: default_arrival_radius(),
            steering_time_horizon: default_time_horizon(),
            separation_weight: default_separation_weight(),
            gravity: default_gravity(),
        };
        physical.bind(binding.clone()).unwrap();
        physical
            .characters
            .upsert(
                "jumper".to_owned(),
                binding.controller,
                binding.actor_to_center([0.0, 0.0, 0.0]),
            )
            .unwrap();

        let mut state = physical.characters.state("jumper").unwrap();
        state.grounded = true;
        state.support_entity = Some(42);
        physical.characters.set_state("jumper", state).unwrap();

        assert!(physical.jump("jumper", 5.4).unwrap());
        let state = physical.characters.state("jumper").unwrap();
        assert!(!state.grounded);
        assert!((state.velocity[1] - 5.4).abs() < 1.0e-6);
        assert_eq!(state.support_entity, None);

        let events = physical.drain_motion_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, "character.jump");
        assert_eq!(events[1].0, "character.locomotion.jump");
        assert_eq!(events[0].1["actor_id"], "jumper");
        assert_eq!(events[0].1["support_entity"], 42);
    }

    #[test]
    fn binding_defaults_body_center_to_capsule_geometry() {
        let binding = PhysicalCharacterBinding {
            actor_id: "actor".to_owned(),
            enabled: true,
            controller: CharacterConfig::default(),
            body_center_offset_y: None,
            arrival_radius: default_arrival_radius(),
            steering_time_horizon: default_time_horizon(),
            separation_weight: default_separation_weight(),
            gravity: default_gravity(),
        };
        assert!((binding.center_offset_y() - 0.9).abs() < 1.0e-6);
        assert_eq!(
            binding.center_to_actor(binding.actor_to_center([1.0, 2.0, 3.0])),
            [1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn physical_character_follows_navmesh_and_completes_world_travel() {
        let mut physics = PhysicsRuntime::empty_for_tests();
        let floor = CollisionMeshResource {
            id: newviso_resource_runtime::AssetId(9001),
            name: "npc_floor".to_owned(),
            bounds: newviso_collision::CollisionBounds {
                min: [-1.0, 0.0, -2.0],
                max: [4.0, 0.0, 2.0],
            },
            vertices: vec![
                [-1.0, 0.0, -2.0],
                [4.0, 0.0, -2.0],
                [4.0, 0.0, 2.0],
                [-1.0, 0.0, 2.0],
            ],
            triangles: vec![[0, 2, 1], [0, 3, 2]],
            material_indices: vec![0, 0],
        };
        physics
            .install_streamed_collision(9001, &floor, [0.0; 3], [0.0; 3], [1.0; 3])
            .unwrap();

        let mut physical = PhysicalCharacterRuntime::default();
        physical
            .upsert_navigation_tile(physics.navigation_tile_source(9001).unwrap())
            .unwrap();
        physical
            .bind(PhysicalCharacterBinding {
                actor_id: "walker".to_owned(),
                enabled: true,
                controller: CharacterConfig::default(),
                body_center_offset_y: None,
                arrival_radius: 0.18,
                steering_time_horizon: 1.5,
                separation_weight: 2.0,
                gravity: 9.81,
            })
            .unwrap();

        let mut world = LivingWorldRuntime::default();
        world
            .configure_simulation(WorldSimulationPolicyDesc {
                transient_full_radius: 20.0,
                transient_reduced_radius: 40.0,
                full_interval_seconds: 0.05,
                reduced_interval_seconds: 0.1,
                background_interval_seconds: 0.25,
                ..WorldSimulationPolicyDesc::default()
            })
            .unwrap();
        world
            .upsert_actor(WorldActorDesc {
                id: "walker".to_owned(),
                kind: "generic".to_owned(),
                position: [0.0, 0.0, 0.0],
                group: None,
                channel: None,
                enabled: true,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
                state: Value::Null,
            })
            .unwrap();
        for (id, position) in [("start", [0.0, 0.0, 0.0]), ("goal", [3.0, 0.0, 0.0])] {
            world
                .upsert_nav_node(WorldNavNodeDesc {
                    id: id.to_owned(),
                    position,
                    tags: Vec::new(),
                    parameters: BTreeMap::new(),
                })
                .unwrap();
        }
        world
            .upsert_nav_edge(WorldNavEdgeDesc {
                id: "start-goal".to_owned(),
                from: "start".to_owned(),
                to: "goal".to_owned(),
                bidirectional: true,
                distance: None,
                cost_scale: 1.0,
                enabled: true,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        world
            .start_travel(WorldTravelRequestDesc {
                actor_id: "walker".to_owned(),
                start_node: Some("start".to_owned()),
                destination_node: "goal".to_owned(),
                speed: 1.5,
                mode: "walk".to_owned(),
                payload: Value::Null,
            })
            .unwrap();

        for _ in 0..80 {
            let observer = world
                .actor_runtime_views()
                .into_iter()
                .find(|view| view.id == "walker")
                .map(|view| view.position)
                .unwrap();
            world.tick_frame(0.05, &[observer]);
            physical.tick(0.05, &mut world, &physics, &[]).unwrap();
        }
        world.tick_frame(0.05, &[[3.0, 0.0, 0.0]]);

        let actor = world
            .actor_runtime_views()
            .into_iter()
            .find(|view| view.id == "walker")
            .unwrap();
        assert!((actor.position[0] - 3.0).abs() < 0.25, "{actor:?}");
        assert!(actor.position[1].abs() < 0.08, "{actor:?}");
        assert!(actor.travel_destination.is_none(), "{actor:?}");
    }

    #[test]
    fn external_world_motion_uses_actual_character_position() {
        let mut world = LivingWorldRuntime::default();
        world
            .upsert_actor(WorldActorDesc {
                id: "actor".to_owned(),
                kind: "generic".to_owned(),
                position: [0.0, 0.0, 0.0],
                group: None,
                channel: None,
                enabled: true,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
                state: Value::Null,
            })
            .unwrap();
        for (id, position) in [("a", [0.0, 0.0, 0.0]), ("b", [2.0, 0.0, 0.0])] {
            world
                .upsert_nav_node(WorldNavNodeDesc {
                    id: id.to_owned(),
                    position,
                    tags: Vec::new(),
                    parameters: BTreeMap::new(),
                })
                .unwrap();
        }
        world
            .upsert_nav_edge(WorldNavEdgeDesc {
                id: "ab".to_owned(),
                from: "a".to_owned(),
                to: "b".to_owned(),
                bidirectional: true,
                distance: None,
                cost_scale: 1.0,
                enabled: true,
                tags: Vec::new(),
                parameters: BTreeMap::new(),
            })
            .unwrap();
        world
            .start_travel(WorldTravelRequestDesc {
                actor_id: "actor".to_owned(),
                start_node: Some("a".to_owned()),
                destination_node: "b".to_owned(),
                speed: 1.0,
                mode: "walk".to_owned(),
                payload: Value::Null,
            })
            .unwrap();
        world
            .set_actor_external_motion("actor", [0.25, 0.0, 0.0], [1.0, 0.0, 0.0])
            .unwrap();
        let view = world
            .actor_runtime_views()
            .into_iter()
            .find(|view| view.id == "actor")
            .unwrap();
        assert_eq!(view.position, [0.25, 0.0, 0.0]);
        assert_eq!(view.velocity, [1.0, 0.0, 0.0]);
        assert!(view.external_motion_authority);
    }
}
