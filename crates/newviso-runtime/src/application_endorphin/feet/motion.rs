//! Contact ownership and continuous terrain correction, independent of actor kind.
use super::*;
use crate::application_physics::PhysicsRuntime;
use newviso_character::{FootMotionInput, FootMotionProfile, FootMotionState};
#[derive(Debug, Default)]
pub(super) struct ActorMotion {
    pub at: f64,
    pub legs: Vec<LegMotion>,
}
#[derive(Debug, Default)]
pub(super) struct LegMotion {
    servo: FootMotionState,
    anchor: Option<Anchor>,
    load: f32,
    previous_corners: Option<[[f32; 3]; 4]>,
}
#[derive(Debug)]
struct Anchor {
    surface: String,
    entity: u64,
    target: [f32; 3],
    rotation: [f32; 4],
    local: Option<([f32; 3], [f32; 4])>,
    previous: [f32; 3],
}
impl Anchor {
    fn capture(
        physics: &PhysicsRuntime,
        p: &placement::Placement,
        sole: NeutralSole,
        skin: f32,
    ) -> Self {
        let rotation = m::multiply(
            m::align_axis(m::rotate(p.rotation, sole.up), p.normal),
            p.rotation,
        );
        let point = sole_contact(sole, p.rotation, p.normal, skin);
        let contact = m::add(p.target, m::rotate(p.rotation, point));
        let target = m::sub(contact, m::rotate(rotation, point));
        let local = physics.vehicle_body_state(p.surface_entity).map(|body| {
            (
                m::rotate(m::inverse(body.rotation), m::sub(target, body.position)),
                m::multiply(m::inverse(body.rotation), rotation),
            )
        });
        Self {
            surface: p.surface_id.clone(),
            entity: p.surface_entity,
            target,
            rotation,
            local,
            previous: p.target,
        }
    }
    fn goal(
        &self,
        physics: &PhysicsRuntime,
        source_flat: [f32; 4],
        source: [f32; 4],
        sole: NeutralSole,
        skin: f32,
    ) -> ([f32; 3], [f32; 4]) {
        let (target, ground) = self.pose(physics);
        let rotation = m::multiply(ground, m::multiply(m::inverse(source_flat), source));
        let normal = m::rotate(ground, sole.up);
        let point = sole_contact(sole, rotation, normal, skin);
        (
            m::add(
                target,
                m::sub(m::rotate(ground, point), m::rotate(rotation, point)),
            ),
            rotation,
        )
    }
    fn pose(&self, physics: &PhysicsRuntime) -> ([f32; 3], [f32; 4]) {
        if let (Some((position, rotation)), Some(body)) =
            (self.local, physics.vehicle_body_state(self.entity))
        {
            (
                m::add(body.position, m::rotate(body.rotation, position)),
                m::multiply(body.rotation, rotation),
            )
        } else {
            (self.target, self.rotation)
        }
    }
}
pub(super) struct MotionPlacement {
    pub geometry: placement::Placement,
    pub target: [f32; 3],
    pub rotation: [f32; 4],
    pub corners: [[f32; 3]; 4],
    pub planted: bool,
    pub contact: f32,
    pub contact_point: Option<[f32; 3]>,
    pub contact_gap: Option<f32>,
    pub phase_weight: f32,
    pub landing_weight: f32,
    pub demand: f32,
    pub velocity: [f32; 3],
    pub held: bool,
    pub anchored_goal: bool,
    pub preview_goal: bool,
    pub collision_adjustment: f32,
    pub orientation_adjustment: f32,
    pub reach_projected: bool,
    pub source_lift: f32,
    pub horizontal_adjustment: f32,
}
impl EngineApplication {
    pub(super) fn foot_motion_profile(&mut self) -> Result<FootMotionProfile, String> {
        if let Some(profile) = self.endorphin.feet.motion_profile {
            return Ok(profile);
        }
        let asset = self
            .settings
            .variables
            .get("engine_foot_motion_profile")
            .and_then(Value::as_str)
            .unwrap_or("physics/endorphin/foot_motion.json");
        let profile: FootMotionProfile =
            serde_json::from_value(newviso_assets_client::AssetClient::new().json(asset)?)
                .map_err(|e| format!("invalid foot motion profile '{asset}': {e}"))?;
        profile.validate()?;
        self.endorphin.feet.motion_profile = Some(profile);
        Ok(profile)
    }
}
pub(super) fn plan(
    physics: &PhysicsRuntime,
    solids: &[(u64, [f32; 3], [f32; 3])],
    state: &mut LegMotion,
    profile: FootMotionProfile,
    input: FootMotionInput,
    cfg: newviso_character::CharacterConfig,
    sole: NeutralSole,
    hip: [f32; 3],
    anchor_hip: [f32; 3],
    knee: [f32; 3],
    rotation: [f32; 4],
    body: u64,
    reserve: f32,
    max_drop: f32,
) -> Option<MotionPlacement> {
    let lift = state.servo.lift(input);
    let authored_rotation = rotation;
    let source_flat = m::multiply(
        m::align_axis(m::rotate(rotation, sole.up), [0.0, 1.0, 0.0]),
        rotation,
    );
    let angle = 2.0
        * source_flat
            .iter()
            .zip(rotation)
            .map(|(a, b)| a * b)
            .sum::<f32>()
            .abs()
            .clamp(0.0, 1.0)
            .acos();
    let x = ((angle / profile.roll_compliance_degrees.to_radians()) - 1.0).clamp(0.0, 1.0);
    let preserve = x * x * (3.0 - 2.0 * x);
    let rotation = m::slerp(
        source_flat,
        authored_rotation,
        1.0 - state.load * (1.0 - preserve),
    );

    let foot = input.authored;
    let reach = input.leg_length;
    let demand = state
        .servo
        .demand(input, profile, true, state.anchor.is_some());
    if state.anchor.as_ref().is_some_and(|anchor| {
        let (target, anchor_rotation) = anchor.pose(physics);
        demand < profile.anchor_release_weight
            || m::length(m::sub(target, anchor_hip)) > reach * profile.anchor_reach_ratio
            || heading_dot(anchor_rotation, rotation, sole)
                < profile.anchor_turn_degrees.to_radians().cos()
    }) {
        state.anchor = None;
    }
    let held_pose = state
        .anchor
        .as_ref()
        .map(|anchor| anchor.goal(physics, source_flat, rotation, sole, cfg.skin));
    let velocity = state.servo.authored_velocity(input);
    let horizon = state.servo.prediction_seconds(input, profile) * 0.5;
    let mut advance = [velocity[0] * horizon, 0.0, velocity[2] * horizon];
    let distance = m::length(advance);
    if distance > reach {
        advance = advance.map(|v| v * reach / distance);
    }
    let ahead = m::add(foot, advance);
    let span = sole
        .corners
        .iter()
        .map(|p| m::length(*p))
        .fold(0.0, f32::max);
    let radius = cfg.radius + span;
    let base = physics.vehicle_body_state(body)?.position[1] - cfg.radius - cfg.half_height;
    let min = [
        foot[0].min(ahead[0]) - radius,
        base - max_drop - cfg.skin,
        foot[2].min(ahead[2]) - radius,
    ];
    let max = [
        foot[0].max(ahead[0]) + radius,
        base + cfg.step_height + cfg.skin,
        foot[2].max(ahead[2]) + radius,
    ];
    let mut normal: Option<placement::Placement> = None;
    let mut held: Option<placement::Placement> = None;
    let mut predicted: Option<placement::Placement> = None;
    physics.character_support_surfaces(min, max, body, solids, |id, surface, offset| {
        let valid = |candidate: &placement::Placement, lift: f32| {
            let ground = m::sub(candidate.contact_point, [0.0, lift, 0.0]);
            physics.character_foot_support_visible([ground; 4], body, solids, cfg.skin)
        };
        let better = |a: &placement::Placement, b: &placement::Placement| {
            a.horizontal_error < b.horizontal_error
                || (a.horizontal_error == b.horizontal_error && a.score < b.score)
        };
        if let Some(candidate) = placement::on_surface(
            surface, offset, id, sole, foot, rotation, hip, reach, lift, cfg, false,
        ) {
            if placement::root_interval(
                hip,
                candidate.target,
                m::length(m::sub(hip, knee)),
                m::length(m::sub(knee, foot)),
                max_drop,
                reserve,
            )
            .is_some()
                && valid(&candidate, lift)
                && normal.as_ref().is_none_or(|b| better(&candidate, b))
            {
                normal = Some(candidate);
            }
        }
        if let (Some(anchor), Some((target, rotation))) = (&state.anchor, held_pose) {
            if anchor.surface == format!("{id}:{}", surface.id) {
                if let Some(candidate) = placement::on_surface(
                    surface, offset, id, sole, target, rotation, hip, reach, 0.0, cfg, true,
                ) {
                    if candidate.horizontal_error <= cfg.skin * cfg.skin && valid(&candidate, 0.0) {
                        held = Some(candidate);
                    }
                }
            }
        }
        if distance > cfg.skin && demand < profile.anchor_engage_weight {
            if let Some(candidate) = placement::on_surface(
                surface, offset, id, sole, ahead, rotation, hip, reach, lift, cfg, false,
            ) {
                if valid(&candidate, lift)
                    && predicted.as_ref().is_none_or(|b| better(&candidate, b))
                {
                    predicted = Some(candidate);
                }
            }
        }
    });
    let is_held = held.is_some();
    let is_preview = !is_held && normal.is_none() && predicted.is_some();
    if !is_held {
        state.anchor = None;
    }
    let raw_rotation = rotation;
    let estimate = state.servo.estimate(foot);
    let query_corners = |position, rotation| {
        sole.corners
            .map(|p| m::add(m::rotate(rotation, p), position))
    };
    let best = held
        .or(normal)
        .or_else(|| predicted.clone())
        .unwrap_or_else(|| {
            let footprint =
                physics.character_foot_clearance(query_corners(foot, rotation), body, solids, cfg);
            let (delta, normal, entity, _) =
                footprint.unwrap_or((0.0, [0.0, 1.0, 0.0], None, foot));
            let mut target = foot;
            if footprint.is_some() {
                target[1] += delta + lift;
            }
            placement::Placement {
                surface_entity: entity.unwrap_or(0),
                complete: false,
                contact_point: m::sub(target, [0.0, lift, 0.0]),
                has_surface: footprint.is_some(),
                target,
                rotation: raw_rotation,
                corners: query_corners(target, raw_rotation),
                normal,
                surface_id: entity.map_or_else(String::new, |id| format!("contact:{id}")),
                score: 0.0,
                horizontal_error: 0.0,
            }
        });
    let mut goal = best.has_surface.then_some(best.target);
    if !is_held && demand < profile.anchor_engage_weight {
        if let Some(target) = &mut goal {
            if let Some(ahead) = &predicted {
                target[1] = target[1].max(ahead.target[1]);
            }
            let preview_rotation = predicted.as_ref().map_or(best.rotation, |p| p.rotation);
            let predicted_position = m::add(estimate, advance);
            if let Some((delta, _, _, _)) = physics.character_foot_clearance(
                query_corners(predicted_position, preview_rotation),
                body,
                solids,
                cfg,
            ) {
                // Anticipate clearance along the authored path, without moving
                // its swing horizontally towards the future tread.
                target[1] = target[1].max(predicted_position[1] + delta + lift);
            }
        }
    }
    let support_delta = state.anchor.as_mut().map_or([0.0; 3], |anchor| {
        let now = best.target;
        let delta = m::sub(now, anchor.previous);
        anchor.previous = now;
        delta
    });
    let mut output = state
        .servo
        .update(input, profile, goal, is_held, support_delta);
    // Preserve authored heel strike and toe-off. Sole flattening follows load.
    let proximity = |delta: f32| {
        let gap = (-delta).max(0.0);
        let x = (gap / cfg.skin.max(f32::EPSILON)).clamp(0.0, 1.0);
        1.0 - x * x * (3.0 - 2.0 * x)
    };
    let initial_rotation = m::slerp(
        rotation,
        best.rotation,
        if is_held {
            output.contact
        } else {
            output.landing_weight
        },
    );
    let mut rotation = initial_rotation;
    let mut clearance =
        physics.character_foot_clearance(query_corners(output.target, rotation), body, solids, cfg);
    let mut horizontal_adjustment = 0.0;
    let contact_origin = m::sub(
        best.contact_point,
        [0.0, if is_held { 0.0 } else { lift }, 0.0],
    );
    let plane_delta = query_corners(output.target, rotation)
        .iter()
        .map(|p| -m::dot(m::sub(*p, contact_origin), best.normal) / best.normal[1])
        .fold(f32::NEG_INFINITY, f32::max);
    if best.complete && clearance.is_some_and(|v| v.0 > plane_delta + cfg.skin) {
        let flat_rotation = m::multiply(
            m::align_axis(m::rotate(rotation, sole.up), best.normal),
            rotation,
        );
        let flat = physics.character_foot_clearance(
            query_corners(output.target, flat_rotation),
            body,
            solids,
            cfg,
        );
        if flat.is_some_and(|v| v.0 <= cfg.skin * 0.1) {
            // Find the smallest ankle adjustment that clears a nosing instead
            // of teleporting the whole foot upward by a riser's height.
            let mut low = 0.0;
            let mut high = 1.0;
            for _ in 0..cfg.max_iterations {
                let t = (low + high) * 0.5;
                let q = m::slerp(initial_rotation, flat_rotation, t);
                if physics
                    .character_foot_clearance(query_corners(output.target, q), body, solids, cfg)
                    .is_some_and(|v| v.0 <= cfg.skin * 0.1)
                {
                    high = t;
                } else {
                    low = t;
                }
            }
            rotation = m::slerp(initial_rotation, flat_rotation, high);
            clearance = physics.character_foot_clearance(
                query_corners(output.target, rotation),
                body,
                solids,
                cfg,
            );
        }
    }
    if let Some(previous) = state.previous_corners {
        let delta = physics.character_foot_sweep(
            previous,
            query_corners(output.target, rotation),
            body,
            solids,
            cfg.max_iterations,
        );
        output.target = m::add(output.target, delta);
        state.servo.project_horizontal(delta);
        state.servo.project_clearance(delta[1]);
        horizontal_adjustment = (delta[0] * delta[0] + delta[2] * delta[2]).sqrt();
        clearance = physics.character_foot_clearance(
            query_corners(output.target, rotation),
            body,
            solids,
            cfg,
        );
    }
    let qdot = initial_rotation
        .iter()
        .zip(rotation)
        .map(|(a, b)| a * b)
        .sum::<f32>();
    let orientation_adjustment = 2.0 * qdot.abs().clamp(0.0, 1.0).acos();
    let collision_adjustment = clearance.map_or(0.0, |v| v.0.max(0.0));
    output.target[1] += collision_adjustment;
    state.servo.project_clearance(collision_adjustment);
    let corners = query_corners(output.target, rotation);
    let tolerance = (profile.plant_tolerance_ratio * input.sole_length).min(cfg.skin * 0.1);
    let settled = corners
        .iter()
        .zip(best.corners)
        .all(|(a, b)| m::length(m::sub(*a, b)) <= tolerance);
    let flatness = corners
        .iter()
        .map(|p| m::dot(*p, best.normal))
        .fold(f32::NEG_INFINITY, f32::max)
        - corners
            .iter()
            .map(|p| m::dot(*p, best.normal))
            .fold(f32::INFINITY, f32::min);
    let planted = flatness <= tolerance
        && best.complete
        && best.has_surface
        && settled
        && output.contact >= profile.anchor_engage_weight
        && output.demand >= profile.anchor_engage_weight
        && lift <= cfg.skin;
    let contact = output.contact * clearance.map_or(0.0, |v| proximity(v.0 - collision_adjustment));
    let touching = contact >= profile.anchor_engage_weight;
    let contact_ready = touching
        && output.contact >= profile.anchor_engage_weight
        && output.demand >= profile.anchor_engage_weight
        && m::length(m::sub(output.target, best.target)) <= cfg.skin;
    if state.anchor.is_none()
        && best.complete
        && contact_ready
        && m::length(m::sub(output.target, anchor_hip)) <= reach * profile.anchor_reach_ratio
    {
        let mut anchor_pose = best.clone();
        anchor_pose.target = output.target;
        anchor_pose.rotation = rotation;
        anchor_pose.corners = corners;
        state.anchor = Some(Anchor::capture(physics, &anchor_pose, sole, cfg.skin));
    }
    state.load = contact;
    Some(MotionPlacement {
        geometry: best,
        source_lift: lift,
        horizontal_adjustment,
        target: output.target,
        rotation,
        corners,
        planted,
        contact,
        contact_point: clearance.map(|v| v.3),
        contact_gap: clearance.map(|v| (collision_adjustment - v.0).max(0.0)),
        phase_weight: output.contact,
        landing_weight: output.landing_weight,
        demand: output.demand,
        velocity: output.correction_velocity,
        held: is_held,
        anchored_goal: is_held,
        preview_goal: is_preview,
        collision_adjustment,
        orientation_adjustment,
        reach_projected: false,
    })
}

