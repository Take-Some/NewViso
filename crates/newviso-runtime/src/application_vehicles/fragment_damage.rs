use super::*;
use newviso_model::ModelFragmentPartPhysics;

#[derive(Clone, Debug)]
pub(super) struct FragmentGroupState {
    pub definition: ModelFragmentPartPhysics,
    health: f32,
    weapon_health: f32,
    last_breaking_impulse: f32,
}
impl FragmentGroupState {
    pub(super) fn new(p: &ModelFragmentPartPhysics) -> Self {
        Self {
            definition: p.clone(),
            health: p.damage_health,
            weapon_health: p.weapon_health,
            last_breaking_impulse: 0.0,
        }
    }
    pub(super) fn repair(&mut self) {
        self.health = self.definition.damage_health;
        self.weapon_health = self.definition.weapon_health;
        self.last_breaking_impulse = 0.0;
    }
    pub(super) fn snapshot(&self) -> Value {
        json!({"group":self.definition.group,
        "parent_group":self.definition.parent_group,"health":self.health,
        "weapon_health":self.weapon_health,"strength":self.definition.strength,
        "last_breaking_impulse":self.last_breaking_impulse,
        "components":self.definition.children.iter().map(|c|c.component).collect::<Vec<_>>()})
    }
}

/// An impulse follows the tree once. Transmission belongs to the child link:
/// up uses the source child; down uses the receiving child, including siblings.
fn propagated(
    groups: &BTreeMap<u16, FragmentGroupState>,
    root: u16,
    magnitude: f32,
) -> BTreeMap<u16, f32> {
    let mut out = BTreeMap::new();
    let mut pending = vec![(root, magnitude)];
    while let Some((id, mag)) = pending.pop() {
        if out.contains_key(&id) || mag <= 0.0 {
            continue;
        }
        let Some(g) = groups.get(&id) else { continue };
        out.insert(id, mag);
        if let Some(parent) = g.definition.parent_group {
            pending.push((parent, mag * g.definition.transmission_up));
        }
        for (&child, c) in groups {
            if c.definition.parent_group == Some(id) {
                pending.push((child, mag * c.definition.transmission_down));
            }
        }
    }
    out
}
pub(super) fn automatically_breakable(part: &VehiclePresentationPart) -> bool {
    part.physics
        .as_ref()
        .is_none_or(|p| p.strength >= 0.0 && p.parent_group.is_some())
}

