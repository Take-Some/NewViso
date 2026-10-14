use super::*;
use newviso_model::ModelFragmentPartPhysics;

pub(super) struct FragmentBody {
    pub mass: f32,
    pub center_of_mass: [f32; 3],
    pub inertia: [f32; 3],
    pub hulls: Vec<Vec<[f32; 3]>>,
    pub components: Vec<u32>,
    pub authored: bool,
}

pub(super) fn physics_snapshot(p: &ModelFragmentPartPhysics) -> Value {
    json!({"group":p.group,"parent_group":p.parent_group,"strength":p.strength,
        "latch_strength":p.latch_strength,"transmission_up":p.transmission_up,
        "transmission_down":p.transmission_down,"joint_stiffness":p.joint_stiffness,
        "soft_angle_limits":p.soft_angle_limits,"flags":p.flags,"damage_health":p.damage_health,
        "weapon_health":p.weapon_health,"min_damage_force":p.min_damage_force,
        "children":p.children.iter().map(|c|json!({"component":c.component,"mass":c.mass,
            "damaged_mass":c.damaged_mass,"center_of_mass":c.center_of_mass,
            "inertia_diagonal":c.inertia_diagonal,"hull_count":c.convex_hulls.len()})).collect::<Vec<_>>()})
}

pub(super) fn aggregate_mass(children: &[(f32, [f32; 3], [f32; 3])]) -> (f32, [f32; 3], [f32; 3]) {
    let mass = children.iter().map(|c| c.0).sum::<f32>();
    let center = std::array::from_fn(|i| {
        children.iter().map(|c| c.0 * c.1[i]).sum::<f32>() / mass.max(1.0e-6)
    });
    let inertia = std::array::from_fn(|i| {
        children
            .iter()
            .map(|c| {
                c.2[i]
                    + c.0
                        * ((c.1[(i + 1) % 3] - center[(i + 1) % 3]).powi(2)
                            + (c.1[(i + 2) % 3] - center[(i + 2) % 3]).powi(2))
            })
            .sum::<f32>()
            .max(1.0e-5)
    });
    (mass, center, inertia)
}

#[derive(Clone, Copy)]
struct AssemblyLink {
    index: u32,
    parent_part: Option<u32>,
    group: Option<u16>,
    parent_group: Option<u16>,
    detached: bool,
}

fn collect_assembly(links: &[AssemblyLink], root: u32) -> BTreeSet<u32> {
    let Some(part) = links.iter().find(|p| p.index == root && !p.detached) else {
        return BTreeSet::new();
    };
    let mut indices = BTreeSet::from([root]);
    let mut groups = part.group.into_iter().collect::<BTreeSet<_>>();
    loop {
        let mut changed = false;
        for link in links.iter().filter(|p| !p.detached) {
            let physical_member = link.group.is_some_and(|g| groups.contains(&g))
                || link.parent_group.is_some_and(|g| groups.contains(&g));
            let render_member = link.parent_part.is_some_and(|p| indices.contains(&p));
            if physical_member || render_member {
                changed |= indices.insert(link.index);
                if let Some(group) = link.group {
                    changed |= groups.insert(group);
                }
            }
        }
        if !changed {
            break;
        }
    }
    indices
}

/// Physical group ownership is authoritative; render parents additionally retain
/// attached glass/rims in older assets without a complete physical group tree.
/// Already detached assemblies keep their own identity and are never reparented.
pub(super) fn assembly_indices(parts: &[VehiclePresentationPart], root: u32) -> BTreeSet<u32> {
    collect_assembly(
        &parts
            .iter()
            .map(|p| AssemblyLink {
                index: p.index,
                parent_part: p.parent_part_index,
                group: p.physics.as_ref().map(|q| q.group),
                parent_group: p.physics.as_ref().and_then(|q| q.parent_group),
                detached: p.detached_entity.is_some(),
            })
            .collect::<Vec<_>>(),
        root,
    )
}

impl EngineApplication {
    pub(super) fn capture_vehicle_wheel_motion(&mut self, entity: u64) {
        let Some(definition) = self.vehicles.definition(entity) else {
            return;
        };
        let Some(telemetry) = self.vehicles.telemetry(entity) else {
            return;
        };
        let Some(binding) = self.vehicle_presentations.get_mut(&entity) else {
            return;
        };
        for part in &mut binding.parts {
            if part.role != ModelFragmentPartRole::Wheel || part.detached_entity.is_some() {
                continue;
            }
            if let Some(wheel) = resolve_wheel_index(definition, part.wheel_slot)
                .and_then(|i| telemetry.wheels.get(i))
                .filter(|w| w.tire_condition != TireCondition::Missing)
            {
                part.wheel_release_motion = Some([wheel.angular_velocity, wheel.steer_angle]);
                part.wheel_release_condition = Some(wheel.tire_condition);
            }
        }
    }