pub(super) fn project_reach(
    p: &mut MotionPlacement,
    hip: [f32; 3],
    upper: f32,
    lower: f32,
    reserve: f32,
) {
    let d = m::sub(p.target, hip);
    let distance = m::length(d);
    let minimum = (upper - lower).abs() + 0.002;
    let maximum = (upper + lower - reserve).max(minimum);
    let length = distance.clamp(minimum, maximum);
    let target = m::add(
        hip,
        m::unit(d).unwrap_or([0.0, -1.0, 0.0]).map(|v| v * length),
    );
    let delta = m::sub(target, p.target);
    p.target = target;
    p.corners = p.corners.map(|v| m::add(v, delta));
    p.planted = false;
    p.held = false;
    p.contact = 0.0;
    p.reach_projected = true;
}

fn heading_dot(a: [f32; 4], b: [f32; 4], sole: NeutralSole) -> f32 {
    let forward = m::sub(sole.corners[3], sole.corners[0]);
    let a = m::rotate(a, forward);
    let b = m::rotate(b, forward);
    let (Ok(a), Ok(b)) = (m::unit([a[0], 0.0, a[2]]), m::unit([b[0], 0.0, b[2]])) else {
        return 1.0;
    };
    m::dot(a, b).clamp(-1.0, 1.0)
}
#[cfg(test)]
mod heading_tests {
    use super::*;
    #[test]
    fn anchors_release_for_turning_without_mistaking_a_heel_roll_for_yaw() {
        let sole = NeutralSole {
            corners: [
                [-0.05, -0.1, -0.05],
                [0.05, -0.1, -0.05],
                [0.05, -0.1, 0.2],
                [-0.05, -0.1, 0.2],
            ],
            up: [0.0, 1.0, 0.0],
        };
        let identity = [0.0, 0.0, 0.0, 1.0];
        assert!(heading_dot(identity, m::axis_angle([0.6, 0.0, 0.0]), sole) > 0.999);
        assert!(
            heading_dot(
                identity,
                m::axis_angle([0.0, std::f32::consts::PI, 0.0]),
                sole
            ) < -0.999
        );
    }
}

