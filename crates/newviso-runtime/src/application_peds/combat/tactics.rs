use super::*;
use newviso_agent::peds::director::{DirectorModel, TacticalBodyObservation, TacticalPosition};
use newviso_endorphin::math;
#[derive(Debug, Default)]
pub(in crate::application_peds) struct CombatHostRuntime {
    pub models: BTreeMap<String, DirectorModel>,
    pub positions: BTreeMap<String, Vec<TacticalPosition>>,
    pub geometry_probes: u64,
    pub verified_cover: u64,
}
impl EngineApplication {
    pub(in crate::application_peds) fn resolve_ped_combat_model(
        &mut self,
        profile: &mut PedProfile,
    ) -> Result<(), String> {
        if let Some(p) = profile.combat.as_mut() {
            if p.director.is_none() {
                let path = &p.director_asset;
                if !self.ped_physics.combat.models.contains_key(path) {
                    let value = newviso_assets_client::AssetClient::new().json(path)?;
                    let model: DirectorModel = serde_json::from_value(value)
                        .map_err(|e| format!("invalid Combat Director '{path}': {e}"))?;
                    model.validate()?;
                    self.ped_physics.combat.models.insert(path.clone(), model);
                }
                p.director = self.ped_physics.combat.models.get(path).cloned();
            }
        }
        Ok(())
    }
    pub(in crate::application_peds) fn combat_equipment_observations(&mut self) {
        let ids: Vec<_> = self
            .peds
            .states()
            .filter(|p| p.profile.combat.is_some())
            .map(|p| p.profile.actor_id.clone())
            .collect();
        for id in ids {
            let owner = self
                .peds
                .state(&id)
                .and_then(|p| p.profile.melee.as_ref())
                .and_then(|m| m.inventory_owner.as_deref())
                .unwrap_or(&id);
            let weapon = self
                .items
                .equipped_item(owner, "hands")
                .and_then(|i| self.items.definition(i))
                .and_then(|d| d.metadata.get("weapon"));
            let cold = weapon.and_then(|w| w.get("type")).and_then(Value::as_str) == Some("melee");
            let reach = if cold {
                weapon
                    .and_then(|w| w.get("melee_range"))
                    .and_then(Value::as_f64)
                    .map(|v| v as f32)
            } else {
                self.peds
                    .state(&id)
                    .and_then(|p| p.profile.melee.as_ref())
                    .and_then(|m| m.model.as_ref())
                    .map(|m| m.approach_range)
            };
            self.peds.set_combat_melee_weapon(&id, cold, reach);
        }
    }
    pub(in crate::application_peds) fn combat_body_observations(
        &self,
        snapshot: &AgentWorldSnapshot,
    ) -> BTreeMap<String, TacticalBodyObservation> {
        self.peds
            .states()
            .filter(|p| p.profile.combat.is_some())
            .map(|p| {
                let body = self.ped_physics.bodies.get(&p.profile.actor_id);
                let stamina = p
                    .melee
                    .as_ref()
                    .zip(p.profile.melee.as_ref())
                    .map_or(1.0, |(s, m)| s.stamina / m.max_stamina);
                let stance = body
                    .and_then(|b| {
                        b.endorphin.model.anatomy.as_ref().map(|a| {
                            (b.endorphin.damage.function_capacity(a, "left_stance")
                                + b.endorphin.damage.function_capacity(a, "right_stance"))
                                * 0.5
                        })
                    })
                    .unwrap_or(1.0);
                let capacity = ((p.health / p.profile.max_health) * stamina)
                    .clamp(0.0, 1.0)
                    .sqrt()
                    * stance;
                let available = body.is_some_and(|b| b.authority(p.dead).combat)
                    && p.stunned_until <= snapshot.world_seconds;
                let pressure = body.map_or(0.0, |b| b.endorphin.control.brace)
                    + snapshot
                        .stimuli
                        .iter()
                        .filter(|s| {
                            p.combat
                                .as_ref()
                                .is_some_and(|c| s.source == c.target_actor)
                                && s.remaining_seconds > 0.0
                                && matches!(s.kind.as_str(), "gunshot" | "weapon.fire")
                        })
                        .map(|s| s.intensity.max(0.0))
                        .fold(0.0, f32::max);
                (
                    p.profile.actor_id.clone(),
                    TacticalBodyObservation {
                        capacity,
                        available,
                        pressure,
                    },
                )
            })
            .collect()
    }
    pub(in crate::application_peds) fn refresh_tactical_positions(
        &mut self,
        snapshot: &AgentWorldSnapshot,
        due: &std::collections::BTreeSet<String>,
    ) {
        let mut updates = BTreeMap::new();
        let mut probes = 0u64;
        let mut covers = 0u64;
        for id in due {
            let Some(p) = self.peds.state(id) else {
                continue;
            };
            let Some((profile, c)) = p.profile.combat.as_ref().zip(p.combat.as_ref()) else {
                continue;
            };
            let Some(model) = profile.director.as_ref() else {
                continue;
            };
            let Some(actor) = snapshot
                .actors
                .iter()
                .find(|a| a.id == *id && a.enabled && a.simulation_tier == "full")
            else {
                continue;
            };
            let Some(threat) = c.last_seen_position else {
                continue;
            };
            let Some(physics) = self.physics.as_ref() else {
                continue;
            };
            let Some(owner) = self.combat_body_entity(id) else {
                continue;
            };
            let mut ignore = vec![owner];
            if let Some(rig) = self
                .ped_physics
                .bodies
                .get(id)
                .and_then(|b| b.endorphin.ragdoll.as_ref())
            {
                ignore.extend(rig.entities());
            }
            let radius = p.profile.capsule_radius;
            let height = p.profile.capsule_height;
            let extent = model.cover_search_radius;
            let min = actor.position.map(|v| v - extent);
            let max = actor.position.map(|v| v + extent);
            let solids = self.scene.physics_static_solid_aabbs_near(&[(min, max)]);
            // Native AABBs propose goals; exact resident shapes still decide visibility.
            let mut obstacles = solids.clone();
            obstacles.extend(physics.tactical_obstacle_bounds(min, max));
            obstacles.sort_by(|(a, b), (c, d)| {
                a.iter()
                    .chain(b)
                    .zip(c.iter().chain(d))
                    .map(|(x, y)| x.total_cmp(y))
                    .find(|c| !c.is_eq())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            obstacles.dedup();
            let eye = [threat[0], threat[1] + height * 0.75, threat[2]];
            let mut candidates = Vec::new();
            let away = math::unit([
                actor.position[0] - threat[0],
                0.0,
                actor.position[2] - threat[2],
            ])
            .unwrap_or([0.0, 0.0, 1.0]);
            let range = if matches!(c.style.as_str(), "fists" | "melee" | "unarmed") {
                c.melee_reach.unwrap_or_else(|| {
                    p.profile
                        .melee
                        .as_ref()
                        .map_or(p.profile.combat_range, |m| m.range)
                })
            } else {
                p.profile.combat_range
            };
            let distance = math::length(math::sub(actor.position, threat));
            let target_radius = self
                .peds
                .state(&c.target_actor)
                .map_or(radius, |p| p.profile.capsule_radius);
            let scale = if c.tactics.role == "active" {
                model.preferred_range_scale
            } else {
                model.support_radius_scale
            };
            let desired = (range * scale).max(radius + target_radius + model.cover_clearance);
            for angle in [-model.flank_angle_radians, model.flank_angle_radians] {
                let direction = [
                    away[0] * angle.cos() + away[2] * angle.sin(),
                    0.0,
                    away[2] * angle.cos() - away[0] * angle.sin(),
                ];
                candidates.push((
                    "flank",
                    math::add(threat, direction.map(|v| v * desired)),
                    1.0,
                ));
            }
            candidates.push((
                "retreat",
                math::add(
                    actor.position,
                    away.map(|v| v * (desired - distance).max(radius * 2.0)),
                ),
                1.0,
            ));
            let mut nearby: Vec<_> = obstacles
                .iter()
                .filter(|(min, max)| {
                    max[1] - min[1] >= height * 0.5
                        && (max[0] - min[0]).max(max[2] - min[2]) < extent * 2.0
                })
                .collect();
            nearby.sort_by(|(a, b), (c, d)| {
                let center = |min: &[f32; 3], max: &[f32; 3]| {
                    std::array::from_fn(|i| (min[i] + max[i]) * 0.5)
                };
                math::length(math::sub(center(a, b), actor.position))
                    .total_cmp(&math::length(math::sub(center(c, d), actor.position)))
            });
            for (min, max) in nearby {
                let pad = radius + model.cover_clearance;
                let x = [min[0] - pad, (min[0] + max[0]) * 0.5, max[0] + pad];
                let z = [min[2] - pad, (min[2] + max[2]) * 0.5, max[2] + pad];
                // Face centers sample actual cover shadows; outer corners also
                // provide potential peeks. Visibility decides their final role.
                for (i, j) in [
                    (0, 1),
                    (2, 1),
                    (1, 0),
                    (1, 2),
                    (0, 0),
                    (0, 2),
                    (2, 0),
                    (2, 2),
                ] {
                    candidates.push(("cover", [x[i], actor.position[1], z[j]], 1.0));
                }
                if candidates.len() >= model.maximum_position_probes {
                    break;
                }
            }
            let mut accepted = Vec::new();
            for (mut kind, point, quality) in
                candidates.into_iter().take(model.maximum_position_probes)
            {
                probes += 1;
                let Some(point) = physics.tactical_standing_position(
                    actor.position,
                    point,
                    height,
                    radius,
                    model.cover_clearance,
                    self.ped_physics
                        .bodies
                        .get(id)
                        .map(|b| b.endorphin.model.recovery.minimum_support_cosine)
                        .unwrap_or(1.0),
                    &ignore,
                    &solids,
                ) else {
                    continue;
                };
                if kind == "cover" {
                    let goal = [point[0], point[1] + height * 0.75, point[2]];
                    let delta = math::sub(goal, eye);
                    let mut vision_ignore = ignore.clone();
                    if let Some(target) = self.combat_body_entity(&c.target_actor) {
                        vision_ignore.push(target);
                    }
                    let hidden = physics
                        .character_sweep_sphere_ignoring(
                            eye,
                            delta,
                            radius * 0.02,
                            &vision_ignore,
                            &solids,
                        )
                        .is_some();
                    if hidden {
                        covers += 1;
                    } else {
                        kind = "peek";
                    }
                }
                accepted.push(TacticalPosition {
                    position: point,
                    kind: kind.into(),
                    quality,
                    travel_cost: math::length(math::sub(point, actor.position)) / extent,
                });
            }
            updates.insert(id.clone(), accepted);
        }
        self.ped_physics
            .combat
            .positions
            .retain(|id, _| self.peds.state(id).is_some_and(|p| !p.dead));
        self.ped_physics.combat.positions.extend(updates);
        self.ped_physics.combat.geometry_probes += probes;
        self.ped_physics.combat.verified_cover += covers;
    }
}