    pub(super) fn vehicle_fragment_body(
        &self,
        entity: u64,
        parts: &[&VehiclePresentationPart],
        center: [f32; 3],
        half: [f32; 3],
        scale: [f32; 3],
        fallback_hull: Vec<[f32; 3]>,
    ) -> FragmentBody {
        let mut seen = BTreeSet::new();
        let mut children = Vec::new();
        let mut hulls = Vec::new();
        let mut components = Vec::new();
        for part in parts {
            let Some(p) = &part.physics else { continue };
            for child in &p.children {
                if !seen.insert(child.component) {
                    continue;
                }
                components.push(child.component);
                if child.mass <= 0.0 {
                    continue;
                }
                let pose = self
                    .scene
                    .entity_fragment_part_pose(entity, &part.mesh_names);
                let angles = pose.as_ref().map_or([0.0; 3], |p| p.rotation_degrees);
                let com = self.scene.entity_fragment_physics_point(
                    entity,
                    &part.mesh_names,
                    child.center_of_mass,
                );
                let com = std::array::from_fn(|a| (com[a] - center[a]) * scale[a]);
                let axes: [[f32; 3]; 3] = std::array::from_fn(|a| {
                    let mut v = [0.0; 3];
                    v[a] = 1.0;
                    rotate_euler_xyz(v, angles)
                });
                // Physics ABI stores a diagonal tensor. Rotate the source diagonal
                // into the captured vehicle frame and retain its diagonal terms.
                let inertia = std::array::from_fn(|a| {
                    (0..3)
                        .map(|j| axes[j][a].powi(2) * child.inertia_diagonal[j])
                        .sum::<f32>()
                        * ((scale[(a + 1) % 3].powi(2) + scale[(a + 2) % 3].powi(2)) * 0.5)
                });
                children.push((child.mass, com, inertia));
                for h in &child.convex_hulls {
                    let points = h
                        .iter()
                        .map(|point| {
                            let q = self.scene.entity_fragment_physics_point(
                                entity,
                                &part.mesh_names,
                                *point,
                            );
                            std::array::from_fn(|a| (q[a] - center[a]) * scale[a])
                        })
                        .collect();
                    let h = super::collision_hull::fragment_collision_hull(points);
                    if h.len() >= 4 {
                        hulls.push(h);
                    }
                }
            }
        }
        let authored = !children.is_empty();
        let (mass, center_of_mass, inertia) = if authored {
            aggregate_mass(&children)
        } else {
            let mass = match parts.first().map(|p| p.role) {
                Some(ModelFragmentPartRole::Wheel) => 22.0,
                Some(ModelFragmentPartRole::Door) => 28.0,
                Some(ModelFragmentPartRole::Bonnet) => 18.0,
                Some(ModelFragmentPartRole::Boot) => 16.0,
                Some(ModelFragmentPartRole::Roof) => 24.0,
                Some(ModelFragmentPartRole::BodyPanel) => 12.0,
                Some(ModelFragmentPartRole::Spoiler) => 7.0,
                Some(ModelFragmentPartRole::Breakable | ModelFragmentPartRole::Extra) => 6.0,
                _ => 8.0,
            };
            (
                mass,
                [0.0; 3],
                std::array::from_fn(|a| {
                    mass / 3.0 * (half[(a + 1) % 3].powi(2) + half[(a + 2) % 3].powi(2)).max(0.001)
                }),
            )
        };
        if hulls.is_empty() {
            let h = super::collision_hull::fragment_collision_hull(
                fallback_hull
                    .into_iter()
                    .map(|p| std::array::from_fn(|a| p[a] * scale[a]))
                    .collect(),
            );
            if h.len() >= 4 {
                hulls.push(h);
            }
        }
        FragmentBody {
            mass,
            center_of_mass,
            inertia,
            hulls,
            components,
            authored,
        }
    }