fn sole_contact(sole: NeutralSole, rotation: [f32; 4], normal: [f32; 3], skin: f32) -> [f32; 3] {
    let heights = sole.corners.map(|p| m::dot(m::rotate(rotation, p), normal));
    let min = heights.into_iter().fold(f32::INFINITY, f32::min);
    let mut total = 0.0;
    let mut point = [0.0; 3];
    for (p, height) in sole.corners.into_iter().zip(heights) {
        let x = (1.0 - (height - min) / skin.max(f32::EPSILON)).clamp(0.0, 1.0);
        let weight = x * x * (3.0 - 2.0 * x);
        total += weight;
        for k in 0..3 {
            point[k] += p[k] * weight;
        }
    }
    point.map(|v| v / total)
}

#[cfg(test)]
mod pivot_tests {
    use super::*;
    #[test]
    fn a_loaded_heel_or_toe_roll_pivots_on_the_saved_sole_without_cancelling_animation() {
        let sole = NeutralSole {
            corners: [
                [-0.05, -0.1, -0.05],
                [0.05, -0.1, -0.05],
                [0.05, -0.1, 0.2],
                [-0.05, -0.1, 0.2],
            ],
            up: [0.0, 1.0, 0.0],
        };
        let physics = PhysicsRuntime::empty_for_tests();
        let target = [0.0, 0.1, 0.0];
        let identity = [0.0, 0.0, 0.0, 1.0];
        let corners = sole.corners.map(|p| m::add(p, target));
        let geometry = placement::Placement {
            surface_entity: 42,
            complete: true,
            has_surface: true,
            contact_point: corners[0],
            target,
            rotation: identity,
            corners,
            normal: [0.0, 1.0, 0.0],
            surface_id: "42:0".into(),
            score: 0.0,
            horizontal_error: 0.0,
        };
        let anchor = Anchor::capture(&physics, &geometry, sole, 0.015);
        for angle in [-0.4, 0.4] {
            let animated = m::axis_angle([angle, 0.0, 0.0]);
            let (position, rotation) = anchor.goal(&physics, identity, animated, sole, 0.015);
            let point = sole_contact(sole, rotation, [0.0, 1.0, 0.0], 0.015);
            let world = m::add(position, m::rotate(rotation, point));
            assert!(m::length(m::sub(world, m::add(target, point))) < 1e-6);
            assert!((world[1]).abs() < 1e-6);
            assert!(m::dot(m::rotate(rotation, sole.up), m::rotate(animated, sole.up)) > 0.99999);
            assert!((position[1] - target[1]).abs() > 0.01);
        }
    }
}

