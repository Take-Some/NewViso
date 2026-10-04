use super::*;

impl PhysicsRuntime {
    pub(crate) fn step(
        &mut self,
        dt: f32,
        scene_solids: &[(u64, [f32; 3], [f32; 3])],
        vehicles: &mut VehicleRuntime,
    ) -> Result<(), String> {
        if !dt.is_finite() || dt <= 0.0 {
            return Ok(());
        }

        self.frame_index = self.frame_index.wrapping_add(1);
        self.accumulator += dt;

        let fixed_dt = self.settings.fixed_dt();
        let max_steps = self.settings.max_steps_per_frame;
        let mut steps = 0usize;
        let mut frame_events = Vec::new();
        self.ballistic_damage_contacts.clear();
        self.ballistic_impacts.clear();
        while self.accumulator + 1.0e-7 >= fixed_dt && steps < max_steps {
            self.fixed_tick = self.fixed_tick.wrapping_add(1);

            let mut bodies = if self.settings.scene_colliders_enabled {
                static_scene_bodies(scene_solids, self.settings)
                    .into_iter()
                    // A resident CollisionMeshResource with the same scene entity is
                    // already represented by its exact streamed triangle mesh. Never
                    // add the generic SceneWorld AABB for that entity as a second body:
                    // it can create a false support plane (notably the 1 m fallback
                    // bounds on YBN-only entities).
                    .filter(|body| {
                        !self.bodies.contains_key(&body.entity)
                            && !self.streamed_colliders.contains_key(&body.entity)
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            bodies.extend(self.bodies.values().cloned());

            let vehicle_states = vehicles
                .entity_ids()
                .filter_map(|entity| {
                    self.bodies.get(&entity).map(|body| {
                        (
                            entity,
                            VehicleBodyState {
                                position: body.position,
                                rotation: body.rotation,
                                linear_velocity: body.linear_velocity,
                                angular_velocity: body.angular_velocity,
                            },
                        )
                    })
                })
                .collect::<BTreeMap<_, _>>();
            let vehicle_plan =
                vehicles.prepare_frame(fixed_dt, self.settings.gravity.abs(), &vehicle_states);

            let mut commands = if steps == 0 {
                std::mem::take(&mut self.pending_commands)
            } else {
                Vec::new()
            };
            for impulse in vehicle_plan.impulses {
                let seq = self.next_command_seq;
                self.next_command_seq = self.next_command_seq.wrapping_add(1).max(1);
                commands.push(PhysicsCommand {
                    seq,
                    kind: PhysicsCommandKind::ApplyImpulse {
                        entity: impulse.vehicle,
                        impulse: impulse.impulse,
                        point: impulse.point,
                    },
                });
            }
            for angular in vehicle_plan.angular_velocity_deltas {
                let Some(body) = self.bodies.get(&angular.vehicle) else {
                    continue;
                };
                let velocity =
                    std::array::from_fn(|axis| body.angular_velocity[axis] + angular.delta[axis]);
                let seq = self.next_command_seq;
                self.next_command_seq = self.next_command_seq.wrapping_add(1).max(1);
                commands.push(PhysicsCommand {
                    seq,
                    kind: PhysicsCommandKind::SetAngularVelocity {
                        entity: angular.vehicle,
                        velocity,
                    },
                });
            }

            let colliders = if steps == 0 {
                std::mem::take(&mut self.pending_streamed_colliders)
                    .into_values()
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let mut queries = if steps == 0 {
                std::mem::take(&mut self.pending_queries)
            } else {
                Vec::new()
            };
            queries.extend(vehicle_plan.probes.into_iter().map(|probe| PhysicsQuery {
                seq: probe.seq,
                ignore_entity: Some(probe.vehicle),
                kind: PhysicsQueryKind::Ray {
                    origin: probe.origin,
                    dir: probe.direction,
                    max_t: probe.max_distance,
                },
            }));

            let submitted_ballistic = queries
                .iter()
                .filter_map(|query| {
                    self.pending_ballistic_damage
                        .contains_key(&query.seq)
                        .then_some(query.seq)
                })
                .collect::<BTreeSet<_>>();
            let input = PhysicsFrameInput {
                frame_index: self.frame_index,
                fixed_tick: self.fixed_tick,
                dt: fixed_dt,
                gravity: self.settings.gravity,
                contact_skin: self.settings.contact_skin,
                bodies,
                colliders,
                commands,
                queries,
            };
            let mut output = match self.client.step_frame_ref(&input) {
                Ok(output) => output,
                Err(error) => {
                    for collider in input.colliders {
                        self.pending_streamed_colliders
                            .insert(collider.entity, collider);
                    }
                    if !input.commands.is_empty() {
                        self.pending_commands.splice(0..0, input.commands);
                    }
                    if !input.queries.is_empty() {
                        self.pending_queries.splice(0..0, input.queries);
                    }
                    return Err(error);
                }
            };

            self.enrich_contact_surfaces(&mut output);
            self.apply_output(&output);
            let mut completed_ballistic = BTreeSet::new();
            for hit in &output.query_hits {
                let Some(source) = self.pending_ballistic_damage.get(&hit.seq).copied() else {
                    continue;
                };
                // BallisticRay hits are ordered by distance/hit_index. For standard
                // bullets apply damage to the first hit only; penetration can be
                // added later by explicitly increasing a shot's policy.
                if completed_ballistic.insert(hit.seq) {
                    let falloff = if hit.distance <= source.falloff_min {
                        1.0
                    } else if hit.distance >= source.falloff_max {
                        source.falloff_modifier
                    } else {
                        let span = (source.falloff_max - source.falloff_min).max(1.0e-6);
                        let t = ((hit.distance - source.falloff_min) / span).clamp(0.0, 1.0);
                        1.0 + (source.falloff_modifier - 1.0) * t
                    };
                    self.ballistic_damage_contacts.push(PhysicsDamageContact {
                        source: source.source,
                        target: hit.entity,
                        damage_kind: source.damage_kind,
                        direct_damage: source.damage * falloff,
                        contact_impulse: source.impulse * falloff,
                        point: hit.position,
                        impulse_direction: source.direction,
                    });
                    self.ballistic_impacts.push(PhysicsBallisticImpact {
                        source: source.source,
                        target: hit.entity,
                        point: hit.position,
                        normal: hit.normal,
                        distance: hit.distance,
                        surface_entity: Some(hit.entity),
                        surface_id: self.resolve_surface_id(hit.entity, hit.position),
                    });
                }
            }
            // A submitted query is complete even when it missed. Retire its
            // damage metadata so misses do not leak bookkeeping indefinitely.
            for seq in submitted_ballistic {
                self.pending_ballistic_damage.remove(&seq);
            }

            let vehicle_hits = output
                .query_hits
                .iter()
                .map(|hit| VehicleProbeHit {
                    seq: hit.seq,
                    position: hit.position,
                    normal: hit.normal,
                    distance: hit.distance,
                    surface_entity: Some(hit.entity),
                    surface_id: self.resolve_surface_id(hit.entity, hit.position),
                })
                .collect::<Vec<_>>();
            vehicles.accept_probe_hits(&vehicle_hits);
            frame_events.extend(output.events.iter().cloned());
            self.last_output = output;
            self.accumulator -= fixed_dt;
            steps += 1;
        }

        self.last_output.events = frame_events;
        if steps == max_steps && self.accumulator >= fixed_dt {
            self.accumulator = self.accumulator.min(fixed_dt);
        }

        Ok(())
    }

    pub(super) fn apply_output(&mut self, output: &PhysicsFrameOutput) {
        for pose in &output.pose_updates {
            let Some(body) = self.bodies.get_mut(&pose.entity) else {
                continue;
            };
            body.position = pose.position;
            body.rotation = pose.rotation;
            refresh_bounds(body);
        }

        for velocity in &output.velocity_updates {
            let Some(body) = self.bodies.get_mut(&velocity.entity) else {
                continue;
            };
            body.linear_velocity = velocity.linear_velocity;
            body.angular_velocity = velocity.angular_velocity;
        }
    }
}
