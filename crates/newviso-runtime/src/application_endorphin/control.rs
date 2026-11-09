use super::*;
use newviso_endorphin::BodyPose;

impl EndorphinRuntime {
    /// Feedback uses the preceding completed observation on every physics tick.
    pub(crate) fn update_controls(
        peds: &newviso_agent::peds::PedRuntime,
        bodies: &mut BTreeMap<String, PedBody>,
        physics: &mut PhysicsRuntime,
        scene_solids: &[([f32; 3], [f32; 3])],
    ) {
        let (tick, fixed_dt) = physics.fixed_clock();
        for (id, body) in bodies.iter_mut() {
            let Some(rig) = &body.endorphin.ragdoll else {
                continue;
            };
            let dead = peds.is_dead(id) || body.reset_requested;
            let Some(anatomy) = body.endorphin.model.anatomy.as_ref() else {
                continue;
            };
            if dead {
                body.endorphin.control.activation = 0.0;
                body.endorphin.control.balance_torque = [0.0; 3];
                body.endorphin.control.joint_torques.clear();
                body.endorphin.control.world_drives.clear();
                body.endorphin.control.stance_hips.clear();
                body.endorphin.control.plants.targets.clear();
                body.endorphin.control.stepping = Default::default();
                body.endorphin.control.joint_targets.clear();
                body.endorphin.control.protection.reset();
            } else if let Some(dt) = body.endorphin.control_clock.sample(tick, fixed_dt) {
                let support = physics.endorphin_support(
                    rig.entities(),
                    body.endorphin.model.recovery.minimum_support_cosine,
                    physics
                        .vehicle_body_state(body.entity)
                        .map_or(f32::MIN, |p| p.position[1]),
                );
                let energy = rig.kinetic_energy_per_mass(
                    support.map_or([0.0; 3], |(_, s)| s.velocity),
                    |e| {
                        physics.vehicle_body_state(e).map(|p| BodyPose {
                            position: p.position,
                            rotation: p.rotation,
                            linear_velocity: p.linear_velocity,
                            angular_velocity: p.angular_velocity,
                        })
                    },
                );
                let capacity = peds.state(id).map_or(0.0, |p| {
                    let stamina = p
                        .melee
                        .as_ref()
                        .zip(p.profile.melee.as_ref())
                        .map_or(1.0, |(s, m)| (s.stamina / m.max_stamina).clamp(0.0, 1.0));
                    ((p.health / p.profile.max_health).clamp(0.0, 1.0) * stamina).sqrt()
                });
                let mut com = [0.0; 3];
                let mut velocity = [0.0; 3];
                let mut mass = 0.0;
                for part in &rig.parts {
                    if let Some(p) = physics.vehicle_body_state(part.entity) {
                        for i in 0..3 {
                            com[i] += p.position[i] * part.mass_kg;
                            velocity[i] += p.linear_velocity[i] * part.mass_kg;
                        }
                        mass += part.mass_kg;
                    }
                }
                if mass > 0.0 {
                    com = com.map(|v| v / mass);
                    velocity = velocity.map(|v| v / mass);
                }
                let joint_position = |name: &str| {
                    rig.parts.iter().find(|p| p.joint == name).and_then(|part| {
                        physics.vehicle_body_state(part.entity).map(|p| {
                            newviso_endorphin::math::position(newviso_endorphin::math::product(
                                newviso_endorphin::math::matrix(p.position, p.rotation),
                                part.body_to_joint,
                            ))
                        })
                    })
                };
                let up = joint_position(&anatomy.orientation.upper)
                    .zip(joint_position(&anatomy.orientation.lower))
                    .map_or([0.0; 3], |(a, b)| newviso_endorphin::math::sub(a, b));
                let contacts = &mut body.endorphin.support_contacts;
                contacts.clear();
                body.endorphin.balance_contacts.clear();
                for (index, part) in rig.parts.iter().enumerate() {
                    if !rig.stance_entities.contains(&part.entity) {
                        continue;
                    }
                    let points = &mut body.endorphin.contact_points;
                    points.clear();
                    physics.endorphin_contact_points(
                        &[part.entity],
                        rig.entities(),
                        body.endorphin.model.recovery.minimum_support_cosine,
                        points,
                    );
                    contacts.extend(points.iter().map(|position| {
                        newviso_endorphin::SupportContact {
                            part: index,
                            position: *position,
                        }
                    }));
                    body.endorphin.balance_contacts.extend_from_slice(points);
                }
                let previous_normal = body.endorphin.control.balance.support_normal;
                body.endorphin.control.balance = newviso_endorphin::measure_balance(
                    &body.endorphin.balance_contacts,
                    com,
                    velocity,
                    support.map_or([0.0; 3], |(_, s)| s.velocity),
                    up,
                    physics.gravity_magnitude(),
                    anatomy.balance.footprint_padding,
                    anatomy.balance.max_lean_angle,
                );
                body.endorphin.control.balance.support_normal = support.map_or(
                    if newviso_endorphin::math::length(previous_normal) > f32::EPSILON {
                        previous_normal
                    } else {
                        [0.0, 1.0, 0.0]
                    },
                    |(_, s)| s.normal,
                );
                let support_plan = rig.support_plan_with_torso(
                    contacts,
                    &body.endorphin.control.balance,
                    com,
                    velocity,
                    support.map_or([0.0; 3], |(_, s)| s.velocity),
                    physics.gravity_magnitude(),
                    anatomy.balance.gains,
                    body.endorphin.control.torso_target,
                    body.endorphin.control.height_target,
                    body.endorphin
                        .control
                        .stepping
                        .support_goal
                        .or(body.endorphin.control.support_target),
                    |e| {
                        physics.vehicle_body_state(e).map(|p| BodyPose {
                            position: p.position,
                            rotation: p.rotation,
                            linear_velocity: p.linear_velocity,
                            angular_velocity: p.angular_velocity,
                        })
                    },
                );
                body.endorphin.control.balance_torque =
                    support_plan.as_ref().map_or([0.0; 3], |p| p.root_moment);
                body.endorphin.control.joint_torques.clear();
                body.endorphin.control.world_drives.clear();
                body.endorphin.control.stance_hips.clear();
                if let Some(plan) = support_plan {
                    body.endorphin.control.world_drives = plan.world_drives;
                    body.endorphin.control.stance_hips = plan.stance_hips;
                    body.endorphin
                        .control
                        .joint_torques
                        .extend(plan.joint_torques);
                }
                let plant = if anatomy.balance.stepping.is_none() {
                    body.endorphin.control.plants.correct(
                        rig,
                        contacts,
                        support.map_or([0.0; 3], |(_, s)| s.velocity),
                        physics.gravity_magnitude(),
                        2.0 * (body.radius + body.half_height),
                        body.endorphin.control.balance.uprightness,
                        body.endorphin.control.brace,
                        anatomy.balance.gains,
                        body.endorphin.model.recovery.response_time_scale,
                        dt,
                        |e| {
                            physics.vehicle_body_state(e).map(|p| BodyPose {
                                position: p.position,
                                rotation: p.rotation,
                                linear_velocity: p.linear_velocity,
                                angular_velocity: p.angular_velocity,
                            })
                        },
                    )
                } else {
                    body.endorphin.control.plants.observe(
                        rig,
                        contacts,
                        support.map_or([0.0; 3], |(_, s)| s.velocity),
                        physics.gravity_magnitude(),
                        2.0 * (body.radius + body.half_height),
                        body.endorphin.model.recovery.response_time_scale,
                        dt,
                        |e| {
                            physics.vehicle_body_state(e).map(|p| BodyPose {
                                position: p.position,
                                rotation: p.rotation,
                                linear_velocity: p.linear_velocity,
                                angular_velocity: p.angular_velocity,
                            })
                        },
                    );
                    Vec::new()
                };
                if body.endorphin.control.joint_torques.len() != rig.joints.len() {
                    body.endorphin
                        .control
                        .joint_torques
                        .resize(rig.joints.len(), [0.0; 3]);
                }
                for (index, (torque, correction)) in body
                    .endorphin
                    .control
                    .joint_torques
                    .iter_mut()
                    .zip(plant)
                    .enumerate()
                {
                    if anatomy.balance.stepping.is_none()
                        && !body.endorphin.control.stance_hips.contains(&index)
                    {
                        *torque = newviso_endorphin::math::add(*torque, correction);
                    }
                }
                if let Some(model) = anatomy.balance.stepping.as_ref() {
                    body.endorphin.control.joint_targets = body.endorphin.control.stepping.plan(
                        model,
                        rig,
                        &body.endorphin.control.balance,
                        contacts,
                        com,
                        velocity,
                        physics.gravity_magnitude(),
                        capacity,
                        dt,
                        |e| {
                            physics.vehicle_body_state(e).map(|p| BodyPose {
                                position: p.position,
                                rotation: p.rotation,
                                linear_velocity: p.linear_velocity,
                                angular_velocity: p.angular_velocity,
                            })
                        },
                        |point, radius, reach| {
                            physics.endorphin_foot_ground(
                                point,
                                radius,
                                reach,
                                rig.entities(),
                                scene_solids,
                                body.endorphin.model.recovery.minimum_support_cosine,
                            )
                        },
                        |from, to, radius| {
                            physics.endorphin_foot_path_clear(
                                from,
                                to,
                                radius,
                                rig.entities(),
                                scene_solids,
                            )
                        },
                    );
                }
                body.endorphin.control.observe(
                    anatomy,
                    energy.unwrap_or(0.0),
                    physics.gravity_magnitude(),
                    2.0 * (body.radius + body.half_height),
                    capacity,
                    rig.elastic_energy_per_mass(),
                    support.map_or(0.0, |(_, s)| s.normal[1].clamp(0.0, 1.0)),
                    dt,
                );
                super::protection::physical::update(
                    rig,
                    &body.endorphin.protection_profile,
                    &body.endorphin.protection_offsets,
                    &body.endorphin.damage,
                    &mut body.endorphin.control.protection,
                    body.radius,
                    physics,
                    scene_solids,
                    dt,
                    capacity,
                );
                body.endorphin.control.activation = body
                    .endorphin
                    .control
                    .activation
                    .max(capacity * body.endorphin.control.protection.activation);
            }
            let protective_targets = if dead {
                Vec::new()
            } else {
                super::protection::physical::targets(body, physics)
            };
            for (index, joint) in rig.joints.iter().enumerate() {
                let local = body
                    .endorphin
                    .damage
                    .part_capacity(&rig.parts[joint.child_part].name)
                    .min(
                        body.endorphin
                            .damage
                            .part_capacity(&rig.parts[joint.parent_part].name),
                    );
                let current = physics
                    .vehicle_body_state(joint.parent)
                    .zip(physics.vehicle_body_state(joint.child))
                    .map(|(parent, child)| {
                        newviso_endorphin::math::multiply(
                            newviso_endorphin::math::inverse(newviso_endorphin::math::multiply(
                                parent.rotation,
                                joint.parent_frame_rotation,
                            )),
                            newviso_endorphin::math::multiply(
                                child.rotation,
                                joint.child_frame_rotation,
                            ),
                        )
                    });
                let mut motor = if dead {
                    None
                } else {
                    joint.muscle.and_then(|m| {
                        body.endorphin.control.command(
                            m,
                            joint.initial_rotation,
                            current.unwrap_or(joint.initial_rotation),
                            local,
                        )
                    })
                };
                if let (Some(m), Some(torque)) =
                    (&mut motor, body.endorphin.control.joint_torques.get(index))
                {
                    if let Some(parent) = physics.vehicle_body_state(joint.parent) {
                        let frame = newviso_endorphin::math::multiply(
                            parent.rotation,
                            joint.parent_frame_rotation,
                        );
                        m.feed_forward_torque = newviso_endorphin::math::rotate(
                            newviso_endorphin::math::inverse(frame),
                            torque.map(|v| {
                                v * anatomy
                                    .balance
                                    .joint_weights
                                    .get(&rig.parts[joint.child_part].name)
                                    .copied()
                                    .unwrap_or(1.0)
                            }),
                        );
                    }
                }
                if let (Some(m), Some(Some(target))) =
                    (&mut motor, body.endorphin.control.joint_targets.get(index))
                {
                    m.target_rotation = newviso_endorphin::math::slerp(
                        m.target_rotation,
                        *target,
                        1.0 - body.endorphin.control.brace,
                    );
                }
                if let (Some(m), Some(Some((target, weight)))) =
                    (&mut motor, protective_targets.get(index))
                {
                    m.target_rotation =
                        newviso_endorphin::math::slerp(m.target_rotation, *target, *weight);
                }
                physics.set_joint_motor(
                    joint.id,
                    motor.map(|m| newviso_physics_client::PhysicsJointMotor {
                        world_drive: body
                            .endorphin
                            .control
                            .world_drives
                            .get(index)
                            .and_then(|g| *g)
                            .map(|g| newviso_physics_client::PhysicsWorldAngularDrive {
                                body: match g.body {
                                    newviso_endorphin::AngularDriveBody::Parent => {
                                        newviso_physics_client::PhysicsJointBody::Parent
                                    }
                                    newviso_endorphin::AngularDriveBody::Child => {
                                        newviso_physics_client::PhysicsJointBody::Child
                                    }
                                },
                                target_rotation: g.target_rotation,
                                stiffness: g.stiffness,
                                damping: g.damping,
                            }),
                        target_rotation: m.target_rotation,
                        feed_forward_torque: m.feed_forward_torque,
                        stiffness: m.stiffness,
                        damping: m.damping,
                        max_torque: m.max_torque,
                    }),
                );
            }
        }
    }
}
