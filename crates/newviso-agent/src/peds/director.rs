//! Tactical allocation over observed combatants. Geometry and asset loading stay in the host.
use super::*;
fn indicator(value: bool) -> f32 {
    u8::from(value) as f32
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectorWeights {
    pub distance: f32,
    pub readiness: f32,
    pub vitality: f32,
    pub stamina: f32,
    pub waiting: f32,
    pub threat: f32,
    pub counter: f32,
    pub guard: f32,
    pub retreat: f32,
    pub cover: f32,
    pub flank: f32,
    pub observe: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectorModel {
    pub schema: String,
    pub max_active_melee: usize,
    pub max_active_ranged: usize,
    pub maximum_threat_probes: usize,
    pub maximum_position_probes: usize,
    pub sense_interval: f32,
    pub slot_duration_scale: f32,
    pub slot_separation_radians: f32,
    pub support_radius_scale: f32,
    pub cover_search_radius: f32,
    pub cover_clearance: f32,
    pub flank_angle_radians: f32,
    pub preferred_range_scale: f32,
    pub repath_distance: f32,
    pub switch_cost: f32,
    pub melee_request_interval: f32,
    pub engagement_seconds: f32,
    pub weights: DirectorWeights,
}
impl DirectorModel {
    pub fn validate(&self) -> Result<(), String> {
        let w = &self.weights;
        if self.schema != "newviso.combat-director.v1"
            || self.max_active_melee == 0
            || self.max_active_ranged == 0
            || self.max_active_melee > 16
            || self.max_active_ranged > 32
            || !(1..=32).contains(&self.maximum_threat_probes)
            || !(1..=128).contains(&self.maximum_position_probes)
            || ![
                self.sense_interval,
                self.slot_duration_scale,
                self.slot_separation_radians,
                self.support_radius_scale,
                self.cover_search_radius,
                self.cover_clearance,
                self.flank_angle_radians,
                self.preferred_range_scale,
                self.repath_distance,
                self.melee_request_interval,
                self.engagement_seconds,
            ]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
            || !self.switch_cost.is_finite()
            || self.switch_cost < 0.0
            || self.slot_separation_radians > std::f32::consts::PI
            || ![
                w.distance,
                w.readiness,
                w.vitality,
                w.stamina,
                w.waiting,
                w.threat,
                w.counter,
                w.guard,
                w.retreat,
                w.cover,
                w.flank,
                w.observe,
            ]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0)
        {
            return Err("invalid Combat Director model".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct TacticalState {
    pub role: String,
    pub action: String,
    pub target_actor: String,
    pub goal: Option<[f32; 3]>,
    pub utility: f32,
    pub attack_allowed: bool,
    pub waiting_seconds: f32,
    pub permit_until: f64,
    pub revisions: u64,
    #[serde(skip)]
    last_observed: f64,
    #[serde(skip)]
    last_service: [u64; 2],
    #[serde(skip)]
    next_decision: f64,
    #[serde(skip)]
    task_until: f64,
}
#[derive(Clone, Debug, Default)]
pub struct TacticalBodyObservation {
    pub capacity: f32,
    pub available: bool,
    pub pressure: f32,
}
#[derive(Clone, Debug)]
pub struct TacticalPosition {
    pub position: [f32; 3],
    pub kind: String,
    pub quality: f32,
    pub travel_cost: f32,
}
#[derive(Clone, Debug)]
struct AttackLease {
    target: String,
    group: String,
    ranged: bool,
    until: f64,
}
#[derive(Clone, Debug, Default)]
pub(super) struct DirectorRuntime {
    leases: BTreeMap<String, AttackLease>,
}
#[derive(Clone)]
struct Participant {
    id: String,
    target: String,
    group: String,
    position: [f32; 3],
    target_position: [f32; 3],
    ranged: bool,
    eligible: bool,
    action_running: bool,
    score: f32,
    duration: f32,
    model: DirectorModel,
}
impl DirectorRuntime {
    fn allocate(&mut self, now: f64, actors: &[Participant]) -> BTreeMap<String, (String, f64)> {
        self.leases.retain(|id, lease| {
            actors.iter().any(|a| {
                a.id == *id
                    && a.eligible
                    && a.target == lease.target
                    && a.group == lease.group
                    && a.ranged == lease.ranged
                    && (now < lease.until || a.action_running)
            })
        });
        let mut groups: BTreeMap<(String, String, bool), Vec<&Participant>> = BTreeMap::new();
        for a in actors.iter().filter(|a| a.eligible) {
            groups
                .entry((a.target.clone(), a.group.clone(), a.ranged))
                .or_default()
                .push(a);
        }
        for ((target, group, ranged), mut candidates) in groups {
            let capacity = candidates
                .iter()
                .map(|a| {
                    if ranged {
                        a.model.max_active_ranged
                    } else {
                        a.model.max_active_melee
                    }
                })
                .min()
                .unwrap_or(0);
            let mut admitted: Vec<_> = candidates
                .iter()
                .filter(|a| self.leases.contains_key(&a.id))
                .copied()
                .collect();
            candidates.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)));
            for a in candidates {
                if admitted.len() >= capacity {
                    break;
                }
                if self.leases.contains_key(&a.id) {
                    continue;
                }
                let bearing = (a.position[0] - a.target_position[0])
                    .atan2(a.position[2] - a.target_position[2]);
                let separated = admitted.iter().all(|b| {
                    let other = (b.position[0] - b.target_position[0])
                        .atan2(b.position[2] - b.target_position[2]);
                    let angle = (bearing - other).sin().atan2((bearing - other).cos()).abs();
                    angle
                        >= a.model
                            .slot_separation_radians
                            .min(b.model.slot_separation_radians)
                });
                if !separated {
                    continue;
                }
                self.leases.insert(
                    a.id.clone(),
                    AttackLease {
                        target: target.clone(),
                        group: group.clone(),
                        ranged,
                        until: now + f64::from(a.duration * a.model.slot_duration_scale),
                    },
                );
                admitted.push(a);
            }
        }
        actors
            .iter()
            .map(|a| {
                (
                    a.id.clone(),
                    self.leases
                        .get(&a.id)
                        .map_or(("support".into(), 0.0), |l| ("active".into(), l.until)),
                )
            })
            .collect()
    }
}
impl PedRuntime {
    /// Candidate probes use authored hostility and actual residency. Unseen actors never become targets.
    pub fn tactical_probes(&self, snapshot: &AgentWorldSnapshot) -> Vec<(String, String, f32)> {
        let due: std::collections::BTreeSet<_> = self
            .combat_probes(snapshot.world_seconds)
            .into_iter()
            .map(|p| p.0)
            .collect();
        let mut result = Vec::new();
        for p in self
            .records
            .values()
            .filter(|p| !p.dead && due.contains(&p.profile.actor_id))
        {
            let Some(c) = &p.profile.combat else {
                continue;
            };
            let Some(actor) = snapshot
                .actors
                .iter()
                .find(|a| a.id == p.profile.actor_id && a.enabled && a.simulation_tier == "full")
            else {
                continue;
            };
            let mut candidates: Vec<_> = snapshot
                .actors
                .iter()
                .filter(|a| a.enabled && a.simulation_tier == "full" && a.id != actor.id)
                .filter(|a| {
                    a.id == c.target_actor
                        || self.records.get(&a.id).is_some_and(|other| {
                            matches!(
                                self.relationships
                                    .get(&(p.profile.group.clone(), other.profile.group.clone())),
                                Some(PedRelationship::Hate | PedRelationship::Wanted)
                            )
                        })
                })
                .filter(|a| distance_between(a.position, actor.position) <= c.sight_range)
                .collect();
            candidates.sort_by(|a, b| {
                distance_between(a.position, actor.position)
                    .total_cmp(&distance_between(b.position, actor.position))
                    .then(a.id.cmp(&b.id))
            });
            let limit = c.director.as_ref().map_or(1, |m| m.maximum_threat_probes);
            for target in candidates.into_iter().take(limit) {
                result.push((actor.id.clone(), target.id.clone(), c.sight_range));
            }
        }
        result
    }
    pub fn select_combat_targets(
        &mut self,
        snapshot: &AgentWorldSnapshot,
        visibility: &BTreeMap<(String, String), bool>,
    ) {
        let target_meta: BTreeMap<_, _> = self
            .records
            .values()
            .map(|p| {
                (
                    p.profile.actor_id.clone(),
                    (p.profile.group.clone(), p.dead),
                )
            })
            .collect();
        let relationships = &self.relationships;
        for p in self.records.values_mut().filter(|p| !p.dead) {
            let Some((profile, state)) = p.profile.combat.as_ref().zip(p.combat.as_mut()) else {
                continue;
            };
            let Some(model) = &profile.director else {
                continue;
            };
            let Some(actor) = snapshot
                .actors
                .iter()
                .find(|a| a.id == p.profile.actor_id && a.enabled)
            else {
                continue;
            };
            let mut candidates: Vec<_> = visibility
                .iter()
                .filter(|((a, _), visible)| a == &actor.id && **visible)
                .filter(|((_, id), _)| {
                    target_meta.get(id).is_none_or(|(_, dead)| !*dead)
                        && (id == &profile.target_actor
                            || target_meta.get(id).is_some_and(|(group, dead)| {
                                !dead
                                    && matches!(
                                        relationships
                                            .get(&(p.profile.group.clone(), group.clone())),
                                        Some(PedRelationship::Hate | PedRelationship::Wanted)
                                    )
                            }))
                })
                .filter_map(|((_, id), _)| {
                    snapshot.actors.iter().find(|a| &a.id == id && a.enabled)
                })
                .map(|target| {
                    let distance = distance_between(actor.position, target.position);
                    let injury_threat =
                        if p.last_damage_source.as_deref() == Some(target.id.as_str()) {
                            model.weights.threat
                        } else {
                            0.0
                        };
                    let retained = if target.id == state.target_actor {
                        model.switch_cost
                    } else {
                        0.0
                    };
                    (
                        model.weights.distance / (1.0 + distance / profile.sight_range)
                            + injury_threat
                            + retained,
                        target,
                    )
                })
                .collect();
            candidates.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.id.cmp(&b.1.id)));
            if let Some((_, target)) = candidates.first() {
                if state.target_actor != target.id {
                    state.target_actor = target.id.clone();
                    state.last_seen_position = None;
                    state.ready_at = snapshot.world_seconds + f64::from(profile.reaction_seconds);
                    state.mode = "search".into();
                    state.visible = false;
                    state.tactics = TacticalState::default();
                }
            }
        }
    }
    pub fn set_combat_melee_weapon(&mut self, id: &str, melee: bool, reach: Option<f32>) {
        if let Some(s) = self.records.get_mut(id).and_then(|p| p.combat.as_mut()) {
            s.melee_weapon = melee;
            s.melee_reach = reach.filter(|v| v.is_finite() && *v > 0.0);
        }
    }
}

#[cfg(test)]
mod tests;

mod planning;
