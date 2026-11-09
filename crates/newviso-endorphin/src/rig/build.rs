//! Fit native body geometry, mass and joint frames to a resident skeleton.
use super::*;
impl RigDefinition {
    /// Build from the installed animated skeleton. No identity bones, invented
    /// spin or arbitrary force is substituted when input data is unavailable.
    pub fn build(
        &self,
        material: &RagdollMaterial,
        input: RigInput<'_>,
    ) -> Result<RigPlan, String> {
        self.validate()?;
        if input.entities.len() != self.parts.len()
            || input.entities.iter().collect::<BTreeSet<_>>().len() != input.entities.len()
            || input
                .source
                .position
                .iter()
                .chain(&input.source.rotation)
                .chain(&input.source.linear_velocity)
                .chain(&input.source.angular_velocity)
                .chain(&input.visual_position)
                .any(|v| !v.is_finite())
            || !input.visual_heading.is_finite()
            || !input.source_mass_kg.is_finite()
            || input.source_mass_kg <= 0.0
            || !input.source_radius.is_finite()
            || input.source_radius <= 0.0
            || !input.source_half_height.is_finite()
            || input.source_half_height < 0.0
        {
            return Err("invalid Endorphin rig input".into());
        }
        let matrix = |joint: &str| -> Result<[f32; 16], String> {
            let m = *input
                .matrices
                .get(joint)
                .ok_or_else(|| format!("missing Endorphin bone '{joint}'"))?;
            if m.iter().any(|v| !v.is_finite())
                || (0..3)
                    .any(|i| math::length([m[i * 4], m[i * 4 + 1], m[i * 4 + 2]]) <= f32::EPSILON)
            {
                return Err(format!("invalid Endorphin bone matrix '{joint}'"));
            }
            Ok(m)
        };
        let reference = input.reference_matrices.unwrap_or(input.matrices);
        let ref_matrix = |name: &str| -> Result<[f32; 16], String> {
            let m = *reference
                .get(name)
                .ok_or_else(|| format!("missing anatomical reference {name}"))?;
            if m.iter().any(|v| !v.is_finite()) {
                return Err("invalid anatomical reference matrix".into());
            }
            Ok(m)
        };
        let orientation = if let Some(anatomy) = input.anatomy {
            anatomy.validate(self)?;
            if input.reference_matrices.is_none() {
                return Err("anatomy requires neutral reference matrices".into());
            }
            let o = &anatomy.orientation;
            let up = math::unit(math::sub(
                math::position(ref_matrix(&o.upper)?),
                math::position(ref_matrix(&o.lower)?),
            ))?;
            let right = math::unit(math::sub(
                math::position(ref_matrix(&o.right)?),
                math::position(ref_matrix(&o.left)?),
            ))?;
            let forward = if let Some([from, to]) = &o.forward {
                let axis = math::sub(
                    math::position(ref_matrix(to)?),
                    math::position(ref_matrix(from)?),
                );
                math::unit(math::sub(axis, up.map(|v| v * math::dot(axis, up))))?
            } else {
                math::unit(math::cross(right, up))?
            };
            let right = math::unit(math::cross(up, forward))?;
            Some((up, forward, right))
        } else {
            None
        };
        let weight: f32 = self.parts.iter().map(|p| p.mass_weight).sum();
        let mut bodies = Vec::with_capacity(self.parts.len());
        let mut parts = Vec::with_capacity(self.parts.len());
        let mut joints = Vec::with_capacity(self.parts.len() - 1);
        let mut initial = BTreeMap::<String, (usize, [f32; 3], [f32; 4], [f32; 4])>::new();
        for (index, part) in self.parts.iter().enumerate() {
            let start = matrix(&part.joint)?;
            let a = math::position(start);
            let end = part
                .end_joint
                .as_deref()
                .map(|j| matrix(j).map(math::position))
                .transpose()?;
            let mut center = end.map_or(a, |b| math::add(a, b).map(|v| v * 0.5));
            let rest = ref_matrix(&part.joint)?;
            let rest_end = part
                .end_joint
                .as_deref()
                .map(|j| ref_matrix(j).map(math::position))
                .transpose()?;
            let mut rest_rotation = rest_end.map_or_else(
                || math::rotation(rest),
                |b| math::from_axis(math::sub(b, math::position(rest))),
            );
            let mut box_half_extents = None;
            if let Some(sole) = part.sole {
                let up = orientation.map_or([0.0, 1.0, 0.0], |o| o.0);
                let forward = orientation.map_or([0.0, 0.0, 1.0], |o| o.1);
                let right = orientation.map_or([1.0, 0.0, 0.0], |o| o.2);
                let span = math::sub(
                    rest_end.ok_or("sole requires a neutral end joint")?,
                    math::position(rest),
                );
                let horizontal = math::sub(span, up.map(|v| v * math::dot(span, up)));
                let direction = math::unit(horizontal).unwrap_or(forward);
                rest_rotation = math::anatomical_frame(direction, up, right)?;
                let length = math::dot(span, direction).abs();
                let heel = part.radius * sole.heel_radius_scale;
                let offset = math::add(
                    direction.map(|v| v * (length - heel) * 0.5),
                    up.map(|v| v * part.radius * sole.vertical_offset_radius_scale),
                );
                let change =
                    math::multiply(math::rotation(start), math::inverse(math::rotation(rest)));
                center = math::add(a, math::rotate(change, offset));
                box_half_extents = Some([
                    part.radius * sole.width_radius_scale,
                    (length + heel) * 0.5,
                    part.radius * sole.thickness_radius_scale,
                ]);
            }
            let rotation = if input.reference_matrices.is_some() {
                math::multiply(
                    math::multiply(math::rotation(start), math::inverse(math::rotation(rest))),
                    rest_rotation,
                )
            } else {
                end.map_or_else(
                    || math::rotation(start),
                    |b| math::from_axis(math::sub(b, a)),
                )
            };
            // Capsule tips match the sampled bone segment; short segments
            // become spheres rather than growing an extra minimum cylinder.
            let half_height = end.map_or(0.0, |b| {
                (math::length(math::sub(b, a)) * 0.5 - part.radius).max(0.0)
            });
            let mass = self.mass_kg * part.mass_weight / weight;
            let entity = input.entities[index];
            let inertia = box_half_extents.map_or_else(
                || math::capsule_inertia(mass, part.radius, half_height),
                |h| {
                    [
                        mass * (h[1] * h[1] + h[2] * h[2]) / 3.0,
                        mass * (h[0] * h[0] + h[2] * h[2]) / 3.0,
                        mass * (h[0] * h[0] + h[1] * h[1]) / 3.0,
                    ]
                },
            );
            bodies.push(BodySpec {
                entity,
                pose: BodyPose {
                    position: center,
                    rotation,
                    linear_velocity: [0.0; 3],
                    angular_velocity: [0.0; 3],
                },
                radius: part.radius,
                half_height,
                mass_kg: mass,
                inertia_diagonal: inertia,
                box_half_extents,
            });
            parts.push(RigPart {
                support_axis: part.sole.map(|_| [0.0, 0.0, 1.0]),
                support_clearance: box_half_extents.map(|h| h[2]),
                support_half_extents: box_half_extents,
                name: part.name.clone(),
                joint: part.joint.clone(),
                entity,
                body_to_joint: math::product(math::inverse_pose(center, rotation), start),
                mass_kg: mass,
                inertia_diagonal: inertia,
            });
            if let Some(parent) = &part.parent {
                let (parent_index, parent_center, parent_rotation, parent_rest_rotation) =
                    initial[parent];
                let anchor = matrix(part.anchor_joint.as_deref().unwrap_or(&part.joint))?;
                let position = math::position(anchor);
                let (parent_frame, child_frame) = if let Some((up, forward, right)) = orientation {
                    let axis = rest_end.map_or_else(
                        || {
                            part.parent
                                .as_ref()
                                .map_or(up, |p| math::rotate(initial[p].3, [0.0, 1.0, 0.0]))
                        },
                        |p| math::sub(p, math::position(rest)),
                    );
                    let neutral = math::anatomical_frame(axis, forward, right)?;
                    (
                        math::multiply(math::inverse(parent_rest_rotation), neutral),
                        math::multiply(math::inverse(rest_rotation), neutral),
                    )
                } else {
                    let frame = math::rotation(anchor);
                    (
                        math::multiply(math::inverse(parent_rotation), frame),
                        math::multiply(math::inverse(rotation), frame),
                    )
                };
                let anatomy_part = input.anatomy.and_then(|m| m.parts.get(&part.name));
                joints.push(JointSpec {
                    id: entity,
                    parent: input.entities[parent_index],
                    child: entity,
                    parent_anchor: math::rotate(
                        math::inverse(parent_rotation),
                        math::sub(position, parent_center),
                    ),
                    child_anchor: math::rotate(
                        math::inverse(rotation),
                        math::sub(position, center),
                    ),
                    parent_frame_rotation: parent_frame,
                    child_frame_rotation: child_frame,
                    max_force: material.joint_max_force,
                    cone_limit: part.cone_limit_degrees.to_radians(),
                    angular_stiffness: material.joint_angular_stiffness,
                    angular_damping: material.joint_angular_damping,
                    collision_family: input.entities[0],
                    parent_part: parent_index,
                    child_part: index,
                    initial_rotation: math::multiply(
                        math::inverse(math::multiply(parent_rotation, parent_frame)),
                        math::multiply(rotation, child_frame),
                    ),
                    angular_limits: anatomy_part.and_then(|p| p.limits),
                    muscle: anatomy_part.and_then(|p| p.muscle),
                });
            }
            initial.insert(part.name.clone(), (index, center, rotation, rest_rotation));
        }
        let center_of_mass: [f32; 3] = std::array::from_fn(|i| {
            bodies
                .iter()
                .map(|b| b.mass_kg * b.pose.position[i])
                .sum::<f32>()
                / self.mass_kg
        });
        let mut inertia = [[0.0; 3]; 3];
        for b in &bodies {
            let local = math::tensor(b.pose.rotation, b.inertia_diagonal);
            let r = math::sub(b.pose.position, center_of_mass);
            for i in 0..3 {
                for j in 0..3 {
                    inertia[i][j] += local[i][j]
                        + b.mass_kg * (if i == j { math::dot(r, r) } else { 0.0 } - r[i] * r[j]);
                }
            }
        }
        let linear_momentum = input
            .source
            .linear_velocity
            .map(|v| v * input.source_mass_kg);
        let source_inertia = math::tensor(
            input.source.rotation,
            math::capsule_inertia(
                input.source_mass_kg,
                input.source_radius,
                input.source_half_height,
            ),
        );
        // Include orbital momentum when the articulated COM differs from the
        // source capsule COM. Both quantities are measured about the NEW COM.
        let angular_momentum = math::add(
            math::tensor_vector(source_inertia, input.source.angular_velocity),
            math::cross(
                math::sub(input.source.position, center_of_mass),
                linear_momentum,
            ),
        );
        let angular = math::solve_symmetric(inertia, angular_momentum)
            .ok_or_else(|| "singular Endorphin body inertia".to_owned())?;
        let velocity = linear_momentum.map(|v| v / self.mass_kg);
        for b in &mut bodies {
            b.pose.linear_velocity = math::add(
                velocity,
                math::cross(angular, math::sub(b.pose.position, center_of_mass)),
            );
            b.pose.angular_velocity = angular;
        }
        let root = bodies[0].pose.position;
        let palette = parts
            .iter()
            .map(|p| (p.joint.clone(), input.matrices[&p.joint]))
            .collect();
        Ok(RigPlan {
            state: RigState {
                previous: vec![None; parts.len()],
                parts,
                joints,
                elapsed: 0.0,
                visual_offset: math::sub(input.visual_position, root),
                visual_rotation: [0.0, input.visual_heading, 0.0],
                palette,
                entities: {
                    let mut ids = input.entities.to_vec();
                    ids.sort_unstable();
                    ids
                },
                stance_entities: {
                    let mut ids: Vec<_> = self
                        .parts
                        .iter()
                        .enumerate()
                        .filter_map(|(i, p)| {
                            input
                                .anatomy
                                .filter(|a| a.balance.support_parts.contains(&p.name))
                                .map(|_| input.entities[i])
                        })
                        .collect();
                    ids.sort_unstable();
                    ids
                },
                pose_updates: 0,
                pose_skips: 0,
            },
            bodies,
            center_of_mass,
            linear_momentum,
            angular_momentum,
        })
    }
}
