use super::*;
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
    animation_generation: u64,
    presentation_blend: Option<SkinningPresentationBlend>,
    last_skinning_result_elapsed_seconds: Option<f64>,
}

#[derive(Clone, Copy, Debug)]
struct DerivedJointDriver {
    joint: u16,
    bind_offset: [f32; 16],
}

#[derive(Clone, Debug)]
struct AnimationPlayback {
    clip: Arc<ModelAnimationClip>,
    time_seconds: f32,
    playback_rate: f32,
    last_sample_elapsed_seconds: Option<f64>,
    transition_from: Option<AnimationTransition>,
    generation: u64,
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
}

enum SkinningCompletion {
    Ready(SkinningResult),
    Failed { stable_id: u64, error: String },
}

const SKINNING_WORKER_COUNT: usize = 4;
const MAX_SKINNING_IN_FLIGHT: usize = 8;
const MAX_SKINNING_RESULTS_APPLIED_PER_FRAME: usize = 4;
const DEFAULT_ANIMATION_BLEND_SECONDS: f32 = 0.18;
const MAX_ANIMATION_FRAME_DT: f32 = 0.1;
const MIN_SKIN_PRESENTATION_BLEND_SECONDS: f32 = 1.0 / 240.0;
const MAX_SKIN_PRESENTATION_BLEND_SECONDS: f32 = 0.1;

pub(super) struct SkinningWorkerPool {
    task_tx: Option<Sender<SkinningJob>>,
    result_rx: Receiver<SkinningCompletion>,
    workers: Vec<JoinHandle<()>>,
}

impl std::fmt::Debug for SkinningWorkerPool {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SkinningWorkerPool")
            .field("worker_count", &self.workers.len())
            .finish_non_exhaustive()
    }
}

impl SkinningWorkerPool {
    pub(super) fn new() -> Result<Self, String> {
        let (task_tx, task_rx) = mpsc::channel::<SkinningJob>();
        let (result_tx, result_rx) = mpsc::channel::<SkinningCompletion>();
        let task_rx = Arc::new(Mutex::new(task_rx));
        let mut workers = Vec::with_capacity(SKINNING_WORKER_COUNT);

        for index in 0..SKINNING_WORKER_COUNT {
            let task_rx = Arc::clone(&task_rx);
            let result_tx = result_tx.clone();
            let worker = thread::Builder::new()
                .name(format!("newviso-skin-{index}"))
                .spawn(move || loop {
                    let job = {
                        let receiver = task_rx.lock().expect("skinning task queue poisoned");
                        receiver.recv()
                    };
                    let Ok(job) = job else {
                        return;
                    };
                    let stable_id = job.stable_id;
                    let completion = match run_skinning_job(job) {
                        Ok(result) => SkinningCompletion::Ready(result),
                        Err(error) => SkinningCompletion::Failed { stable_id, error },
                    };
                    if result_tx.send(completion).is_err() {
                        return;
                    }
                })
                .map_err(|error| format!("failed to spawn skinning worker: {error}"))?;
            workers.push(worker);
        }

        Ok(Self {
            task_tx: Some(task_tx),
            result_rx,
            workers,
        })
    }

    fn submit(&self, job: SkinningJob) -> Result<(), String> {
        self.task_tx
            .as_ref()
            .ok_or_else(|| "skinning worker pool is shutting down".to_owned())?
            .send(job)
            .map_err(|_| "skinning worker pool disconnected".to_owned())
    }

    fn try_recv(&self) -> Result<SkinningCompletion, TryRecvError> {
        self.result_rx.try_recv()
    }
}