pub(super) fn constrain_reach_contact(
    physics: &PhysicsRuntime,
    solids: &[(u64, [f32; 3], [f32; 3])],
    state: &mut LegMotion,
    p: &mut MotionPlacement,
    hip: [f32; 3],
    upper: f32,
    lower: f32,
    reserve: f32,
    body: u64,
    cfg: newviso_character::CharacterConfig,
) {
    // Alternate anatomical reach and the swept contact half-spaces. Applying
    // either just once lets the other projection cross a riser again.
    for _ in 0..cfg.max_iterations * 4 {
        project_reach(p, hip, upper, lower, reserve);
        let delta = state.previous_corners.map_or([0.0; 3], |previous| {
            physics.character_foot_sweep(previous, p.corners, body, solids, cfg.max_iterations)
        });
        p.target = m::add(p.target, delta);
        p.corners = p.corners.map(|v| m::add(v, delta));
        p.horizontal_adjustment += (delta[0] * delta[0] + delta[2] * delta[2]).sqrt();
        if m::length(delta) < cfg.skin * 0.01 {
            break;
        }
    }
}
/// Transport the authored bend plane with the retargeted leg axis. Reusing the
/// old world-space knee as a pole can reverse a deeply flexed stair knee.
pub(super) fn bend_pole(
    hip: [f32; 3],
    knee: [f32; 3],
    foot: [f32; 3],
    target: [f32; 3],
    posture: f32,
) -> [f32; 3] {
    let actual = m::add(hip, [0.0, posture, 0.0]);
    let rotation = m::align_axis(m::sub(foot, hip), m::sub(target, actual));
    m::add(actual, m::rotate(rotation, m::sub(knee, hip)))
}
#[cfg(test)]
mod bend_tests {
    use super::*;
    #[test]
    fn lifted_stair_goal_transports_the_source_bend_instead_of_inverting_the_knee() {
        let hip = [0.0, 1.0, 0.0];
        let knee = [0.0, 0.55, -0.1];
        let foot = [0.0, 0.1, 0.0];
        let target = [0.0, 0.7, -0.4];
        let pole = bend_pole(hip, knee, foot, target, -0.05);
        let actual = [0.0, 0.95, 0.0];
        let axis = m::unit(m::sub(target, actual)).unwrap();
        let off = m::sub(pole, actual);
        let bend = m::sub(off, axis.map(|v| v * m::dot(off, axis)));
        assert!(bend[1] > 0.05 && bend[2] < 0.0);
        assert!((m::length(off) - m::length(m::sub(knee, hip))).abs() < 1e-5);
    }
}
pub(super) fn commit(
    state: &mut LegMotion,
    placement: &MotionPlacement,
    authored: [f32; 3],
) -> [f32; 3] {
    // Retain late posture/reach corrections in the continuous controller.
    // Otherwise becoming reachable switches straight back to the unprojected
    // target, even though both individual poses satisfy anatomy.
    let delta = m::sub(placement.target, state.servo.estimate(authored));
    state.servo.project_horizontal(delta);
    state.servo.project_clearance(delta[1]);
    state.previous_corners = Some(placement.corners);
    state.servo.constraint_response()
}
