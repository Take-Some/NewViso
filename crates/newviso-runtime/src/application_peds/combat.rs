use super::*;
use crate::application_physics::{PhysicsDamageContact, PhysicsDamageKind};
use newviso_agent::{peds::PedState, AgentWorldSnapshot};
mod tactics;
pub(super) use tactics::CombatHostRuntime;

impl EngineApplication {
    pub(crate) fn tick_ped_combat(&mut self, snapshot: &AgentWorldSnapshot) -> Result<(), String> {
        self.combat_equipment_observations();
        let due: std::collections::BTreeSet<_> = self
            .peds
            .combat_probes(snapshot.world_seconds)
            .into_iter()
            .map(|v| v.0)
            .collect();
        let mut observations = BTreeMap::new();
        for (id, target, range) in self.peds.tactical_probes(snapshot) {
            let a = snapshot
                .actors
                .iter()
                .find(|a| a.id == id && a.enabled && a.simulation_tier == "full");
            let b = snapshot
                .actors
                .iter()
                .find(|a| a.id == target && a.enabled && a.simulation_tier == "full");
            let visible = a.zip(b).is_some_and(|(a, b)| {
                let height = self
                    .peds
                    .state(&id)
                    .map_or(0.0, |p| p.profile.capsule_height);
                let target_height = self
                    .peds
                    .state(&target)
                    .map_or(height, |p| p.profile.capsule_height);
                let origin = [a.position[0], a.position[1] + height * 0.75, a.position[2]];
                let goal = [
                    b.position[0],
                    b.position[1] + target_height * 0.6,
                    b.position[2],
                ];
                newviso_endorphin::math::length(newviso_endorphin::math::sub(goal, origin)) <= range
                    && self.clear_combat_line(&id, &target, origin, goal)
            });
            observations.insert((id, target), visible);
        }
        self.peds.select_combat_targets(snapshot, &observations);
        let visibility = self
            .peds
            .states()
            .filter_map(|p| {
                p.combat.as_ref().map(|c| {
                    (
                        p.profile.actor_id.clone(),
                        observations
                            .get(&(p.profile.actor_id.clone(), c.target_actor.clone()))
                            .copied()
                            .unwrap_or(false),
                    )
                })
            })
            .collect();
        let mut commands = self
            .peds
            .tick_combat(snapshot, &visibility, &mut self.agents)?;
        self.refresh_tactical_positions(snapshot, &due);
        let bodies = self.combat_body_observations(snapshot);
        commands.extend(self.peds.tick_combat_director(
            snapshot,
            &bodies,
            &self.ped_physics.combat.positions,
            &mut self.agents,
        )?);
        application_agents::apply_agent_commands(&mut self.living_world, commands)
    }

    /// External controllers retain body/camera ownership; damage uses the same armour and death rules as AI.
    pub(crate) fn apply_external_combat_damage(
        &mut self,
        id: &str,
        contact: PhysicsDamageContact,
    ) -> Result<bool, String> {
        if self.peds.is_dead(id) {
            return Ok(true);
        }
        let vehicle = self.vehicles.contains(contact.source);
        let amount = if matches!(contact.damage_kind, PhysicsDamageKind::Collision) {
            if !vehicle {
                return Ok(true);
            }
            let speed = self.physics.as_ref().map_or(0.0, |p| {
                p.contact_closing_speed(contact.source, contact.target)
            });
            ((speed - 2.5).max(0.0) * 30.0).min(1000.0)
        } else {
            contact.direct_damage
        };
        if amount <= 0.0 {
            return Ok(true);
        }
        if vehicle {
            let now = self.living_world.world_seconds();
            let key = (id.to_owned(), contact.source);
            if self
                .ped_physics
                .last_collision_damage
                .get(&key)
                .is_some_and(|last| now - last < 0.4)
            {
                return Ok(true);
            }
            self.ped_physics.last_collision_damage.insert(key, now);
        }
        let source = self
            .ped_physics
            .weapon_sources
            .get(&contact.source)
            .cloned()
            .unwrap_or_else(|| format!("physics.{}", contact.source));
        let zone = if !vehicle {
            self.living_world
                .actor_runtime_views()
                .into_iter()
                .find(|a| a.id == id)
                .and_then(|actor| {
                    self.peds.state(id).map(|ped| {
                        super::hit_zones::zone_from_contact(
                            contact.point,
                            actor.position,
                            ped.profile.capsule_height,
                            ped.profile.capsule_radius,
                        )
                    })
                })
        } else {
            None
        };
        let final_damage = amount * zone.map_or(1.0, |region| region.multiplier());
        self.peds.damage(
            id,
            source.clone(),
            final_damage,
            contact.point,
            &mut self.agents,
        )?;
        self.peds
            .interrupt_combat(id, self.living_world.world_seconds(), 0.18);
        self.save_ped_vitals(id)?;
        host::publish_event_json(
            "game.player.damage",
            "newviso.peds",
            json!({"actor_id":id,
            "kind":if vehicle {"vehicle"}else{"bullet"},"amount":final_damage,"hit_zone":zone.map(|z|z.name()),"amplitude":(final_damage/25.0).clamp(0.15,1.0)}),
        )?;
        host::publish_event_json(
            "combat.player.hit",
            "newviso.peds",
            json!({"actor_id":id,
            "source":source,"damage":final_damage,"hit_zone":zone.map(|z|z.name()),"position":contact.point,"dead":self.peds.is_dead(id)}),
        )?;
        Ok(true)
    }
}

/// A bounded deterministic cone makes incoming fire avoidable, without frame-rate-dependent RNG.
pub(super) fn shot_direction(direction: [f32; 3], ped: Option<&PedState>) -> [f32; 3] {
    let Some((profile, state)) = ped.and_then(|p| p.profile.combat.as_ref().zip(p.combat.as_ref()))
    else {
        return direction;
    };
    let length = direction
        .iter()
        .map(|n| n * n)
        .sum::<f32>()
        .sqrt()
        .max(0.001);
    let f = direction.map(|n| n / length);
    let horizontal = f[0].hypot(f[2]).max(0.001);
    let right = [-f[2] / horizontal, 0.0, f[0] / horizontal];
    let up = [
        -right[2] * f[1],
        right[2] * f[0] - right[0] * f[2],
        right[0] * f[1],
    ];
    let seed = actor_hash(ped.unwrap().profile.actor_id.as_str())
        .wrapping_add(state.shots.wrapping_mul(2654435761));
    let angle = (seed % 65536) as f32 * std::f32::consts::TAU / 65536.0;
    let radius = (((seed >> 16) % 65536) as f32 / 65535.0).sqrt() * profile.spread;
    std::array::from_fn(|i| f[i] + radius * (angle.cos() * right[i] + angle.sin() * up[i]))
}
