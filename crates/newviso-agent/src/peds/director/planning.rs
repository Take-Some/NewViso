//! Tactical scoring and task admission from current observations.
use super::*;
impl PedRuntime {
    pub fn tick_combat_director(
        &mut self,
        snapshot: &AgentWorldSnapshot,
        bodies: &BTreeMap<String, TacticalBodyObservation>,
        positions: &BTreeMap<String, Vec<TacticalPosition>>,
        agents: &mut AgentRuntime,
    ) -> Result<Vec<AgentCommand>, String> {
        let now = snapshot.world_seconds;
        let target_phases: BTreeMap<String, String> = self
            .records
            .values()
            .filter_map(|p| {
                p.melee
                    .as_ref()
                    .map(|m| (p.profile.actor_id.clone(), m.phase.clone()))
            })
            .collect();
        let mut commands = Vec::new();
        // The Director owns the reaction task through occlusion and memory expiry.
        // A sensing-mode change must not replace cover with a legacy chase task.
        for p in self.records.values_mut() {
            let Some((profile, c)) = p.profile.combat.as_ref().zip(p.combat.as_mut()) else {
                continue;
            };
            if profile.director.is_none() {
                continue;
            }
            let resident = snapshot
                .actors
                .iter()
                .any(|a| a.id == p.profile.actor_id && a.enabled && a.simulation_tier == "full");
            if p.dead || !resident || c.last_seen_position.is_none() {
                if c.tactics.action != "patrol" || c.tactics.goal.is_some() {
                    if let Some(cancel) =
                        agents.clear_task(&p.profile.agent_id, "combat.engagement")?
                    {
                        commands.push(cancel);
                    }
                    c.tactics.action = "patrol".into();
                    c.tactics.goal = None;
                    c.tactics.revisions += 1;
                }
                c.tactics.attack_allowed = false;
                c.tactics.permit_until = 0.0;
                if let Some(m) = p.melee.as_mut() {
                    m.guard_requested = false;
                    m.blocking = false;
                }
            }
        }
        let mut participants = Vec::new();
        for p in self.records.values_mut().filter(|p| !p.dead) {
            let Some((profile, c)) = p.profile.combat.as_ref().zip(p.combat.as_mut()) else {
                continue;
            };
            let Some(model) = &profile.director else {
                continue;
            };
            let Some(a) = snapshot
                .actors
                .iter()
                .find(|a| a.id == p.profile.actor_id && a.enabled && a.simulation_tier == "full")
            else {
                continue;
            };
            let Some(target) = c.last_seen_position else {
                continue;
            };
            let physical = bodies.get(&p.profile.actor_id);
            let capacity = physical.map_or(1.0, |b| b.capacity).clamp(0.0, 1.0);
            let available = physical.is_none_or(|b| b.available);
            let stamina = p
                .melee
                .as_ref()
                .zip(p.profile.melee.as_ref())
                .map_or(1.0, |(s, m)| s.stamina / m.max_stamina);
            let distance = distance_between(a.position, target);
            let elapsed = (now - c.tactics.last_observed).max(0.0) as f32;
            // A permit is permission to approach, not evidence of service.
            // Keep priority until an actual melee action or shot starts.
            let service = [
                p.melee.as_ref().map_or(0, |m| u64::from(m.attacks)),
                u64::from(c.shots),
            ];
            if service != c.tactics.last_service {
                c.tactics.waiting_seconds = 0.0;
                c.tactics.last_service = service;
            } else if c.visible {
                c.tactics.waiting_seconds += elapsed;
            }
            c.tactics.last_observed = now;
            let ready = now >= c.ready_at && now >= p.stunned_until;
            let ranged = !matches!(c.style.as_str(), "fists" | "melee" | "unarmed");
            let duration = if ranged {
                p.profile.combat_cooldown_seconds
            } else {
                p.profile
                    .melee
                    .as_ref()
                    .map_or(p.profile.combat_cooldown_seconds, |m| {
                        m.windup_seconds
                            + m.recovery_seconds
                            + p.melee
                                .as_ref()
                                .and_then(|s| s.action.as_ref())
                                .map_or(0.0, |a| a.active_seconds)
                    })
            };
            let score = model.weights.distance
                / (1.0 + distance / p.profile.combat_range.max(p.profile.capsule_radius))
                + model.weights.readiness * indicator(ready)
                + model.weights.vitality * capacity
                + model.weights.stamina * stamina
                + model.weights.waiting * c.tactics.waiting_seconds / duration.max(f32::EPSILON);
            participants.push(Participant {
                id: p.profile.actor_id.clone(),
                target: c.target_actor.clone(),
                group: p.profile.group.clone(),
                position: a.position,
                target_position: target,
                ranged,
                eligible: c.mode == "engage"
                    && c.visible
                    && available
                    && capacity > 0.0
                    && c.style != "unarmed",
                action_running: p
                    .melee
                    .as_ref()
                    .is_some_and(|s| matches!(s.phase.as_str(), "windup" | "active")),
                score,
                duration,
                model: model.clone(),
            });
        }
        let roles = self.director.allocate(now, &participants);
        let mut reserved: Vec<[f32; 3]> = Vec::new();
        for participant in participants {
            let p = self
                .records
                .get_mut(&participant.id)
                .expect("director participant");
            let c = p.combat.as_mut().expect("combat state");
            let model = &participant.model;
            let (role, until) = roles
                .get(&participant.id)
                .cloned()
                .unwrap_or(("observer".into(), 0.0));
            c.tactics.role = if role == "support"
                && distance_between(participant.position, participant.target_position)
                    > p.profile.combat_range * model.support_radius_scale
            {
                "observer".into()
            } else {
                role
            };
            c.tactics.permit_until = until;
            c.tactics.target_actor = c.target_actor.clone();
            if c.tactics.role != "active"
                || !c.visible
                || bodies.get(&participant.id).is_some_and(|b| !b.available)
                || now < p.stunned_until
            {
                c.tactics.attack_allowed = false;
            }
            if now < c.tactics.next_decision {
                continue;
            }
            c.tactics.next_decision = now + f64::from(model.sense_interval);
            let body = bodies.get(&participant.id);
            let capacity = body.map_or(1.0, |b| b.capacity).clamp(0.0, 1.0);
            let available = body.is_none_or(|b| b.available);
            let pressure = body.map_or(0.0, |b| b.pressure).max(0.0);
            let distance = distance_between(participant.position, participant.target_position);
            let range = if participant.ranged {
                p.profile.combat_range
            } else {
                c.melee_reach.unwrap_or_else(|| {
                    p.profile
                        .melee
                        .as_ref()
                        .map_or(p.profile.combat_range, |m| m.range)
                })
            };
            let stamina = p
                .melee
                .as_ref()
                .zip(p.profile.melee.as_ref())
                .map_or(1.0, |(s, m)| s.stamina / m.max_stamina);
            let target = target_phases.get(&participant.target);
            let target_attack = c.visible
                && target.is_some_and(|phase| matches!(phase.as_str(), "windup" | "active"));
            let target_recovering = c.visible && target.is_some_and(|phase| phase == "recovery");
            let ready = now >= c.ready_at && now >= p.stunned_until;
            let can_attack = available
                && ready
                && c.visible
                && c.tactics.role == "active"
                && if participant.ranged {
                    c.magazine > 0 && c.reload_until <= now
                } else {
                    stamina > 0.0
                };
            let incoming = indicator(target_attack) / (1.0 + distance / range);
            let opportunity = |kind: &str| {
                positions
                    .get(&participant.id)
                    .into_iter()
                    .flatten()
                    .filter(|v| {
                        v.kind == kind
                            && reserved.iter().all(|r| {
                                distance_between(*r, v.position) > p.profile.capsule_radius * 2.0
                            })
                    })
                    .max_by(|a, b| {
                        (a.quality / (1.0 + a.travel_cost))
                            .total_cmp(&(b.quality / (1.0 + b.travel_cost)))
                    })
            };
            let cover = opportunity("cover");
            let flank = opportunity("flank");
            let peek = opportunity("peek");
            let mut choices: Vec<(&str, f32, Option<[f32; 3]>)> = Vec::new();
            if c.mode == "flee" {
                choices.push((
                    "flee",
                    model.weights.retreat + model.weights.readiness,
                    Some(participant.target_position),
                ));
            } else if !c.visible {
                choices.push((
                    "search",
                    model.weights.observe + model.switch_cost,
                    Some(participant.target_position),
                ));
                if participant.ranged && c.reload_until <= now && c.magazine > 0 {
                    if let Some(v) = peek {
                        choices.push((
                            "peek",
                            model.weights.flank * v.quality + model.weights.observe,
                            Some(v.position),
                        ));
                    }
                }
            }
            if can_attack {
                choices.push((
                    if target_recovering && !participant.ranged {
                        "counter"
                    } else {
                        "attack"
                    },
                    model.weights.readiness * capacity * stamina
                        + model.weights.counter * indicator(target_recovering),
                    None,
                ));
            }
            if !participant.ranged && available && stamina > 0.0 && target_attack {
                choices.push((
                    "guard",
                    model.weights.guard * incoming * stamina,
                    Some(participant.position),
                ));
            }
            if let Some(v) = cover {
                choices.push((
                    "cover",
                    model.weights.cover
                        * (pressure + (1.0 - capacity) + indicator(c.reload_until > now))
                        * v.quality,
                    Some(v.position),
                ));
            }
            if let Some(v) = flank {
                choices.push((
                    "flank",
                    model.weights.flank
                        * v.quality
                        * indicator(c.tactics.role != "active" || !c.visible),
                    Some(v.position),
                ));
            }
            let retreat = (1.0 - capacity)
                + (1.0 - stamina)
                + ((range * model.preferred_range_scale - distance) / range).max(0.0);
            if available && retreat > 0.0 {
                if let Some(v) = positions
                    .get(&participant.id)
                    .into_iter()
                    .flatten()
                    .filter(|v| v.kind == "retreat")
                    .max_by(|a, b| a.quality.total_cmp(&b.quality))
                {
                    choices.push(("retreat", model.weights.retreat * retreat, Some(v.position)));
                }
            }
            choices.push((
                if available { "observe" } else { "recover" },
                model.weights.observe,
                Some(participant.position),
            ));
            if c.mode == "flee" {
                choices.retain(|(a, _, _)| *a == "flee");
            }
            if participant.action_running {
                choices.retain(|(a, _, _)| matches!(*a, "attack" | "counter" | "recover"));
            }
            let old = c.tactics.action.clone();
            let old_goal = c.tactics.goal;
            let chosen = choices.iter().max_by(|a, b| {
                let sa = a.1 + if a.0 == old { model.switch_cost } else { 0.0 };
                let sb = b.1 + if b.0 == old { model.switch_cost } else { 0.0 };
                sa.total_cmp(&sb).then(a.0.cmp(b.0))
            });
            let Some((action, utility, goal)) = chosen.copied() else {
                continue;
            };
            c.tactics.action = action.into();
            c.tactics.utility = utility;
            c.tactics.goal = goal;
            c.tactics.attack_allowed = can_attack && matches!(action, "attack" | "counter");
            if let Some(m) = p.melee.as_mut() {
                m.guard_requested = action == "guard";
                m.blocking =
                    m.guard_requested && m.phase == "idle" && !m.guard_broken && m.stamina > 0.0;
            }
            if let Some(goal) = goal {
                reserved.push(goal);
            }
            let changed = now >= c.tactics.task_until
                || old != action
                || old_goal
                    .zip(goal)
                    .is_some_and(|(a, b)| distance_between(a, b) > model.repath_distance)
                || old_goal.is_some() != goal.is_some();
            if !changed {
                continue;
            }
            c.tactics.revisions += 1;
            c.tactics.task_until = now + f64::from(model.engagement_seconds);
            if let Some(cancel) = agents.clear_task(&p.profile.agent_id, "combat.engagement")? {
                commands.push(cancel);
            }
            let task = if action == "flee" {
                AgentTaskKind::Flee {
                    threat_position: participant.target_position,
                    threat_actor: None,
                    speed: p.profile.move_speed,
                    safe_distance: p.profile.flee_distance,
                    duration_seconds: p.profile.flee_seconds,
                }
            } else if matches!(action, "attack" | "counter") {
                AgentTaskKind::Combat {
                    target_actor: c.target_actor.clone(),
                    speed: p.profile.move_speed,
                    range,
                    cooldown_seconds: if participant.ranged {
                        p.profile.combat_cooldown_seconds
                    } else {
                        model.melee_request_interval
                    },
                    damage: p.profile.combat_damage,
                    duration_seconds: model.engagement_seconds,
                }
            } else {
                AgentTaskKind::GoToPosition {
                    position: goal.unwrap_or(participant.position),
                    speed: p.profile.move_speed,
                    // Tactical visibility was verified at the goal. Arrive within the
                    // authored clearance rather than stopping a body radius short.
                    stop_distance: model.cover_clearance.min(p.profile.capsule_radius),
                }
            };
            agents.set_task(
                &p.profile.agent_id,
                AgentTaskDesc {
                    id: "combat.engagement".into(),
                    lane: AgentTaskLane::Reaction,
                    priority: 90,
                    task,
                },
            )?;
            self.events.push(json!({"kind":"ped.combat.tactic","actor_id":participant.id,"target_actor":participant.target,
                "role":c.tactics.role,"action":action,"goal":goal,"utility":utility}));
        }
        Ok(commands)
    }
}