    pub(super) fn vehicle_fragment_relative_motion(
        &self,
        entity: u64,
        part: &VehiclePresentationPart,
        center: [f32; 3],
        rotation: [f32; 3],
        scale: [f32; 3],
    ) -> ([f32; 3], [f32; 3]) {
        let mut local_spin = [0.0; 3];
        match part.role {
            ModelFragmentPartRole::Wheel => {
                let motion = self
                    .vehicles
                    .definition(entity)
                    .and_then(|d| resolve_wheel_index(d, part.wheel_slot))
                    .and_then(|i| {
                        self.vehicles.telemetry(entity).and_then(|t| {
                            t.wheels
                                .get(i)
                                .filter(|w| w.tire_condition != TireCondition::Missing)
                                .map(|w| [w.angular_velocity, w.steer_angle])
                        })
                    })
                    .or(part.wheel_release_motion)
                    .unwrap_or([0.0; 2]);
                local_spin =
                    rotate_euler_xyz([-motion[0], 0.0, 0.0], [0.0, motion[1].to_degrees(), 0.0]);
            }
            ModelFragmentPartRole::Door => {
                let side = if part.pivot[0] < 0.0 { -1.0 } else { 1.0 };
                local_spin[1] =
                    part.door_motion.map_or(0.0, |m| m.current_speed) * 70f32.to_radians() * side;
            }
            ModelFragmentPartRole::Bonnet => {
                local_spin[0] =
                    part.door_motion.map_or(0.0, |m| m.current_speed) * 70f32.to_radians()
            }
            ModelFragmentPartRole::Boot => {
                local_spin[0] =
                    -part.door_motion.map_or(0.0, |m| m.current_speed) * 65f32.to_radians()
            }
            _ => {}
        }
        let spin = rotate_euler_xyz(local_spin, rotation);
        let offset = rotate_euler_xyz(
            std::array::from_fn(|i| (center[i] - part.pivot[i]) * scale[i]),
            rotation,
        );
        (detached_fragment_velocity([0.0; 3], spin, offset), spin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assembly_retains_physical_children_and_render_siblings_without_stealing_debris() {
        let links = [
            AssemblyLink {
                index: 0,
                parent_part: None,
                group: Some(0),
                parent_group: None,
                detached: false,
            },
            AssemblyLink {
                index: 1,
                parent_part: Some(0),
                group: Some(1),
                parent_group: Some(0),
                detached: false,
            },
            AssemblyLink {
                index: 2,
                parent_part: Some(0),
                group: Some(1),
                parent_group: Some(0),
                detached: false,
            },
            AssemblyLink {
                index: 3,
                parent_part: None,
                group: Some(2),
                parent_group: Some(1),
                detached: false,
            },
            AssemblyLink {
                index: 4,
                parent_part: Some(1),
                group: None,
                parent_group: None,
                detached: false,
            },
            AssemblyLink {
                index: 5,
                parent_part: Some(1),
                group: Some(3),
                parent_group: Some(1),
                detached: true,
            },
            AssemblyLink {
                index: 6,
                parent_part: Some(5),
                group: Some(4),
                parent_group: Some(3),
                detached: true,
            },
        ];
        assert_eq!(collect_assembly(&links, 1), BTreeSet::from([1, 2, 3, 4]));
        assert!(collect_assembly(&links, 5).is_empty());
    }
    #[test]
    fn malformed_group_cycle_terminates_without_capturing_unrelated_root() {
        let links = [
            AssemblyLink {
                index: 1,
                parent_part: None,
                group: Some(1),
                parent_group: Some(2),
                detached: false,
            },
            AssemblyLink {
                index: 2,
                parent_part: None,
                group: Some(2),
                parent_group: Some(1),
                detached: false,
            },
            AssemblyLink {
                index: 0,
                parent_part: None,
                group: Some(0),
                parent_group: None,
                detached: false,
            },
        ];
        assert_eq!(collect_assembly(&links, 1), BTreeSet::from([1, 2]));
    }

    #[test]
    fn detached_assembly_mass_uses_parallel_axis_without_duplicate_render_parts() {
        let (mass, com, inertia) = aggregate_mass(&[
            (2.0, [-1.0, 0.0, 0.0], [1.0; 3]),
            (2.0, [1.0, 0.0, 0.0], [1.0; 3]),
        ]);
        assert_eq!(mass, 4.0);
        assert_eq!(com, [0.0; 3]);
        assert_eq!(inertia, [2.0, 6.0, 6.0]);
    }
}
