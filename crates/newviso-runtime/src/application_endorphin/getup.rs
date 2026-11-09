//! Recovery admission fits the installed physical skeleton, then checks the
//! prospective root/capsule before releasing any articulated bodies.
use super::*;
use newviso_endorphin::{
    find_recovery_placement, match_weighted_recovery_samples, math, recovery_sample_times,
    FallDirection, RecoveryPlacement, RecoveryPlacementInput, RecoveryPlacementReport,
    RecoveryPoseMatch, WeightedRecoveryMarker,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct GetUpMatchStatus {
    pub status: &'static str,
    pub evaluated: usize,
    pub unavailable: usize,
    pub filtered: usize,
    pub duplicate_clips: usize,
    pub marker_count: usize,
    pub authored: bool,
    pub selection_score: Option<f64>,
    pub sample_cache_hits: usize,
    pub samples_tested: usize,
    pub match_time_seconds: Option<f32>,
    pub clip_duration_seconds: Option<f32>,
    pub clip_ref: Option<String>,
    pub root_joint: Option<String>,
    pub selected: Option<RecoveryPoseMatch>,
    pub placement: Option<RecoveryPlacementReport>,
    pub errors: Vec<String>,
}
impl Default for GetUpMatchStatus {
    fn default() -> Self {
        Self {
            status: "not_requested",
            evaluated: 0,
            unavailable: 0,
            filtered: 0,
            duplicate_clips: 0,
            marker_count: 0,
            authored: false,
            selection_score: None,
            sample_cache_hits: 0,
            samples_tested: 0,
            match_time_seconds: None,
            clip_duration_seconds: None,
            clip_ref: None,
            root_joint: None,
            selected: None,
            placement: None,
            errors: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub(super) struct GetUpPoseCache {
    skeleton: Arc<newviso_model::ModelSkeleton>,
    scale: [f32; 3],
    joints: Vec<String>,
    clips: BTreeMap<String, SampledGetUp>,
}
#[derive(Debug)]
struct SampledGetUp {
    clip: Arc<newviso_model::ModelAnimationClip>,
    duration_seconds: f32,
    samples: Arc<[GetUpSample]>,
}

#[derive(Debug)]
struct GetUpSample {
    time_seconds: f32,
    points: Vec<[f32; 3]>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GetUpSelection {
    pub direction: FallDirection,
    pub fit: RecoveryPoseMatch,
    pub time_seconds: f32,
    pub duration_seconds: f32,
    pub blend_seconds: Option<f32>,
    root_local: [f32; 3],
    floor_offset: f32,
    source_root: [f32; 3],
    model_offset: [f32; 3],
}

fn installed_root<'a>(skeleton: &'a newviso_model::ModelSkeleton, joint: &str) -> Option<&'a str> {
    let mut index = skeleton
        .joints
        .iter()
        .position(|j| j.name.eq_ignore_ascii_case(joint))?;
    for _ in 0..skeleton.joints.len() {
        let node = skeleton.joints.get(index)?;
        if let Some(parent) = node.parent {
            index = parent as usize;
        } else {
            return Some(node.name.as_str());
        }
    }
    None
}

impl EngineApplication {
    pub(crate) fn ped_getup_root_joint(&self, id: &str) -> Option<&str> {
        let body = self.ped_physics.bodies.get(id)?;
        if let Some(name) = body.endorphin.getup_match.root_joint.as_deref() {
            return Some(name);
        }
        installed_root(
            body.endorphin.bound_skeleton.as_ref()?,
            &body.endorphin.model.rig.parts.first()?.joint,
        )
    }

    pub(crate) fn select_ped_getup_pose(
        &mut self,
        id: &str,
        clips: &newviso_endorphin::DirectionalClips,
    ) -> Option<GetUpSelection> {
        self.ped_physics.bodies.get_mut(id)?.endorphin.getup_match = GetUpMatchStatus {
            status: "unavailable",
            ..Default::default()
        };
        let (
            observed,
            mut names,
            entity,
            scale,
            skeleton,
            root_name,
            source_root,
            model_offset,
            contacts,
            model,
        ) = {
            let body = self.ped_physics.bodies.get(id)?;
            let rig = body.endorphin.ragdoll.as_ref()?;
            let physics = self.physics.as_ref()?;
            let profile = body.endorphin.model.recovery_match.as_ref();
            let (observed, names): (Option<Vec<_>>, Vec<_>) = if let Some(profile) = profile {
                let markers: Vec<_> = profile.joints.iter().filter(|m| m.weight > 0.0).collect();
                let joint_names: Vec<_> = markers.iter().map(|m| m.joint.as_str()).collect();
                let matrices = match self
                    .scene
                    .entity_joint_world_matrices(body.entity, &joint_names)
                {
                    Ok(matrices) => matrices,
                    Err(error) => {
                        self.ped_physics
                            .bodies
                            .get_mut(id)?
                            .endorphin
                            .getup_match
                            .errors
                            .push(error);
                        return None;
                    }
                };
                let observed = markers
                    .iter()
                    .map(|marker| {
                        Some(WeightedRecoveryMarker {
                            position: math::position(*matrices.get(&marker.joint)?),
                            weight: marker.weight,
                        })
                    })
                    .collect();
                (observed, markers.iter().map(|m| m.joint.clone()).collect())
            } else {
                let observed = rig
                    .parts
                    .iter()
                    .map(|part| {
                        let pose = physics.vehicle_body_state(part.entity)?;
                        Some(WeightedRecoveryMarker {
                            position: math::position(math::product(
                                math::matrix(pose.position, pose.rotation),
                                part.body_to_joint,
                            )),
                            weight: part.mass_kg,
                        })
                    })
                    .collect();
                (
                    observed,
                    rig.parts.iter().map(|p| p.joint.clone()).collect(),
                )
            };
            let skeleton = self.scene.entity_skeleton(body.entity)?.clone();
            let root_name = installed_root(&skeleton, &rig.parts.first()?.joint)?.to_owned();
            let source_root = math::position(
                self.scene
                    .entity_joint_world_matrix(body.entity, &root_name)
                    .ok()?,
            );
            let transform = self.scene.entity_transform_matrix(body.entity)?;
            let scale: [f32; 3] = std::array::from_fn(|i| {
                math::length([transform[i * 4], transform[i * 4 + 1], transform[i * 4 + 2]])
            });
            let binding = self.world_actor_presentations.get(id)?;
            let definition = binding
                .locomotion
                .as_ref()
                .and_then(|l| l.ragdoll.as_ref())
                .unwrap_or(&body.endorphin.model.rig);
            let contacts: Option<Vec<_>> = rig
                .parts
                .iter()
                .map(|part| {
                    definition
                        .parts
                        .iter()
                        .find(|p| p.name == part.name)
                        .map(|p| (part.joint.clone(), p.radius))
                })
                .collect();
            (
                observed?,
                names,
                body.entity,
                scale,
                skeleton,
                root_name,
                source_root,
                binding.position_offset,
                contacts?,
                body.endorphin.model.clone(),
            )
        };
        let profile = model.recovery_match.as_ref();
        let mut index_for = |name: &str| {
            if let Some(i) = names.iter().position(|n| n.eq_ignore_ascii_case(name)) {
                i
            } else {
                names.push(name.to_owned());
                names.len() - 1
            }
        };
        let root_index = index_for(&root_name);
        let floor_indices: Vec<_> = contacts
            .iter()
            .map(|(name, r)| (index_for(name), *r))
            .collect();
        let valid_cache = self.ped_physics.bodies[id]
            .endorphin
            .getup_pose_cache
            .as_ref()
            .is_some_and(|cache| {
                Arc::ptr_eq(&cache.skeleton, &skeleton)
                    && cache.scale == scale
                    && cache.joints == names
            });
        if !valid_cache {
            self.ped_physics
                .bodies
                .get_mut(id)?
                .endorphin
                .getup_pose_cache = Some(GetUpPoseCache {
                skeleton,
                scale,
                joints: names.clone(),
                clips: BTreeMap::new(),
            });
        }
        let joints: Vec<_> = names.iter().map(String::as_str).collect();
        let mut status = GetUpMatchStatus {
            status: "unavailable",
            authored: profile.is_some(),
            marker_count: observed.len(),
            root_joint: Some(root_name),
            ..Default::default()
        };
        let mut best: Option<GetUpSelection> = None;
        let mut best_score = f64::INFINITY;
        let mut seen_authored_clips = BTreeSet::new();
        for (direction, clip) in [
            (FallDirection::Front, &clips.front),
            (FallDirection::Back, &clips.back),
            (FallDirection::Left, &clips.left),
            (FallDirection::Right, &clips.right),
        ] {
            let item = profile.and_then(|p| p.clips.get(&clip.clip_ref));
            if profile.is_some() {
                if item.is_none() {
                    status.filtered += 1;
                    continue;
                }
                if !seen_authored_clips.insert(clip.clip_ref.as_str()) {
                    status.duplicate_clips += 1;
                    continue;
                }
            }
            let sampled = self
                .load_cached_scene_animation_clip(entity, &clip.clip_ref, true)
                .and_then(|loaded| {
                    let Some(loaded) = loaded else {
                        return Ok(None);
                    };
                    let duration = clip.duration_seconds.min(loaded.duration_seconds);
                    if !loaded.duration_seconds.is_finite() || duration <= 0.0 {
                        return Err(format!(
                            "invalid get-up clip duration for {}",
                            clip.clip_ref
                        ));
                    }
                    let mut fallback_times = Vec::new();
                    let times = if let Some(item) = item {
                        if item
                            .sample_times_seconds
                            .last()
                            .is_none_or(|t| *t >= duration)
                        {
                            return Err(format!("recovery sample outside clip {}", clip.clip_ref));
                        }
                        item.sample_times_seconds.as_slice()
                    } else {
                        fallback_times.extend(recovery_sample_times(duration));
                        &fallback_times
                    };
                    let cached = self.ped_physics.bodies[id]
                        .endorphin
                        .getup_pose_cache
                        .as_ref()
                        .and_then(|cache| cache.clips.get(&clip.clip_ref))
                        .filter(|entry| {
                            Arc::ptr_eq(&entry.clip, &loaded)
                                && entry.duration_seconds == duration
                                && entry
                                    .samples
                                    .iter()
                                    .map(|s| s.time_seconds)
                                    .eq(times.iter().copied())
                        })
                        .map(|entry| entry.samples.clone());
                    if let Some(samples) = cached {
                        status.sample_cache_hits += 1;
                        return Ok(Some((duration, samples)));
                    }
                    let samples: Arc<[GetUpSample]> = times
                        .iter()
                        .copied()
                        .map(|time_seconds| {
                            let matrices = self.scene.entity_clip_joint_model_matrices(
                                entity,
                                &loaded,
                                time_seconds,
                                &joints,
                            )?;
                            let points = matrices
                                .into_iter()
                                .map(|m| {
                                    let point = math::position(m);
                                    std::array::from_fn(|i| point[i] * scale[i])
                                })
                                .collect();
                            Ok(GetUpSample {
                                time_seconds,
                                points,
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()?
                        .into();
                    self.ped_physics
                        .bodies
                        .get_mut(id)
                        .expect("resident ped")
                        .endorphin
                        .getup_pose_cache
                        .as_mut()
                        .expect("get-up cache")
                        .clips
                        .insert(
                            clip.clip_ref.clone(),
                            SampledGetUp {
                                clip: loaded,
                                duration_seconds: duration,
                                samples: samples.clone(),
                            },
                        );
                    Ok(Some((duration, samples)))
                });
            let (duration_seconds, samples) = match sampled {
                Ok(Some(entry)) => entry,
                Ok(None) => {
                    status.unavailable += 1;
                    continue;
                }
                Err(error) => {
                    status.unavailable += 1;
                    status.errors.push(error);
                    continue;
                }
            };
            status.samples_tested += samples.len();
            let candidates = samples
                .iter()
                .map(|sample| &sample.points[..observed.len()]);
            let Some((index, fit)) = match_weighted_recovery_samples(&observed, candidates) else {
                status.unavailable += 1;
                status
                    .errors
                    .push("invalid or underdetermined recovery markers".into());
                continue;
            };
            status.evaluated += 1;
            let sample = &samples[index];
            let points = &sample.points;
            let floor_offset = floor_indices
                .iter()
                .map(|(i, r)| points[*i][1] - r)
                .fold(f32::INFINITY, f32::min);
            let score = f64::from(fit.normalized_error)
                * f64::from(item.map_or(1.0, |item| item.score_multiplier));
            if score < best_score {
                best_score = score;
                status.selection_score = Some(score);
                status.clip_ref = Some(clip.clip_ref.clone());
                best = Some(GetUpSelection {
                    direction,
                    fit,
                    time_seconds: sample.time_seconds,
                    duration_seconds,
                    blend_seconds: item.and_then(|item| item.blend_seconds),
                    root_local: points[root_index],
                    floor_offset,
                    source_root,
                    model_offset,
                });
            }
        }
        if let Some(selection) = best {
            status.status = "matched";
            status.selected = Some(selection.fit);
            status.match_time_seconds = Some(selection.time_seconds);
            status.clip_duration_seconds = Some(selection.duration_seconds);
        }
        self.ped_physics.bodies.get_mut(id)?.endorphin.getup_match = status;
        best
    }

    pub(crate) fn place_ped_getup(
        &mut self,
        id: &str,
        selection: GetUpSelection,
    ) -> Option<RecoveryPlacement> {
        let body = self.ped_physics.bodies.get(id)?;
        let physics = self.physics.as_ref()?;
        let ground = self.ped_support_height(id)?;
        let radius = body.radius;
        let height = 2.0 * (radius + body.half_height);
        let skin = physics.contact_skin();
        let input = RecoveryPlacementInput {
            pose_match: selection.fit,
            root_local: selection.root_local,
            model_offset: selection.model_offset,
            floor_offset: selection.floor_offset,
            capsule_radius: radius,
            capsule_center_offset: body.center_offset,
            clearance: 0.002,
        };
        if !selection
            .fit
            .translation
            .iter()
            .chain(&selection.source_root)
            .chain(&selection.root_local)
            .chain(&selection.model_offset)
            .all(|v| v.is_finite())
            || !selection.floor_offset.is_finite()
            || !ground.is_finite()
        {
            self.ped_physics
                .bodies
                .get_mut(id)?
                .endorphin
                .getup_match
                .placement = Some(RecoveryPlacementReport {
                invalid_input: true,
                ..Default::default()
            });
            return None;
        }
        let target = selection.fit.translation;
        let probe_y = target[1].max(ground + radius);
        let reach = height * (2.0 / 3.0);
        let extent = 2.0 * radius + skin;
        let body_x = target[0] - selection.model_offset[0];
        let body_z = target[2] - selection.model_offset[2];
        let (sin, cos) = selection.fit.heading_radians.sin_cos();
        let root_x = target[0] + cos * selection.root_local[0] + sin * selection.root_local[2];
        let root_z = target[2] - sin * selection.root_local[0] + cos * selection.root_local[2];
        let vertical = selection.floor_offset.abs()
            + selection.root_local[1].abs()
            + selection.model_offset[1].abs();
        let min = [
            body_x.min(selection.source_root[0]).min(root_x) - extent,
            (probe_y - reach).min(selection.source_root[1]) - radius - vertical,
            body_z.min(selection.source_root[2]).min(root_z) - extent,
        ];
        let max = [
            body_x.max(selection.source_root[0]).max(root_x) + extent,
            (probe_y + height).max(selection.source_root[1]) + vertical,
            body_z.max(selection.source_root[2]).max(root_z) + extent,
        ];
        if !min.iter().chain(&max).all(|v| v.is_finite()) {
            self.ped_physics
                .bodies
                .get_mut(id)?
                .endorphin
                .getup_match
                .placement = Some(RecoveryPlacementReport {
                invalid_input: true,
                ..Default::default()
            });
            return None;
        }
        let solids = self.scene.physics_static_solid_aabbs_near(&[(min, max)]);
        body.endorphin.ragdoll.as_ref()?;
        let ignore = self.ped_getup_query_ignore(id);
        let report = find_recovery_placement(
            input,
            |point| {
                physics.endorphin_getup_ground(
                    [point[0], probe_y, point[2]],
                    reach,
                    &ignore,
                    &solids,
                    body.endorphin.model.recovery.minimum_support_cosine,
                )
            },
            |placed| {
                physics
                    .endorphin_getup_obstruction(
                        [
                            placed.capsule_center[0],
                            placed.ground + radius + skin,
                            placed.capsule_center[2],
                        ],
                        2.0 * body.half_height,
                        (radius - skin).max(f32::EPSILON),
                        &ignore,
                        &solids,
                    )
                    .is_none()
            },
            |root| {
                physics.endorphin_getup_root_path_clear(
                    selection.source_root,
                    root,
                    &ignore,
                    &solids,
                )
            },
        );
        let selected = report.selected;
        let prepared = if let Some(placed) = selected {
            (|| {
                let surface = physics.endorphin_getup_surface(
                    [placed.capsule_center[0], probe_y, placed.capsule_center[2]],
                    reach,
                    &ignore,
                    &solids,
                    body.endorphin.model.recovery.minimum_support_cosine,
                )?;
                if let Some(anchor) = surface.support {
                    let frame = physics.endorphin_recovery_support_frame(anchor.entity)?;
                    let binding = newviso_endorphin::RecoverySupport::bind(
                        anchor.entity,
                        frame,
                        placed,
                        surface.normal,
                    )?;
                    let sample = binding.sample(
                        Some(frame),
                        body.endorphin.model.recovery.minimum_support_cosine,
                    )?;
                    Some((
                        Some(binding),
                        crate::application_peds::physics::recovery_support::GetupSupportStatus {
                            status: "bound",
                            entity_key: Some(anchor.entity.to_string()),
                            initial: Some(sample),
                            current: Some(sample),
                            transport_updates: 0,
                        },
                    ))
                } else {
                    Some((
                        None,
                        crate::application_peds::physics::recovery_support::GetupSupportStatus {
                            status: "world",
                            ..Default::default()
                        },
                    ))
                }
            })()
        } else {
            Some((None, Default::default()))
        };
        let body = self.ped_physics.bodies.get_mut(id)?;
        body.endorphin.getup_match.placement = Some(report);
        let Some((attachment, status)) = prepared else {
            body.endorphin.getup_support.status = "unavailable";
            return None;
        };
        body.endorphin.recovery_support = attachment;
        body.endorphin.getup_support = status;
        selected
    }
}