impl EngineApplication {
    pub(super) fn apply_vehicle_fragment_damage(
        &mut self,
        entity: u64,
        part_index: u32,
        kind: VehicleDamageType,
        damage: f32,
        impulse: f32,
    ) -> bool {
        if damage <= 0.0 {
            return false;
        }
        let Some(binding) = self.vehicle_presentations.get_mut(&entity) else {
            return false;
        };
        let Some(root) = binding
            .parts
            .iter()
            .find(|p| p.index == part_index)
            .and_then(|p| p.physics.as_ref())
            .map(|p| p.group)
        else {
            return false;
        };
        // Detached subtrees stop transmitting force into the original car.
        let active_groups = binding
            .fragment_groups
            .iter()
            .filter(|(id, _)| {
                binding.parts.iter().any(|p| {
                    p.physics.as_ref().is_some_and(|q| q.group == **id)
                        && p.detached_entity.is_none()
                })
            })
            .map(|(id, g)| (*id, g.clone()))
            .collect::<BTreeMap<_, _>>();
        let forces = propagated(&active_groups, root, impulse.max(0.0));
        let damage_by_group = propagated(&active_groups, root, damage);
        for (&id, amount) in &damage_by_group {
            let Some(g) = binding.fragment_groups.get_mut(&id) else {
                continue;
            };
            let scale = match kind {
                VehicleDamageType::Bullet => g.definition.weapon_scale,
                VehicleDamageType::Collision => g.definition.vehicle_scale,
                VehicleDamageType::Explosive => g.definition.explosion_scale,
                _ => g.definition.object_scale,
            };
            let amount = amount * scale;
            if kind == VehicleDamageType::Bullet {
                if g.definition.weapon_health > 0.0 {
                    g.weapon_health = (g.weapon_health - amount).max(0.0);
                }
            } else if kind != VehicleDamageType::Collision || amount > g.definition.min_damage_force
            {
                g.health = (g.health - amount).max(0.0);
            }
            g.last_breaking_impulse = forces.get(&id).copied().unwrap_or(0.0) * scale;
        }
        let mut selected = None::<(usize, f32)>;
        for (index, p) in binding.parts.iter().enumerate() {
            if p.detached_entity.is_some() || !p.visible || !vehicle_part_detachable(p.role) {
                continue;
            }
            let Some(q) = p.physics.as_ref() else {
                continue;
            };
            let Some(g) = binding.fragment_groups.get(&q.group) else {
                continue;
            };
            if !automatically_breakable(p) || !forces.contains_key(&q.group) {
                continue;
            }
            if matches!(
                p.role,
                ModelFragmentPartRole::Door
                    | ModelFragmentPartRole::Bonnet
                    | ModelFragmentPartRole::Boot
            ) && p.door_motion.is_none_or(|m| m.latched || m.driven)
            {
                continue;
            }
            // Wheel damage has its own mission/model proofs and suspension
            // thresholds. Its committed missing state triggers assembly release.
            if p.role == ModelFragmentPartRole::Wheel {
                continue;
            }
            if !matches!(
                kind,
                VehicleDamageType::Collision | VehicleDamageType::Explosive
            ) {
                continue;
            }
            let ratio = g.last_breaking_impulse / q.strength.max(1.0e-6);
            if ratio >= 1.0
                && (p.loose || g.health <= 0.0 || ratio >= 4.0)
                && selected.is_none_or(|(_, best)| ratio > best)
            {
                selected = Some((index, ratio));
            }
        }
        if let Some((index, _)) = selected {
            let p = &mut binding.parts[index];
            p.damage = 1.0;
            p.loose = false;
            p.presentation_override = true;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn group(id: u16, parent: Option<u16>, up: f32, down: f32) -> FragmentGroupState {
        FragmentGroupState::new(&ModelFragmentPartPhysics {
            group: id,
            parent_group: parent,
            strength: 100.0,
            latch_strength: 0.0,
            transmission_up: up,
            transmission_down: down,
            joint_stiffness: 0.0,
            soft_angle_limits: [-1.0, 1.0, 1.0, 1.0],
            flags: 0,
            min_damage_force: 100.0,
            damage_health: 1000.0,
            weapon_health: 50.0,
            weapon_scale: 1.0,
            vehicle_scale: 1.0,
            explosion_scale: 1.0,
            object_scale: 1.0,
            children: vec![],
        })
    }
    #[test]
    fn impulse_propagates_up_then_to_siblings_without_feedback() {
        let groups = BTreeMap::from([
            (0, group(0, None, 0.0, 1.0)),
            (1, group(1, Some(0), 0.25, 0.5)),
            (2, group(2, Some(0), 0.25, 0.5)),
            (3, group(3, Some(1), 0.25, 0.5)),
        ]);
        let values = propagated(&groups, 1, 100.0);
        assert_eq!(values[&1], 100.0);
        assert_eq!(values[&0], 25.0);
        assert_eq!(values[&2], 12.5);
        assert_eq!(values[&3], 50.0);
    }
    #[test]
    fn detached_or_invalid_parent_path_does_not_transmit() {
        let groups = BTreeMap::from([
            (1, group(1, Some(0), 0.25, 0.5)),
            (3, group(3, Some(1), 0.25, 0.5)),
        ]);
        assert_eq!(
            propagated(&groups, 3, 100.0),
            BTreeMap::from([(1, 25.0), (3, 100.0)])
        );
        let mut state = group(1, None, 0.25, 0.5);
        state.health = 1.0;
        state.weapon_health = 0.0;
        state.repair();
        assert_eq!(state.health, 1000.0);
        assert_eq!(state.weapon_health, 50.0);
    }
}