impl Drop for SkinningWorkerPool {
    fn drop(&mut self) {
        self.task_tx.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl SkinnedEntityAnimationState {
    pub(super) fn new(
        source_model_id: AssetId,
        source_to_model: [f32; 16],
        skeleton: Arc<ModelSkeleton>,
    ) -> Result<Self, String> {
        let model_to_source = inverse_affine(source_to_model)
            .ok_or_else(|| "skinned model source_to_model transform is singular".to_owned())?;
        let derived_joint_drivers = build_derived_joint_drivers(&skeleton)?;
        Ok(Self {
            source_model_id,
            source_to_model,
            model_to_source,
            skeleton,
            derived_joint_drivers,
            playback: None,
            animation_generation: 0,
            presentation_blend: None,
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
        if !restart_if_same
            && state
                .playback
                .as_ref()
                .is_some_and(|playback| playback.clip.name == clip.name)
        {
            if let Some(playback) = state.playback.as_mut() {
                playback.playback_rate = playback_rate;
            }
            return Ok(false);
        }
        state.animation_generation = state.animation_generation.wrapping_add(1).max(1);
        let generation = state.animation_generation;
        let transition_from = state.playback.as_ref().map(|previous| AnimationTransition {
            clip: previous.clip.clone(),
            time_seconds: previous.time_seconds,
            playback_rate: previous.playback_rate,
            elapsed_seconds: 0.0,
            duration_seconds: DEFAULT_ANIMATION_BLEND_SECONDS,
        });
        state.playback = Some(AnimationPlayback {
            clip,
            time_seconds: 0.0,
            playback_rate,
            last_sample_elapsed_seconds: None,
            transition_from,
            generation,
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

    pub fn entity_animation_name(&self, stable_id: u64) -> Option<&str> {
        self.skinned_entities
            .get(&stable_id)?
            .playback
            .as_ref()
            .map(|playback| playback.clip.name.as_str())
    }

    pub(super) fn update_skinned_animations(&mut self, dt: f32) -> Result<(), String> {
        if !dt.is_finite() || dt <= 0.0 || self.skinned_entities.is_empty() {
            return Ok(());
        }

        let frame_dt = dt.clamp(0.0, MAX_ANIMATION_FRAME_DT);
        for state in self.skinned_entities.values_mut() {
            let Some(playback) = state.playback.as_mut() else {
                continue;
            };
            playback.time_seconds = advance_clip_time(
                &playback.clip,
                playback.time_seconds,
                frame_dt * playback.playback_rate,
            );
            let transition_finished = if let Some(transition) = playback.transition_from.as_mut() {
                transition.time_seconds = advance_clip_time(
                    &transition.clip,
                    transition.time_seconds,
                    frame_dt * transition.playback_rate,
                );
                transition.elapsed_seconds += frame_dt;
                transition.elapsed_seconds >= transition.duration_seconds
            } else {
                false
            };
            if transition_finished {
                playback.transition_from = None;
            }
        }

        let render_frame = self.frame_index;
        let visible = self
            .frame_plan
            .visible_entities
            .iter()
            .map(|id| id.0)
            .collect::<std::collections::BTreeSet<_>>();
        let camera_position = self.camera.position;
        let elapsed_seconds = self.world.process_elapsed_seconds();

        let mut applied = 0usize;
        while applied < MAX_SKINNING_RESULTS_APPLIED_PER_FRAME {
            let result = match self.animation_skinning_pool.try_recv() {
                Ok(SkinningCompletion::Ready(result)) => result,
                Ok(SkinningCompletion::Failed { stable_id, error }) => {
                    self.animation_skinning_in_flight.remove(&stable_id);
                    host_runtime::warn(
                        "newviso.animation",
                        format!("background skinning job failed entity={stable_id}: {error}"),
                    );
                    continue;
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            };
            self.animation_skinning_in_flight.remove(&result.stable_id);

            let Some(mesh) = self.asset_meshes.get(&result.stable_id) else {
                continue;
            };
            if mesh.source_model_id.0 != result.source_model_id
                || mesh.first_vertex as usize * FLOATS_PER_VERTEX != result.upload_from
                || mesh.vertex_count as usize != result.expected_vertex_count
            {
                // Entity was removed/reinstalled while the worker was running.
                continue;
            }

            let result_is_current = self
                .skinned_entities
                .get(&result.stable_id)
                .and_then(|state| state.playback.as_ref())
                .is_some_and(|playback| playback.generation == result.generation);
            if !result_is_current {
                // A clip switch or stop happened while this immutable worker job
                // was running. Never upload a stale pose over the new animation.
                continue;
            }

            if result.packed.len() != result.expected_vertex_count * FLOATS_PER_VERTEX {
                return Err(format!(
                    "skinned packed vertex count changed entity={} actual_floats={} expected_floats={}",
                    result.stable_id,
                    result.packed.len(),
                    result.expected_vertex_count * FLOATS_PER_VERTEX
                ));
            }
            let compact_first_vertex = mesh.skinned_first_vertex.ok_or_else(|| {
                format!(
                    "skinned entity {} has no compact GPU vertex range",
                    result.stable_id
                )
            })? as usize;
            let compact_start = compact_first_vertex
                .checked_mul(FLOATS_PER_VERTEX)
                .ok_or_else(|| "compact skinned vertex offset overflow".to_owned())?;
            let compact_end = compact_start
                .checked_add(result.packed.len())
                .ok_or_else(|| "compact skinned vertex upload range overflow".to_owned())?;
            if compact_end > self.skinned_vertex_data.len() {
                return Err(format!(
                    "skinned compact upload exceeds resident stream entity={} range={}..{} floats={}",
                    result.stable_id,
                    compact_start,
                    compact_end,
                    self.skinned_vertex_data.len()
                ));
            }
            let current_packed = self.skinned_vertex_data[compact_start..compact_end].to_vec();
            let state = self
                .skinned_entities
                .get_mut(&result.stable_id)
                .ok_or_else(|| format!("skinned entity {} disappeared", result.stable_id))?;
            let blend_duration = state
                .last_skinning_result_elapsed_seconds
                .map(|last| (elapsed_seconds - last).max(0.0) as f32)
                .unwrap_or(0.0)
                .clamp(
                    MIN_SKIN_PRESENTATION_BLEND_SECONDS,
                    MAX_SKIN_PRESENTATION_BLEND_SECONDS,
                );
            let first_result = state.last_skinning_result_elapsed_seconds.is_none();
            state.last_skinning_result_elapsed_seconds = Some(elapsed_seconds);

            if first_result {
                self.skinned_vertex_data[compact_start..compact_end]
                    .copy_from_slice(&result.packed);
                state.presentation_blend = None;
                for ranges in &mut self.skinned_dirty_ranges {
                    ranges.push((compact_start, compact_end));
                }
            } else {
                state.presentation_blend = Some(SkinningPresentationBlend {
                    from_packed: current_packed,
                    to_packed: result.packed,
                    elapsed_seconds: 0.0,
                    duration_seconds: blend_duration,
                });
            }
            applied = applied.saturating_add(1);
        }

        // Worker results are intentionally sparse. Interpolate their packed
        // skinned-vertex snapshots at render cadence so animation presentation
        // stays smooth without re-running full CPU skinning every frame.
        {
            let asset_meshes = &self.asset_meshes;
            let skinned_entities = &mut self.skinned_entities;
            let skinned_vertex_data = &mut self.skinned_vertex_data;
            let skinned_dirty_ranges = &mut self.skinned_dirty_ranges;

            for stable_id in visible.iter().copied() {
                let Some(state) = skinned_entities.get_mut(&stable_id) else {
                    continue;
                };
                let Some(blend) = state.presentation_blend.as_mut() else {
                    continue;
                };
                let Some(mesh) = asset_meshes.get(&stable_id) else {
                    state.presentation_blend = None;
                    continue;
                };
                let compact_first_vertex = mesh.skinned_first_vertex.ok_or_else(|| {
                    format!("skinned entity {stable_id} has no compact GPU vertex range")
                })? as usize;
                let compact_start = compact_first_vertex
                    .checked_mul(FLOATS_PER_VERTEX)
                    .ok_or_else(|| "compact skinned vertex offset overflow".to_owned())?;
                let compact_end = compact_start
                    .checked_add(blend.to_packed.len())
                    .ok_or_else(|| "compact skinned presentation range overflow".to_owned())?;
                if compact_end > skinned_vertex_data.len()
                    || blend.from_packed.len() != blend.to_packed.len()
                {
                    return Err(format!(
                        "skinned presentation range mismatch entity={stable_id} range={}..{} from={} to={} resident={}",
                        compact_start,
                        compact_end,
                        blend.from_packed.len(),
                        blend.to_packed.len(),
                        skinned_vertex_data.len()
                    ));
                }

                blend.elapsed_seconds += frame_dt;
                let alpha = if blend.duration_seconds > 0.0 {
                    (blend.elapsed_seconds / blend.duration_seconds).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                for (dst, (from, to)) in skinned_vertex_data[compact_start..compact_end]
                    .iter_mut()
                    .zip(blend.from_packed.iter().zip(blend.to_packed.iter()))
                {
                    *dst = *from + (*to - *from) * alpha;
                }
                for ranges in skinned_dirty_ranges.iter_mut() {
                    ranges.push((compact_start, compact_end));
                }

                if alpha >= 1.0 {
                    state.presentation_blend = None;
                }
            }
        }

        // Skinning is presentation-only. Build candidates from the last
        // visibility plan and schedule immutable jobs onto persistent workers.
        // The main thread never waits for a skinning result.
        let mut candidates = Vec::<(f64, f32, u64, f64)>::new();
        for stable_id in visible.iter().copied() {
            if self.animation_skinning_in_flight.contains(&stable_id) {
                continue;
            }
            let Some(state) = self.skinned_entities.get(&stable_id) else {
                continue;
            };
            let Some(playback) = state.playback.as_ref() else {
                continue;
            };
            let Some(entity) = self.world.entity(SceneEntityId(stable_id)) else {
                continue;
            };
            let delta = entity.transform.position.sub(camera_position);
            let distance_sq = delta.dot(delta);

            let target_hz = if distance_sq <= 20.0 * 20.0 {
                30.0_f64
            } else if distance_sq <= 45.0 * 45.0 {
                20.0
            } else if distance_sq <= 90.0 * 90.0 {
                12.0
            } else {
                6.0
            };
            let target_interval = 1.0 / target_hz;
            let age = playback
                .last_sample_elapsed_seconds
                .map(|last| (elapsed_seconds - last).max(0.0))
                .unwrap_or(f64::INFINITY);
            if age.is_finite() && age + 1.0e-6 < target_interval {
                continue;
            }

            // Higher overdue ratio wins. Distance is the tie breaker, so close
            // actors get first service without starving actors that have waited.
            let urgency = if age.is_finite() {
                age / target_interval
            } else {
                f64::INFINITY
            };
            candidates.push((urgency, distance_sq, stable_id, elapsed_seconds));
        }
        candidates.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| a.2.cmp(&b.2))
        });

        let available =
            MAX_SKINNING_IN_FLIGHT.saturating_sub(self.animation_skinning_in_flight.len());
        let mut submitted = 0usize;
        for (_, _, stable_id, elapsed_seconds) in candidates.into_iter().take(available) {
            let (
                source_model_id,
                source_to_model,
                model_to_source,
                skeleton,
                derived_joint_drivers,
                clip,
                sample_time,
                transition_from,
                generation,
            ) =
                {
                    let state = self
                        .skinned_entities
                        .get_mut(&stable_id)
                        .ok_or_else(|| format!("skinned entity {stable_id} disappeared"))?;
                    let playback = state
                        .playback
                        .as_mut()
                        .ok_or_else(|| format!("skinned entity {stable_id} lost playback"))?;
                    playback.last_sample_elapsed_seconds = Some(elapsed_seconds);
                    let transition_from = playback.transition_from.as_ref().map(|transition| {
                        AnimationTransitionSample {
                            clip: transition.clip.clone(),
                            sample_time: transition.time_seconds,
                            alpha: if transition.duration_seconds > 0.0 {
                                (transition.elapsed_seconds / transition.duration_seconds)
                                    .clamp(0.0, 1.0)
                            } else {
                                1.0
                            },
                        }
                    });
                    (
                        state.source_model_id,
                        state.source_to_model,
                        state.model_to_source,
                        state.skeleton.clone(),
                        state.derived_joint_drivers.clone(),
                        playback.clip.clone(),
                        playback.time_seconds,
                        transition_from,
                        playback.generation,
                    )
                };

            let bind_vertices = self
                .asset_model_cache
                .get(&source_model_id.0)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "skinned entity {} lost cached source geometry model={}",
                        stable_id, source_model_id.0
                    )
                })?;
            let mesh = self.asset_meshes.get(&stable_id).ok_or_else(|| {
                format!("skinned entity {} lost installed render mesh", stable_id)
            })?;
            let job = SkinningJob {
                stable_id,
                source_model_id: source_model_id.0,
                generation,
                skeleton,
                derived_joint_drivers,
                clip,
                sample_time,
                transition_from,
                source_to_model,
                model_to_source,
                bind_vertices,
                upload_from: mesh.first_vertex as usize * FLOATS_PER_VERTEX,
                expected_vertex_count: mesh.vertex_count as usize,
            };
            self.animation_skinning_pool.submit(job)?;
            self.animation_skinning_in_flight.insert(stable_id);
            submitted = submitted.saturating_add(1);
        }

        if render_frame % 60 == 0 {
            host_runtime::info(
                "newviso.perf.animation",
                format!(
                    "frame={} visible_skinned={} submitted={} applied={} in_flight={} total_skinned_entities={}",
                    render_frame,
                    visible
                        .iter()
                        .filter(|stable_id| self.skinned_entities.contains_key(stable_id))
                        .count(),
                    submitted,
                    applied,
                    self.animation_skinning_in_flight.len(),
                    self.skinned_entities.len()
                ),
            );
        }
        Ok(())
    }
}

fn run_skinning_job(job: SkinningJob) -> Result<SkinningResult, String> {
    let palette = if let Some(transition) = job.transition_from.as_ref() {
        build_blended_skin_palette(
            &job.skeleton,
            &job.derived_joint_drivers,
            &transition.clip,
            transition.sample_time,
            &job.clip,
            job.sample_time,
            transition.alpha,
            job.source_to_model,
            job.model_to_source,
        )?
    } else {
        build_skin_palette(
            &job.skeleton,
            &job.derived_joint_drivers,
            &job.clip,
            job.sample_time,
            job.source_to_model,
            job.model_to_source,
        )?
    };
    let packed = skin_vertices_to_packed(&job.bind_vertices, &palette)?;
    if packed.len() != job.expected_vertex_count * FLOATS_PER_VERTEX {
        return Err(format!(
            "skinned packed vertex count changed entity={} actual_floats={} expected_floats={}",
            job.stable_id,
            packed.len(),
            job.expected_vertex_count * FLOATS_PER_VERTEX
        ));
    }
    Ok(SkinningResult {
        stable_id: job.stable_id,
        source_model_id: job.source_model_id,
        generation: job.generation,
        upload_from: job.upload_from,
        expected_vertex_count: job.expected_vertex_count,
        packed,
    })
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
        let Some(base_name) = joint.name.strip_suffix("_helper") else {
            continue;
        };
        let Some(&driver_index) = names.get(base_name) else {
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

fn advance_clip_time(clip: &ModelAnimationClip, time_seconds: f32, delta_seconds: f32) -> f32 {
    let duration = clip.duration_seconds.max(1.0e-6);
    let next = time_seconds + delta_seconds.max(0.0);
    if clip.looping {
        next.rem_euclid(duration)
    } else {
        next.min(duration)
    }
}

type LocalPose = (Vec<[f32; 3]>, Vec<[f32; 4]>, Vec<[f32; 3]>, Vec<bool>);

fn sample_clip_local_pose(
    skeleton: &ModelSkeleton,
    clip: &ModelAnimationClip,
    time_seconds: f32,
) -> Result<LocalPose, String> {
    let mut translations = skeleton
        .joints
        .iter()
        .map(|joint| joint.bind_translation)
        .collect::<Vec<_>>();
    let mut rotations = skeleton
        .joints
        .iter()
        .map(|joint| joint.bind_rotation)
        .collect::<Vec<_>>();
    let mut scales = skeleton
        .joints
        .iter()
        .map(|joint| joint.bind_scale)
        .collect::<Vec<_>>();
    let mut tracked = vec![false; skeleton.joints.len()];

    for track in &clip.tracks {
        let joint = track.joint as usize;
        if joint >= skeleton.joints.len() {
            return Err(format!(
                "animation clip '{}' references joint={} skeleton_joints={}",
                clip.name,
                joint,
                skeleton.joints.len()
            ));
        }
        tracked[joint] = true;
        translations[joint] = sample_vec3_clip(
            &track.translations,
            track.translation_interpolation,
            time_seconds,
            clip.looping,
            clip.duration_seconds,
            translations[joint],
        );
        rotations[joint] = sample_quat_clip(
            &track.rotations,
            track.rotation_interpolation,
            time_seconds,
            clip.looping,
            clip.duration_seconds,
            rotations[joint],
        );
        scales[joint] = sample_vec3_clip(
            &track.scales,
            track.scale_interpolation,
            time_seconds,
            clip.looping,
            clip.duration_seconds,
            scales[joint],
        );
    }

    Ok((translations, rotations, scales, tracked))
}

fn build_palette_from_local_pose(
    skeleton: &ModelSkeleton,
    derived_joint_drivers: &[Option<DerivedJointDriver>],
    translations: &[[f32; 3]],
    rotations: &[[f32; 4]],
    scales: &[[f32; 3]],
    tracked: &[bool],
    source_to_model: [f32; 16],
    model_to_source: [f32; 16],
) -> Result<Vec<[f32; 16]>, String> {
    if derived_joint_drivers.len() != skeleton.joints.len()
        || translations.len() != skeleton.joints.len()
        || rotations.len() != skeleton.joints.len()
        || scales.len() != skeleton.joints.len()
        || tracked.len() != skeleton.joints.len()
    {
        return Err(format!(
            "skeleton '{}' animation pose dimensions do not match joints={}",
            skeleton.name,
            skeleton.joints.len()
        ));
    }

    let mut globals = Vec::with_capacity(skeleton.joints.len());
    let mut palette = Vec::with_capacity(skeleton.joints.len());
    for (index, joint) in skeleton.joints.iter().enumerate() {
        let local = trs_matrix(translations[index], rotations[index], scales[index]);
        let global = if !tracked[index] {
            if let Some(driver) = derived_joint_drivers[index] {
                let driver_index = driver.joint as usize;
                if driver_index >= index {
                    return Err(format!(
                        "skeleton '{}' derived helper joint={} has non-topological driver={}",
                        skeleton.name, index, driver_index
                    ));
                }
                mul_mat4(globals[driver_index], driver.bind_offset)
            } else {
                match joint.parent {
                    Some(parent) => {
                        let parent = parent as usize;
                        if parent >= index {
                            return Err(format!(
                                "skeleton '{}' joint={} has non-topological parent={}",
                                skeleton.name, index, parent
                            ));
                        }
                        mul_mat4(globals[parent], local)
                    }
                    None => local,
                }
            }
        } else {
            match joint.parent {
                Some(parent) => {
                    let parent = parent as usize;
                    if parent >= index {
                        return Err(format!(
                            "skeleton '{}' joint={} has non-topological parent={}",
                            skeleton.name, index, parent
                        ));
                    }
                    mul_mat4(globals[parent], local)
                }
                None => local,
            }
        };
        globals.push(global);
        let source_palette = mul_mat4(global, joint.inverse_bind_matrix);
        palette.push(mul_mat4(
            mul_mat4(source_to_model, source_palette),
            model_to_source,
        ));
    }
    Ok(palette)
}

fn build_skin_palette(
    skeleton: &ModelSkeleton,
    derived_joint_drivers: &[Option<DerivedJointDriver>],
    clip: &ModelAnimationClip,
    time_seconds: f32,
    source_to_model: [f32; 16],
    model_to_source: [f32; 16],
) -> Result<Vec<[f32; 16]>, String> {
    let (translations, rotations, scales, tracked) =
        sample_clip_local_pose(skeleton, clip, time_seconds)?;
    build_palette_from_local_pose(
        skeleton,
        derived_joint_drivers,
        &translations,
        &rotations,
        &scales,
        &tracked,
        source_to_model,
        model_to_source,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_blended_skin_palette(
    skeleton: &ModelSkeleton,
    derived_joint_drivers: &[Option<DerivedJointDriver>],
    from_clip: &ModelAnimationClip,
    from_time_seconds: f32,
    to_clip: &ModelAnimationClip,
    to_time_seconds: f32,
    alpha: f32,
    source_to_model: [f32; 16],
    model_to_source: [f32; 16],
) -> Result<Vec<[f32; 16]>, String> {
    let (from_t, from_r, from_s, from_tracked) =
        sample_clip_local_pose(skeleton, from_clip, from_time_seconds)?;
    let (to_t, to_r, to_s, to_tracked) =
        sample_clip_local_pose(skeleton, to_clip, to_time_seconds)?;
    let alpha = alpha.clamp(0.0, 1.0);

    let mut translations = Vec::with_capacity(skeleton.joints.len());
    let mut rotations = Vec::with_capacity(skeleton.joints.len());
    let mut scales = Vec::with_capacity(skeleton.joints.len());
    let mut tracked = Vec::with_capacity(skeleton.joints.len());
    for joint in 0..skeleton.joints.len() {
        translations.push(lerp_vec3(from_t[joint], to_t[joint], alpha));
        rotations.push(slerp_quat(
            normalize_quat(from_r[joint]),
            normalize_quat(to_r[joint]),
            alpha,
        ));
        scales.push(lerp_vec3(from_s[joint], to_s[joint], alpha));
        tracked.push(from_tracked[joint] || to_tracked[joint]);
    }

    build_palette_from_local_pose(
        skeleton,
        derived_joint_drivers,
        &translations,
        &rotations,
        &scales,
        &tracked,
        source_to_model,
        model_to_source,
    )
}

fn lerp_vec3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn skin_vertices_to_packed(
    bind_vertices: &[AssetTriangleVertex],
    palette: &[[f32; 16]],
) -> Result<Vec<f32>, String> {
    let mut packed = Vec::with_capacity(bind_vertices.len().saturating_mul(FLOATS_PER_VERTEX));
    for vertex in bind_vertices.iter().copied() {
        let mut skinned = vertex;
        let count = vertex.skin_influences as usize;
        if count != 0 {
            let mut position = Vec3::ZERO;
            let mut normal = Vec3::ZERO;
            let tangent_bind = Vec3::new(vertex.tangent[0], vertex.tangent[1], vertex.tangent[2]);
            let mut tangent = Vec3::ZERO;
            for lane in 0..count {
                let joint = vertex.joints[lane] as usize;
                let weight = vertex.weights[lane];
                let matrix = palette.get(joint).ok_or_else(|| {
                    format!(
                        "skin influence references joint={} palette_joints={}",
                        joint,
                        palette.len()
                    )
                })?;
                position = position.add(transform_point_mat4(*matrix, vertex.position).mul(weight));
                normal = normal.add(transform_vector_mat4(*matrix, vertex.normal).mul(weight));
                tangent = tangent.add(transform_vector_mat4(*matrix, tangent_bind).mul(weight));
            }
            skinned.position = position;
            skinned.normal = normal.normalized();
            let tangent = tangent
                .sub(skinned.normal.mul(tangent.dot(skinned.normal)))
                .normalized();
            skinned.tangent[0] = tangent.x;
            skinned.tangent[1] = tangent.y;
            skinned.tangent[2] = tangent.z;
        }

        geometry::append_local_asset_vertex(
            &mut packed,
            skinned.position,
            skinned.normal,
            skinned.tangent,
            skinned.color,
            skinned.uv,
        );
    }
    Ok(packed)
}

fn sample_vec3_clip(
    keys: &[AnimationVec3Key],
    interpolation: AnimationInterpolation,
    time: f32,
    looping: bool,
    duration_seconds: f32,
    fallback: [f32; 3],
) -> [f32; 3] {
    if !looping || keys.len() < 2 || duration_seconds <= 1.0e-8 {
        return sample_vec3(keys, interpolation, time, fallback);
    }

    let time = time.rem_euclid(duration_seconds);
    let first = keys[0];
    let last = keys[keys.len() - 1];
    if time > last.time_seconds && last.time_seconds < duration_seconds {
        if interpolation == AnimationInterpolation::Step {
            return last.value;
        }
        let seam_span = (duration_seconds - last.time_seconds + first.time_seconds).max(1.0e-8);
        let t = ((time - last.time_seconds) / seam_span).clamp(0.0, 1.0);
        return lerp_vec3(last.value, first.value, t);
    }

    if time < first.time_seconds {
        if interpolation == AnimationInterpolation::Step {
            return last.value;
        }
        let previous_time = last.time_seconds - duration_seconds;
        let span = (first.time_seconds - previous_time).max(1.0e-8);
        let t = ((time - previous_time) / span).clamp(0.0, 1.0);
        return lerp_vec3(last.value, first.value, t);
    }

    sample_vec3(keys, interpolation, time, fallback)
}

fn sample_quat_clip(
    keys: &[AnimationQuatKey],
    interpolation: AnimationInterpolation,
    time: f32,
    looping: bool,
    duration_seconds: f32,
    fallback: [f32; 4],
) -> [f32; 4] {
    if !looping || keys.len() < 2 || duration_seconds <= 1.0e-8 {
        return sample_quat(keys, interpolation, time, fallback);
    }

    let time = time.rem_euclid(duration_seconds);
    let first = keys[0];
    let last = keys[keys.len() - 1];
    if time > last.time_seconds && last.time_seconds < duration_seconds {
        if interpolation == AnimationInterpolation::Step {
            return normalize_quat(last.value);
        }
        let seam_span = (duration_seconds - last.time_seconds + first.time_seconds).max(1.0e-8);
        let t = ((time - last.time_seconds) / seam_span).clamp(0.0, 1.0);
        return slerp_quat(normalize_quat(last.value), normalize_quat(first.value), t);
    }

    if time < first.time_seconds {
        if interpolation == AnimationInterpolation::Step {
            return normalize_quat(last.value);
        }
        let previous_time = last.time_seconds - duration_seconds;
        let span = (first.time_seconds - previous_time).max(1.0e-8);
        let t = ((time - previous_time) / span).clamp(0.0, 1.0);
        return slerp_quat(normalize_quat(last.value), normalize_quat(first.value), t);
    }

    sample_quat(keys, interpolation, time, fallback)
}

fn sample_vec3(
    keys: &[AnimationVec3Key],
    interpolation: AnimationInterpolation,
    time: f32,
    fallback: [f32; 3],
) -> [f32; 3] {
    let Some(first) = keys.first() else {
        return fallback;
    };
    if keys.len() == 1 || time <= first.time_seconds {
        return first.value;
    }
    let upper = keys.partition_point(|key| key.time_seconds <= time);
    if upper >= keys.len() {
        return keys.last().map_or(fallback, |key| key.value);
    }
    let a = keys[upper - 1];
    let b = keys[upper];
    if interpolation == AnimationInterpolation::Step {
        return a.value;
    }
    let span = (b.time_seconds - a.time_seconds).max(1.0e-8);
    let t = ((time - a.time_seconds) / span).clamp(0.0, 1.0);
    [
        a.value[0] + (b.value[0] - a.value[0]) * t,
        a.value[1] + (b.value[1] - a.value[1]) * t,
        a.value[2] + (b.value[2] - a.value[2]) * t,
    ]
}

fn sample_quat(
    keys: &[AnimationQuatKey],
    interpolation: AnimationInterpolation,
    time: f32,
    fallback: [f32; 4],
) -> [f32; 4] {
    let Some(first) = keys.first() else {
        return normalize_quat(fallback);
    };
    if keys.len() == 1 || time <= first.time_seconds {
        return normalize_quat(first.value);
    }
    let upper = keys.partition_point(|key| key.time_seconds <= time);
    if upper >= keys.len() {
        return normalize_quat(keys.last().map_or(fallback, |key| key.value));
    }
    let a = keys[upper - 1];
    let b = keys[upper];
    if interpolation == AnimationInterpolation::Step {
        return normalize_quat(a.value);
    }
    let span = (b.time_seconds - a.time_seconds).max(1.0e-8);
    let t = ((time - a.time_seconds) / span).clamp(0.0, 1.0);
    slerp_quat(normalize_quat(a.value), normalize_quat(b.value), t)
}

fn normalize_quat(mut q: [f32; 4]) -> [f32; 4] {
    let length_sq = q.iter().map(|v| v * v).sum::<f32>();
    if !length_sq.is_finite() || length_sq <= 1.0e-12 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let inv = length_sq.sqrt().recip();
    for value in &mut q {
        *value *= inv;
    }
    q
}

fn slerp_quat(a: [f32; 4], mut b: [f32; 4], t: f32) -> [f32; 4] {
    let mut dot = a.iter().zip(b.iter()).map(|(a, b)| a * b).sum::<f32>();
    if dot < 0.0 {
        for value in &mut b {
            *value = -*value;
        }
        dot = -dot;
    }
    if dot > 0.9995 {
        return normalize_quat([
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
            a[3] + (b[3] - a[3]) * t,
        ]);
    }
    let theta = dot.clamp(-1.0, 1.0).acos();
    let sin_theta = theta.sin().max(1.0e-8);
    let wa = ((1.0 - t) * theta).sin() / sin_theta;
    let wb = (t * theta).sin() / sin_theta;
    normalize_quat([
        a[0] * wa + b[0] * wb,
        a[1] * wa + b[1] * wb,
        a[2] * wa + b[2] * wb,
        a[3] * wa + b[3] * wb,
    ])
}

fn trs_matrix(translation: [f32; 3], rotation: [f32; 4], scale: [f32; 3]) -> [f32; 16] {
    let [x, y, z, w] = normalize_quat(rotation);
    let xx = x * x;
    let yy = y * y;
    let zz = z * z;
    let xy = x * y;
    let xz = x * z;
    let yz = y * z;
    let wx = w * x;
    let wy = w * y;
    let wz = w * z;
    [
        (1.0 - 2.0 * (yy + zz)) * scale[0],
        (2.0 * (xy + wz)) * scale[0],
        (2.0 * (xz - wy)) * scale[0],
        0.0,
        (2.0 * (xy - wz)) * scale[1],
        (1.0 - 2.0 * (xx + zz)) * scale[1],
        (2.0 * (yz + wx)) * scale[1],
        0.0,
        (2.0 * (xz + wy)) * scale[2],
        (2.0 * (yz - wx)) * scale[2],
        (1.0 - 2.0 * (xx + yy)) * scale[2],
        0.0,
        translation[0],
        translation[1],
        translation[2],
        1.0,
    ]
}

fn mul_mat4(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            out[column * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[column * 4 + k]).sum();
        }
    }
    out
}

fn transform_point_mat4(matrix: [f32; 16], point: Vec3) -> Vec3 {
    Vec3::new(
        matrix[0] * point.x + matrix[4] * point.y + matrix[8] * point.z + matrix[12],
        matrix[1] * point.x + matrix[5] * point.y + matrix[9] * point.z + matrix[13],
        matrix[2] * point.x + matrix[6] * point.y + matrix[10] * point.z + matrix[14],
    )
}

fn transform_vector_mat4(matrix: [f32; 16], vector: Vec3) -> Vec3 {
    Vec3::new(
        matrix[0] * vector.x + matrix[4] * vector.y + matrix[8] * vector.z,
        matrix[1] * vector.x + matrix[5] * vector.y + matrix[9] * vector.z,
        matrix[2] * vector.x + matrix[6] * vector.y + matrix[10] * vector.z,
    )
}

fn inverse_affine(matrix: [f32; 16]) -> Option<[f32; 16]> {
    let a00 = matrix[0];
    let a01 = matrix[4];
    let a02 = matrix[8];
    let a10 = matrix[1];
    let a11 = matrix[5];
    let a12 = matrix[9];
    let a20 = matrix[2];
    let a21 = matrix[6];
    let a22 = matrix[10];

    let c00 = a11 * a22 - a12 * a21;
    let c01 = a12 * a20 - a10 * a22;
    let c02 = a10 * a21 - a11 * a20;
    let det = a00 * c00 + a01 * c01 + a02 * c02;
    if !det.is_finite() || det.abs() < 1.0e-10 {
        return None;
    }
    let inv_det = 1.0 / det;
    let i00 = c00 * inv_det;
    let i01 = (a02 * a21 - a01 * a22) * inv_det;
    let i02 = (a01 * a12 - a02 * a11) * inv_det;
    let i10 = c01 * inv_det;
    let i11 = (a00 * a22 - a02 * a20) * inv_det;
    let i12 = (a02 * a10 - a00 * a12) * inv_det;
    let i20 = c02 * inv_det;
    let i21 = (a01 * a20 - a00 * a21) * inv_det;
    let i22 = (a00 * a11 - a01 * a10) * inv_det;
    let t = [matrix[12], matrix[13], matrix[14]];
    let it = [
        -(i00 * t[0] + i01 * t[1] + i02 * t[2]),
        -(i10 * t[0] + i11 * t[1] + i12 * t[2]),
        -(i20 * t[0] + i21 * t[1] + i22 * t[2]),
    ];
    Some([
        i00, i10, i20, 0.0, i01, i11, i21, 0.0, i02, i12, i22, 0.0, it[0], it[1], it[2], 1.0,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looping_vec3_sampling_blends_across_clip_seam() {
        let keys = [
            AnimationVec3Key {
                time_seconds: 0.0,
                value: [0.0, 0.0, 0.0],
            },
            AnimationVec3Key {
                time_seconds: 0.75,
                value: [4.0, 0.0, 0.0],
            },
        ];
        let sampled = sample_vec3_clip(
            &keys,
            AnimationInterpolation::Linear,
            0.875,
            true,
            1.0,
            [0.0; 3],
        );
        assert!((sampled[0] - 2.0).abs() < 1.0e-5, "{sampled:?}");
    }

    #[test]
    fn looping_quaternion_sampling_has_no_end_to_start_snap() {
        let keys = [
            AnimationQuatKey {
                time_seconds: 0.0,
                value: [0.0, 0.0, 0.0, 1.0],
            },
            AnimationQuatKey {
                time_seconds: 0.75,
                value: [0.0, 1.0, 0.0, 0.0],
            },
        ];
        let sampled = sample_quat_clip(
            &keys,
            AnimationInterpolation::Linear,
            0.875,
            true,
            1.0,
            [0.0, 0.0, 0.0, 1.0],
        );
        let expected = 0.70710677_f32;
        assert!((sampled[1].abs() - expected).abs() < 1.0e-4, "{sampled:?}");
        assert!((sampled[3].abs() - expected).abs() < 1.0e-4, "{sampled:?}");
    }

    #[test]
    fn blended_palette_crossfades_local_translation() {
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let skeleton = ModelSkeleton {
            name: "blend-test".to_owned(),
            joints: vec![newviso_model::ModelJoint {
                name: "root".to_owned(),
                tag: 1,
                parent: None,
                inverse_bind_matrix: identity,
                bind_translation: [0.0, 0.0, 0.0],
                bind_rotation: [0.0, 0.0, 0.0, 1.0],
                bind_scale: [1.0, 1.0, 1.0],
            }],
        };
        let drivers = build_derived_joint_drivers(&skeleton).unwrap();
        let make_clip = |name: &str, x: f32| ModelAnimationClip {
            name: name.to_owned(),
            duration_seconds: 1.0,
            looping: true,
            tracks: vec![newviso_model::JointAnimationTrack {
                joint: 0,
                translation_interpolation: AnimationInterpolation::Linear,
                rotation_interpolation: AnimationInterpolation::Linear,
                scale_interpolation: AnimationInterpolation::Linear,
                translations: vec![AnimationVec3Key {
                    time_seconds: 0.0,
                    value: [x, 0.0, 0.0],
                }],
                rotations: Vec::new(),
                scales: Vec::new(),
            }],
        };
        let from = make_clip("from", 0.0);
        let to = make_clip("to", 10.0);
        let palette = build_blended_skin_palette(
            &skeleton, &drivers, &from, 0.0, &to, 0.0, 0.5, identity, identity,
        )
        .unwrap();
        let moved = transform_point_mat4(palette[0], Vec3::ZERO);
        assert!((moved.x - 5.0).abs() < 1.0e-5, "x={}", moved.x);
    }

    #[test]
    fn continuous_clip_clock_wraps_without_quantizing_to_sample_rate() {
        let clip = ModelAnimationClip {
            name: "clock".to_owned(),
            duration_seconds: 1.0,
            looping: true,
            tracks: Vec::new(),
        };
        let t = advance_clip_time(&clip, 0.99, 1.0 / 120.0);
        assert!(t > 0.998 && t < 1.0, "t={t}");
        let wrapped = advance_clip_time(&clip, t, 1.0 / 120.0);
        assert!(wrapped > 0.006 && wrapped < 0.008, "wrapped={wrapped}");
    }

    #[test]
    fn quaternion_slerp_preserves_unit_length() {
        let q = slerp_quat([0.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 0.0], 0.5);
        let length = q.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((length - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn untracked_helper_chain_follows_animated_core_joint() {
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let child_inverse_bind = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 1.0,
        ];
        let skeleton = ModelSkeleton {
            name: "helper-test".to_owned(),
            joints: vec![
                newviso_model::ModelJoint {
                    name: "root".to_owned(),
                    tag: 1,
                    parent: None,
                    inverse_bind_matrix: identity,
                    bind_translation: [0.0, 0.0, 0.0],
                    bind_rotation: [0.0, 0.0, 0.0, 1.0],
                    bind_scale: [1.0, 1.0, 1.0],
                },
                newviso_model::ModelJoint {
                    name: "root_helper".to_owned(),
                    tag: 2,
                    parent: None,
                    inverse_bind_matrix: identity,
                    bind_translation: [0.0, 0.0, 0.0],
                    bind_rotation: [0.0, 0.0, 0.0, 1.0],
                    bind_scale: [1.0, 1.0, 1.0],
                },
                newviso_model::ModelJoint {
                    name: "roll".to_owned(),
                    tag: 3,
                    parent: Some(1),
                    inverse_bind_matrix: child_inverse_bind,
                    bind_translation: [1.0, 0.0, 0.0],
                    bind_rotation: [0.0, 0.0, 0.0, 1.0],
                    bind_scale: [1.0, 1.0, 1.0],
                },
            ],
        };
        let drivers = build_derived_joint_drivers(&skeleton).unwrap();
        assert_eq!(drivers[1].map(|driver| driver.joint), Some(0));

        let clip = ModelAnimationClip {
            name: "move-root".to_owned(),
            duration_seconds: 1.0,
            looping: true,
            tracks: vec![newviso_model::JointAnimationTrack {
                joint: 0,
                translation_interpolation: AnimationInterpolation::Linear,
                rotation_interpolation: AnimationInterpolation::Linear,
                scale_interpolation: AnimationInterpolation::Linear,
                translations: vec![AnimationVec3Key {
                    time_seconds: 0.0,
                    value: [2.0, 0.0, 0.0],
                }],
                rotations: Vec::new(),
                scales: Vec::new(),
            }],
        };
        let palette =
            build_skin_palette(&skeleton, &drivers, &clip, 0.0, identity, identity).unwrap();

        for joint in 0..3 {
            let moved = transform_point_mat4(palette[joint], Vec3::ZERO);
            assert!(
                (moved.x - 2.0).abs() < 1.0e-5,
                "joint {joint} did not inherit core motion: x={}",
                moved.x
            );
        }
    }

    #[test]
    fn affine_inverse_roundtrip_is_identity() {
        let m = trs_matrix(
            [1.0, 2.0, -3.0],
            [0.0, 0.38268343, 0.0, 0.9238795],
            [1.2, 0.8, 1.1],
        );
        let inv = inverse_affine(m).unwrap();
        let product = mul_mat4(m, inv);
        for (index, value) in product.iter().enumerate() {
            let expected = if index % 5 == 0 { 1.0 } else { 0.0 };
            assert!((value - expected).abs() < 1.0e-4);
        }
    }
}
