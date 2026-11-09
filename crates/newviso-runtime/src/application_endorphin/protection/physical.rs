//! Articulated bodies use the same reflex, actuated only by their bounded muscles.
use super::*;
use newviso_endorphin::{math as em, BodyPose, ProtectionObservation, ProtectiveArmObservation};
pub(crate) fn update(
    rig: &newviso_endorphin::RigState,
    profile: &ProtectionProfile,
    offsets: &[[f32; 3]],
    damage: &newviso_endorphin::BodyDamageState,
    state: &mut ProtectionState,
    radius: f32,
    physics: &PhysicsRuntime,
    scene_solids: &[([f32; 3], [f32; 3])],
    dt: f32,
    capacity: f32,
) {
    if offsets.len() != profile.arms.len() {
        state.reset();
        return;
    }
    let sample = |joint: &str| {
        rig.parts
            .iter()
            .find(|p| p.joint == joint)
            .and_then(|part| {
                physics.vehicle_body_state(part.entity).map(|p| {
                    let m = em::product(em::matrix(p.position, p.rotation), part.body_to_joint);
                    let point = em::position(m);
                    let velocity = em::add(
                        p.linear_velocity,
                        em::cross(p.angular_velocity, em::sub(point, p.position)),
                    );
                    (point, velocity)
                })
            })
    };
    let Some((head, head_velocity)) = sample(&profile.head_joint) else {
        state.reset();
        return;
    };
    let mut center_velocity = [0.0; 3];
    let arms = profile
        .arms
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let (shoulder, shoulder_velocity) = sample(&a.upper_joint)?;
            center_velocity = em::add(center_velocity, shoulder_velocity);
            Some(ProtectiveArmObservation {
                shoulder,
                elbow: sample(&a.lower_joint)?.0,
                wrist: sample(&a.hand_joint)?.0,
                palm_offset_local: offsets[i],
                capacity: capacity
                    * a.trauma_parts
                        .iter()
                        .map(|p| damage.part_capacity(p))
                        .fold(1.0, f32::min),
            })
        })
        .collect::<Option<Vec<_>>>();
    let Some(arms) = arms else {
        state.reset();
        return;
    };
    let center = arms
        .iter()
        .fold([0.0; 3], |s, a| em::add(s, a.shoulder))
        .map(|v| v / arms.len() as f32);
    let Some(up) = em::unit(em::sub(head, center)).ok() else {
        state.reset();
        return;
    };
    let forward = if arms.len() == 2 {
        newviso_endorphin::protection_forward(up, arms[0].shoulder, arms[1].shoulder)
    } else {
        None
    };
    let Some(forward) = forward else {
        state.reset();
        return;
    };
    let gravity = physics.gravity_magnitude();
    let obs = ProtectionObservation {
        head,
        up,
        forward,
        gravity,
        arms,
    };
    let reach = obs.arms.iter().map(|a| a.reach()).sum::<f32>() / obs.arms.len() as f32;
    // The chest sensor moves with the measured shoulders, including their
    // angular motion. Pelvis translation alone misses a tumbling upper body.
    center_velocity = center_velocity.map(|v| v / obs.arms.len() as f32);
    let sensor_speed = em::length(head_velocity).max(em::length(center_velocity));
    let horizon = profile.tuning.horizon(reach, gravity, sensor_speed);
    let radius = radius * profile.tuning.sensor_radius_fraction;
    let mut threat = None;
    for (point, v) in [(head, head_velocity), (center, center_velocity)] {
        if let Some(t) = physics.protective_threat(
            point,
            v,
            [0.0, -gravity, 0.0],
            radius,
            reach,
            horizon,
            rig.entities(),
            scene_solids,
            None,
        ) {
            if threat
                .as_ref()
                .is_none_or(|old: &newviso_endorphin::ImpactThreat| {
                    t.time_to_impact < old.time_to_impact
                })
            {
                threat = Some(t);
            }
        }
    }
    state.observe(profile, &obs, threat, dt);
}
pub(crate) fn targets(
    body: &crate::application_peds::physics::PedBody,
    physics: &PhysicsRuntime,
) -> Vec<Option<([f32; 4], f32)>> {
    let Some(rig) = &body.endorphin.ragdoll else {
        return Vec::new();
    };
    rig.protection_targets(
        &body.endorphin.protection_profile,
        &body.endorphin.control.protection,
        |e| {
            physics.vehicle_body_state(e).map(|p| BodyPose {
                position: p.position,
                rotation: p.rotation,
                linear_velocity: p.linear_velocity,
                angular_velocity: p.angular_velocity,
            })
        },
    )
}
