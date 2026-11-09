//! Validate authored anatomy against one installed skeleton, then sample one palette.
use super::*;
use newviso_endorphin::{math as em, ProtectionObservation, ProtectiveArmObservation};
pub(super) fn validate(
    profile: &ProtectionProfile,
    skeleton: &newviso_model::ModelSkeleton,
) -> Result<(), String> {
    let index = |name: &str| {
        skeleton
            .joints
            .iter()
            .position(|j| j.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                format!(
                    "protection skeleton '{}' has no joint '{name}'",
                    skeleton.name
                )
            })
    };
    let descendant = |mut child: usize, parent: usize| {
        for _ in 0..skeleton.joints.len() {
            let Some(p) = skeleton.joints[child].parent else {
                return false;
            };
            if p as usize == parent {
                return true;
            }
            child = p as usize;
        }
        false
    };
    index(&profile.head_joint)?;
    for a in &profile.arms {
        let u = index(&a.upper_joint)?;
        let e = index(&a.lower_joint)?;
        let h = index(&a.hand_joint)?;
        let c = index(&a.contact_joint)?;
        if !descendant(e, u) || !descendant(h, e) || (c != h && !descendant(c, h)) {
            return Err(
                "protective arm requires ordered shoulder/elbow/wrist/contact descendants".into(),
            );
        }
        for f in &a.open_hand {
            if !descendant(index(&f.joint)?, h) {
                return Err("protective hand pose joint lies outside the hand".into());
            }
        }
    }
    Ok(())
}
pub(super) fn observe(
    scene: &newviso_scene::Scene3dRuntime,
    entity: u64,
    profile: &ProtectionProfile,
    gravity: f32,
    capacities: &[f32],
) -> Result<ProtectionObservation, String> {
    let matrices = scene.entity_joint_world_matrices(entity, &profile.joints())?;
    let point = |name: &str| em::position(matrices[name]);
    let head = point(&profile.head_joint);
    let arms = profile
        .arms
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let wrist = point(&a.hand_joint);
            let rotation = em::rotation(matrices[&a.hand_joint]);
            ProtectiveArmObservation {
                shoulder: point(&a.upper_joint),
                elbow: point(&a.lower_joint),
                wrist,
                palm_offset_local: em::rotate(
                    em::inverse(rotation),
                    em::sub(point(&a.contact_joint), wrist),
                ),
                capacity: capacities.get(i).copied().unwrap_or(0.0),
            }
        })
        .collect::<Vec<_>>();
    let center = arms
        .iter()
        .fold([0.0; 3], |s, a| em::add(s, a.shoulder))
        .map(|v| v / arms.len() as f32);
    let up = em::unit(em::sub(head, center))?;
    let forward = if arms.len() == 2 {
        newviso_endorphin::protection_forward(up, arms[0].shoulder, arms[1].shoulder)
    } else {
        None
    }
    .or_else(|| {
        scene.entity_transform_matrix(entity).map(|m| {
            let q = em::rotation(m);
            em::rotate(q, profile.forward_axis_local)
        })
    })
    .ok_or("protection has no usable forward reference")?;
    Ok(ProtectionObservation {
        head,
        up,
        forward,
        gravity,
        arms,
    })
}
