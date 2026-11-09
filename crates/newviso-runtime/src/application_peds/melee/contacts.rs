use super::*;
use newviso_endorphin::math;
impl EngineApplication {
    /// Runs after the final animated pose and attachment update for this frame.
    pub(crate) fn resolve_ped_melee_contacts(&mut self) -> Result<(), String> {
        let now = self.living_world.world_seconds();
        let strikes = self.peds.active_melee_strikes(now);
        if strikes.is_empty() {
            self.ped_physics.melee.samples.clear();
            self.ped_physics.melee.receivers.clear();
            return Ok(());
        }
        let receivers = self.melee_receivers();
        let mut ignore: Vec<u64> = receivers.iter().map(|r| r.entity).collect();
        ignore.sort_unstable();
        ignore.dedup();
        let active_ids: std::collections::BTreeSet<_> =
            strikes.iter().map(|s| s.actor_id.clone()).collect();
        self.ped_physics
            .melee
            .samples
            .retain(|id, _| active_ids.contains(id));
        for strike in strikes {
            let equipped_valid = strike.equipment.as_ref().is_none_or(|e| {
                self.items.equipped_item(&e.owner_id, "hands") == e.item_id.as_deref()
                    && self.items.equipped_generation(&e.owner_id, "hands") == e.generation
            });
            if !equipped_valid || !self.ped_combat_authority(&strike.actor_id) {
                self.peds.cancel_melee(&strike.actor_id, now);
                self.ped_physics.melee.samples.remove(&strike.actor_id);
                self.ped_physics.melee.stale_cancellations += 1;
                continue;
            }
            let Some(current) = self.melee_capsules(&strike) else {
                self.ped_physics.melee.pending_poses += 1;
                continue;
            };
            let previous = self
                .ped_physics
                .melee
                .samples
                .remove(&strike.actor_id)
                .filter(|s| s.sequence == strike.sequence && s.capsules.len() == current.len())
                .unwrap_or_else(|| MeleeSample {
                    sequence: strike.sequence,
                    time: now,
                    capsules: current.clone(),
                });
            self.ped_physics.melee.samples.insert(
                strike.actor_id.clone(),
                MeleeSample {
                    sequence: strike.sequence,
                    time: now,
                    capsules: current.clone(),
                },
            );
            if previous.time > strike.active_end {
                continue;
            }
            let span = (now - previous.time).max(f64::EPSILON);
            let from = ((strike.active_start - previous.time) / span).clamp(0.0, 1.0) as f32;
            let to = ((strike.active_end - previous.time) / span).clamp(0.0, 1.0) as f32;
            if from > to {
                continue;
            }
            let mut best: Option<(StrikeContact, Option<MeleeReceiver>, u64, usize)> = None;
            for (index, (old, new)) in previous.capsules.iter().zip(&current).enumerate() {
                let a = old.interpolate(*new, from);
                let b = old.interpolate(*new, to);
                self.ped_physics.melee.sweeps += 1;
                if now >= strike.active_start && from <= to {
                    for receiver in receivers.iter().filter(|r| r.actor != strike.actor_id) {
                        let old = self
                            .ped_physics
                            .melee
                            .receivers
                            .get(&receiver.key())
                            .copied()
                            .unwrap_or(receiver.capsule);
                        let ra = old.interpolate(receiver.capsule, from);
                        let rb = old.interpolate(receiver.capsule, to);
                        if let Some(mut hit) = sweep_strike_capsules(a, b, ra, rb) {
                            hit.fraction = from + (to - from) * hit.fraction;
                            if best.as_ref().is_none_or(|(h, r, e, _)| {
                                hit.fraction < h.fraction
                                    || (hit.fraction == h.fraction
                                        && r.is_some()
                                        && receiver.entity < *e)
                            }) {
                                best = Some((hit, Some(receiver.clone()), receiver.entity, index));
                            }
                        }
                    }
                }
                let (min, max) = old.bounds(*new);
                let solids = self.scene.physics_static_solid_aabbs_near(&[(min, max)]);
                if let Some((entity, hit)) = self
                    .physics
                    .as_ref()
                    .and_then(|p| p.melee_world_sweep(*old, *new, &ignore, &solids))
                {
                    // A solid wins a coincident contact; a blade cannot cut through cover.
                    if best
                        .as_ref()
                        .is_none_or(|(h, _, _, _)| hit.fraction <= h.fraction)
                    {
                        best = Some((hit, None, entity, index));
                    }
                }
            }
            if let Some((hit, receiver, entity, index)) = best {
                if !self
                    .peds
                    .consume_melee_contact(&strike.actor_id, strike.sequence)
                {
                    continue;
                }
                let material = &strike.action.strikes[index].material;
                let Some(receiver) = receiver else {
                    self.ped_physics.melee.world_blocks += 1;
                    self.peds.obstruct_melee(
                        &strike.actor_id,
                        strike.sequence,
                        previous.time + (now - previous.time) * f64::from(hit.fraction),
                        now,
                    );
                    let event = json!({"actor_id":strike.actor_id,"action":strike.action.id,"sequence":strike.sequence,
                        "position":hit.point,"normal":hit.normal,"surface_entity_key":entity.to_string(),"material":material,"collision":"swept_capsule"});
                    self.ped_physics.melee.last_contact = Some(event.clone());
                    host::publish_event_json("ped.melee.obstruction", "newviso.peds", event)?;
                    continue;
                };
                let same_group = self
                    .peds
                    .state(&strike.actor_id)
                    .zip(self.peds.state(&receiver.actor))
                    .is_some_and(|(a, b)| a.profile.group == b.profile.group);
                if same_group || self.peds.is_dead(&receiver.actor) {
                    continue;
                }
                let views = self.living_world.actor_runtime_views();
                let source = views.iter().find(|a| a.id == strike.actor_id);
                let target = views.iter().find(|a| a.id == receiver.actor);
                let Some((source, target)) = source.zip(target) else {
                    continue;
                };
                let incoming = math::sub(source.position, target.position);
                self.suspend_unavailable_ped_melee(&receiver.actor);
                let (guard_damage, blocked) =
                    self.peds
                        .defend_melee(&receiver.actor, strike.damage, incoming, now);
                let damage = if blocked {
                    guard_damage + (strike.damage - guard_damage) * strike.action.guard_penetration
                } else {
                    strike.damage
                };
                let before = self
                    .peds
                    .state(&receiver.actor)
                    .map_or(0.0, |p| p.health + p.armour);
                let mut command = json!({"op":"ped.damage","actor_id":receiver.actor,"source":strike.actor_id,
                    "amount":damage,"origin":source.position,"hit_point":hit.point,"suppress_reaction":blocked});
                if let Some(part) = &receiver.part {
                    command["hit_part"] = json!(part);
                }
                self.apply_ped_command(&command, 0, "ped.damage")?;
                let applied = self
                    .peds
                    .state(&receiver.actor)
                    .map_or(0.0, |p| p.health + p.armour)
                    < before;
                if !applied && !blocked {
                    continue;
                }
                self.peds.melee_hit(&strike.actor_id, now);
                self.ped_physics.melee.body_contacts += 1;
                let motion = math::sub(current[index].end, previous.capsules[index].end);
                let direction = math::unit(motion).unwrap_or(strike.direction);
                let absorption = if strike.damage > 0.0 {
                    damage / strike.damage
                } else {
                    0.0
                };
                if let Some(physics) = self.physics.as_mut() {
                    physics.apply_impulse_from_script(&json!({"entity":receiver.entity,
                        "impulse":direction.map(|v|v*strike.action.impulse*absorption),"point":hit.point}),0)?;
                }
                let event = json!({"actor_id":strike.actor_id,"target_actor":receiver.actor,
                    "action":strike.action.id,"sequence":strike.sequence,"damage":damage,"blocked":blocked,
                    "hit_part":receiver.part,"hit_zone":receiver.region,"position":hit.point,"normal":hit.normal,
                    "damage_type":strike.action.damage_type,"material":material,"collision":"swept_capsule",
                    "item_id":strike.equipment.as_ref().and_then(|e|e.item_id.as_deref()),"equipment_generation":strike.equipment.as_ref().map(|e|e.generation)});
                self.ped_physics.melee.last_contact = Some(event.clone());
                host::publish_event_json("ped.melee.impact", "newviso.peds", event)?;
                if self
                    .peds
                    .state(&receiver.actor)
                    .is_some_and(|p| p.profile.damage_entity.is_some())
                {
                    host::publish_event_json(
                        "game.player.damage",
                        "newviso.peds",
                        json!({"actor_id":receiver.actor,"kind":"melee","amount":damage,
                        "amplitude":absorption}),
                    )?;
                }
            }
        }
        self.ped_physics.melee.receivers = receivers
            .into_iter()
            .map(|r| (r.key(), r.capsule))
            .collect();
        Ok(())
    }
}
