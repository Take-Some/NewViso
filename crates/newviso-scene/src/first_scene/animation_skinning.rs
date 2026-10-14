use super::*;
mod arm_ik;
mod pose_layers;
mod scheduling;
mod transforms;
mod sampling;
mod attachments;
mod frame_update;
mod workers;
#[cfg(test)]
mod tests;
use transforms::*;
use sampling::*;
pub(super) use workers::SkinningWorkerPool;
pub(super) use arm_ik::extend_bounds as arm_ik_bounds;
pub use arm_ik::SceneArmIkConstraint;
use newviso_model::{
    AnimationInterpolation, AnimationQuatKey, AnimationVec3Key, ModelAnimationClip, ModelSkeleton,
};
use newviso_resource_runtime::AssetId;
use std::{
    collections::BTreeMap,
    sync::{
        mpsc::{self, Receiver, Sender, TryRecvError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

#[derive(Clone, Debug)]
pub(super) struct SkinnedEntityAnimationState {
    source_model_id: AssetId,
    source_to_model: [f32; 16],
    model_to_source: [f32; 16],
    skeleton: Arc<ModelSkeleton>,
    derived_joint_drivers: Arc<[Option<DerivedJointDriver>]>,
    playback: Option<AnimationPlayback>,
    pose_layers: pose_layers::PoseLayers,
    animation_generation: u64,
    presentation_blend: Option<SkinningPresentationBlend>,
    presented_palette: Vec<[f32; 16]>,
    palette_blend: Option<PalettePresentationBlend>,
    foot_joint_indices: Option<[usize; 2]>,
    presented_feet: Option<[[f32; 3]; 2]>,
    presented_generation: Option<u64>,
    last_skinning_result_elapsed_seconds: Option<f64>,
}

#[derive(Clone, Copy, Debug)]
struct DerivedJointDriver {
    joint: u16,
    bind_offset: [f32; 16],
}

#[derive(Clone, Copy, Debug)]
struct AnimationMoverOrigin {
    position: [f32; 3],
    rotation_degrees: [f32; 3],
    scale: [f32; 3],
}

#[derive(Clone, Debug)]
struct AnimationPlayback {
    clip: Arc<ModelAnimationClip>,
    time_seconds: f32,
    playback_rate: f32,
    last_sample_elapsed_seconds: Option<f64>,
    transition_from: Option<AnimationTransition>,
    generation: u64,
    mover_origin: Option<AnimationMoverOrigin>,
}

#[derive(Clone, Debug)]
struct AnimationTransition {
    clip: Arc<ModelAnimationClip>,
    time_seconds: f32,
    playback_rate: f32,
    elapsed_seconds: f32,
    duration_seconds: f32,
}

struct SkinningJob {
    gpu_skinning: bool,
    arm_ik: Vec<SceneArmIkConstraint>,
    pose_layers: pose_layers::PoseLayerSample,
    foot_joint_indices: Option<[usize; 2]>,
    stable_id: u64,
    source_model_id: u64,
    generation: u64,
    skeleton: Arc<ModelSkeleton>,
    derived_joint_drivers: Arc<[Option<DerivedJointDriver>]>,
    clip: Arc<ModelAnimationClip>,
    sample_time: f32,
    transition_from: Option<AnimationTransitionSample>,
    source_to_model: [f32; 16],
    model_to_source: [f32; 16],
    bind_vertices: Arc<[AssetTriangleVertex]>,
    upload_from: usize,
    expected_vertex_count: usize,
}

#[derive(Clone, Debug)]
struct SkinningPresentationBlend {
    from_packed: Vec<f32>,
    to_packed: Vec<f32>,
    from_feet: Option<[[f32; 3]; 2]>,
    to_feet: Option<[[f32; 3]; 2]>,
    elapsed_seconds: f32,
    duration_seconds: f32,
}

#[derive(Clone, Debug)]
struct PalettePresentationBlend {
    from: Vec<[f32; 16]>,
    to: Vec<[f32; 16]>,
    from_feet: Option<[[f32; 3]; 2]>,
    to_feet: Option<[[f32; 3]; 2]>,
    elapsed_seconds: f32,
    duration_seconds: f32,
}

#[derive(Clone)]
struct AnimationTransitionSample {
    clip: Arc<ModelAnimationClip>,
    sample_time: f32,
    alpha: f32,
}

struct SkinningResult {
    stable_id: u64,
    source_model_id: u64,
    generation: u64,
    upload_from: usize,
    expected_vertex_count: usize,
    packed: Vec<f32>,
    palette: Vec<[f32; 16]>,
    gpu_skinning: bool,
    feet: Option<[[f32; 3]; 2]>,
}

enum SkinningCompletion {
    Ready(SkinningResult),
    Failed { stable_id: u64, error: String },
}

const SKINNING_WORKER_COUNT: usize = 4;
const MAX_SKINNING_IN_FLIGHT: usize = 8;
const MAX_SKINNING_RESULTS_APPLIED_PER_FRAME: usize = 4;
const MAX_ANIMATION_FRAME_DT: f32 = 0.1;
const MIN_SKIN_PRESENTATION_BLEND_SECONDS: f32 = 1.0 / 240.0;
const MAX_SKIN_PRESENTATION_BLEND_SECONDS: f32 = 0.1;

/// Stable skeleton-side foot index lookup; once per model, not every frame.
fn find_foot_joint_indices(skeleton: &ModelSkeleton) -> Option<[usize; 2]> {
    let mut indices = [None; 2];
    for (index, joint) in skeleton.joints.iter().enumerate() {
        let name = joint.name.to_ascii_lowercase();
        if !(name.contains("foot") || name.contains("ankle")) { continue; }
        if indices[0].is_none() && [
            "l_foot", "leftfoot", "foot_l", "l_ankle", "ankle_l", "left_foot",
        ].iter().any(|needle| name.contains(needle)) {
            indices[0] = Some(index);
        } else if indices[1].is_none() && [
            "r_foot", "rightfoot", "foot_r", "r_ankle", "ankle_r", "right_foot",
        ].iter().any(|needle| name.contains(needle)) {
            indices[1] = Some(index);
        }
        if indices[0].is_some() && indices[1].is_some() { break; }
    }
    Some([indices[0]?, indices[1]?])
}

/// Skinning job's palette, not latest simulation playback time, is the source
/// of audio contact poses. Matches globals_from_palette's space conversion.
fn skin_palette_feet_positions(
    skeleton: &ModelSkeleton,
    palette: &[[f32; 16]],
    source_to_model: [f32; 16],
    indices: [usize; 2],
) -> Option<[[f32; 3]; 2]> {
    let mut positions = [[0.0; 3]; 2];
    for (side, index) in indices.into_iter().enumerate() {
        let matrix = palette.get(index)?;
        let joint = skeleton.joints.get(index)?;
        let bind = inverse_affine(joint.inverse_bind_matrix)?;
        let global = mul_mat4(mul_mat4(*matrix, source_to_model), bind);
        positions[side] = [global[12], global[13], global[14]];
    }
    positions.iter().flatten().all(|v| v.is_finite()).then_some(positions)
}

fn interpolate_presented_feet(
    from: Option<[[f32; 3]; 2]>,
    to: Option<[[f32; 3]; 2]>,
    alpha: f32,
) -> Option<[[f32; 3]; 2]> {
    match (from, to) {
        (Some(from), Some(to)) => {
            if alpha <= 0.0 { return Some(from); }
            if alpha >= 1.0 { return Some(to); }
            Some(std::array::from_fn(|foot|
                std::array::from_fn(|axis| from[foot][axis]
                    + (to[foot][axis] - from[foot][axis]) * alpha)
            ))
        }
        (_, to) => to,
    }
}

impl SkinnedEntityAnimationState {
    pub(super) fn presented_skin_palette(&self) -> &[[f32; 16]] {
        &self.presented_palette
    }

    pub(super) fn new(
        source_model_id: AssetId,
        source_to_model: [f32; 16],
        skeleton: Arc<ModelSkeleton>,
    ) -> Result<Self, String> {
        let model_to_source = inverse_affine(source_to_model)
            .ok_or_else(|| "skinned model source_to_model transform is singular".to_owned())?;
        let derived_joint_drivers = build_derived_joint_drivers(&skeleton)?;
        let foot_joint_indices = find_foot_joint_indices(&skeleton);
        Ok(Self {
            source_model_id,
            source_to_model,
            model_to_source,
            presented_palette: vec![trs_matrix([0.;3], [0.,0.,0.,1.], [1.;3]); skeleton.joints.len()],
            palette_blend: None,
            skeleton,
            derived_joint_drivers,
            playback: None,
            pose_layers: Default::default(),
            animation_generation: 0,
            presentation_blend: None,
            foot_joint_indices,
            presented_feet: None,
            presented_generation: None,
            last_skinning_result_elapsed_seconds: None,
        })
    }
}

impl Scene3dRuntime {
    pub fn play_entity_animation(
        &mut self,
        stable_id: u64,
        clip: Arc<ModelAnimationClip>,
        playback_rate: f32,
        restart_if_same: bool,
        start_time_seconds: f32,
        blend_seconds: f32,
        apply_mover: bool,
    ) -> Result<bool, String> {
        if !playback_rate.is_finite() || playback_rate <= 0.0 {
            return Err(format!(
                "animation playback rate must be finite and > 0, got {playback_rate}"
            ));
        }
        if !clip.duration_seconds.is_finite() || clip.duration_seconds <= 0.0 {
            return Err(format!(
                "animation clip '{}' has invalid duration {}",
                clip.name, clip.duration_seconds
            ));
        }
        if !start_time_seconds.is_finite() || start_time_seconds < 0.0 {
            return Err(format!(
                "animation start time must be finite and >= 0, got {start_time_seconds}"
            ));
        }
        if !blend_seconds.is_finite() || blend_seconds < 0.0 {
            return Err(format!(
                "animation blend duration must be finite and >= 0, got {blend_seconds}"
            ));
        }
        let start_time_seconds = if clip.looping {
            start_time_seconds.rem_euclid(clip.duration_seconds)
        } else {
            start_time_seconds.min(clip.duration_seconds)
        };
        let mover_origin = if apply_mover && clip.mover.is_some() {
            self.entity_transform_values(stable_id)
                .map(|(position, rotation_degrees, scale)| AnimationMoverOrigin {
                    position,
                    rotation_degrees,
                    scale,
                })
        } else {
            None
        };
        let state = self
            .skinned_entities
            .get_mut(&stable_id)
            .ok_or_else(|| format!("scene entity {stable_id} is not an installed skinned model"))?;
        for track in &clip.tracks {
            if track.joint as usize >= state.skeleton.joints.len() {
                return Err(format!(
                    "animation clip '{}' track joint={} exceeds skeleton joints={}",
                    clip.name,
                    track.joint,
                    state.skeleton.joints.len()
                ));
            }
        }
        if state.pose_layers.is_physical() {
            return Ok(false);
        }
        if !restart_if_same
            && state
                .playback
                .as_ref()
                .is_some_and(|playback| playback.clip.name == clip.name)
        {
            if let Some(playback) = state.playback.as_mut() {
                playback.playback_rate = playback_rate;
                if playback.mover_origin.is_none() {
                    playback.mover_origin = mover_origin;
                }
            }
            return Ok(false);
        }
        state.animation_generation = state.animation_generation.wrapping_add(1).max(1);
        let generation = state.animation_generation;
        let transition_from = if blend_seconds > 0.0 {
            state.playback.as_ref().map(|previous| AnimationTransition {
                clip: previous.clip.clone(),
                time_seconds: previous.time_seconds,
                playback_rate: previous.playback_rate,
                elapsed_seconds: 0.0,
                duration_seconds: blend_seconds,
            })
        } else {
            None
        };
        state.playback = Some(AnimationPlayback {
            clip,
            time_seconds: start_time_seconds,
            playback_rate,
            last_sample_elapsed_seconds: None,
            transition_from,
            generation,
            mover_origin,
        });
        Ok(true)
    }

    pub fn stop_entity_animation(&mut self, stable_id: u64) -> bool {
        let Some(state) = self.skinned_entities.get_mut(&stable_id) else {
            return false;
        };
        state.animation_generation = state.animation_generation.wrapping_add(1).max(1);
        state.playback.take().is_some()
    }

    /// Adjust locomotion cadence in-place. Rebinding the clip would restart
    /// the gait, lose foot phase and repeatedly blend with the old pose.
    /// The character audio system supplies a strictly bounded calibrated rate.
    pub fn set_entity_locomotion_rate(
        &mut self,
        stable_id: u64,
        rate: f32,
    ) -> bool {
        if !rate.is_finite() || !(0.70..=1.30).contains(&rate) {
            return false;
        }
        let Some(state) = self.skinned_entities.get_mut(&stable_id) else {
            return false;
        };
        if state.pose_layers.is_physical() {
            return false;
        }
        let Some(playback) = state.playback.as_mut() else {
            return false;
        };
        if !playback.clip.looping { return false; }
        // Keep the clock and generation untouched. The renderer and contact
        // events now sample the same continuous animation phase.
        playback.playback_rate = rate;
        true
    }

    pub fn entity_animation_name(&self, stable_id: u64) -> Option<&str> {
        self.skinned_entities
            .get(&stable_id)?
            .playback
            .as_ref()
            .map(|playback| playback.clip.name.as_str())
    }

    /// Playback diagnostics use the same clock sampled by skeletal skinning.
    pub fn entity_animation_playback(&self, stable_id: u64) -> Option<serde_json::Value> {
        let state = self.skinned_entities.get(&stable_id)?;
        if state.pose_layers.is_physical() {
            return None;
        }
        let playback = state.playback.as_ref()?;
        Some(
            serde_json::json!({"clip": playback.clip.name, "time_seconds": playback.time_seconds,
            "duration_seconds": playback.clip.duration_seconds, "looping": playback.clip.looping,
            "playback_rate": playback.playback_rate}),
        )
    }

}

fn build_derived_joint_drivers(
    skeleton: &ModelSkeleton,
) -> Result<Arc<[Option<DerivedJointDriver>]>, String> {
    let mut names = BTreeMap::<&str, usize>::new();
    let mut bind_globals = Vec::<[f32; 16]>::with_capacity(skeleton.joints.len());

    for (index, joint) in skeleton.joints.iter().enumerate() {
        if names.insert(joint.name.as_str(), index).is_some() {
            return Err(format!(
                "skeleton '{}' contains duplicate joint name '{}'",
                skeleton.name, joint.name
            ));
        }
        let local = trs_matrix(
            joint.bind_translation,
            joint.bind_rotation,
            joint.bind_scale,
        );
        let global = match joint.parent {
            Some(parent) => {
                let parent = parent as usize;
                if parent >= index {
                    return Err(format!(
                        "skeleton '{}' joint={} has non-topological parent={}",
                        skeleton.name, index, parent
                    ));
                }
                mul_mat4(bind_globals[parent], local)
            }
            None => local,
        };
        bind_globals.push(global);
    }

    let mut drivers = vec![None; skeleton.joints.len()];
    for (index, joint) in skeleton.joints.iter().enumerate() {
        let driver_name = if let Some(base_name) = joint.name.strip_suffix("_helper") {
            Some(base_name)
        } else {
            // RSC7 ped rigs contain weighted roll bones that are siblings of
            // the deforming thigh rather than children of it. Parent-only
            // inheritance leaves those vertices in bind pose while the thigh
            // rotates, which visibly stretches the character.
            match joint.name.as_str() {
                // Lower body roll / muscle helpers are siblings of the actual
                // deforming limb in many RSC7 ped rigs. Driving only from their
                // authored parent (usually pelvis/thigh) leaves weighted verts
                // behind when the child limb rotates.
                "RB_L_ThighRoll" | "RB_L_BumRoll" => Some("SKEL_L_Thigh"),
                "RB_R_ThighRoll" | "RB_R_BumRoll" => Some("SKEL_R_Thigh"),
                "MH_L_Knee" => Some("SKEL_L_Calf"),
                "MH_R_Knee" => Some("SKEL_R_Calf"),

                // Same rule for upper-body roll/elbow helpers.
                "RB_L_ArmRoll" => Some("SKEL_L_UpperArm"),
                "RB_R_ArmRoll" => Some("SKEL_R_UpperArm"),
                "RB_L_ForeArmRoll" | "MH_L_Elbow" => Some("SKEL_L_Forearm"),
                "RB_R_ForeArmRoll" | "MH_R_Elbow" => Some("SKEL_R_Forearm"),
                _ => None,
            }
        };
        let Some(driver_name) = driver_name else {
            continue;
        };
        let Some(&driver_index) = names.get(driver_name) else {
            continue;
        };
        if driver_index >= index || driver_index > u16::MAX as usize {
            continue;
        }
        let bind_offset = mul_mat4(
            skeleton.joints[driver_index].inverse_bind_matrix,
            bind_globals[index],
        );
        drivers[index] = Some(DerivedJointDriver {
            joint: driver_index as u16,
            bind_offset,
        });
    }

    Ok(Arc::from(drivers))
}

type LocalPose = (Vec<[f32; 3]>, Vec<[f32; 4]>, Vec<[f32; 3]>, Vec<bool>);

