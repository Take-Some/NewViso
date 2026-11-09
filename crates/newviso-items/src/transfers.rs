//! Atomic equipped-item release and item state transfer, independent of combat.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PickupOrigin {
    pub owner_id: String,
    pub equipment_generation: u64,
    pub reason: String,
    pub pose_source: String,
    pub initial_pose: PickupBodyPose,
}
pub struct EquipmentDrop<'a> {
    pub owner: &'a str,
    pub slot: &'a str,
    pub item: &'a str,
    pub generation: u64,
    pub pickup_id: &'a str,
    pub radius: f32,
    pub origin: PickupOrigin,
    pub state: Option<Value>,
}
impl ItemsRuntime {
    pub fn definition(&self, id: &str) -> Option<&ItemDefinition> {
        self.definitions.get(id)
    }
    pub fn equipped_generation(&self, owner: &str, slot: &str) -> u64 {
        self.equipment_generations
            .get(owner)
            .and_then(|s| s.get(slot))
            .copied()
            .unwrap_or(0)
    }
    pub fn item_state(&self, owner: &str, item: &str) -> Option<&Value> {
        self.item_states.get(owner).and_then(|s| s.get(item))
    }
    pub fn set_item_state(&mut self, owner: &str, item: &str, state: Value) -> Result<(), String> {
        if self.inventory_quantity(owner, item) != 1
            || self.definitions.get(item).is_none_or(|d| d.max_stack != 1)
        {
            return Err("instance state requires an owned non-stackable item".into());
        }
        if self.item_state(owner, item) != Some(&state) {
            self.item_states
                .entry(owner.into())
                .or_default()
                .insert(item.into(), state);
            self.bump_revision();
        }
        Ok(())
    }
    /// Reject stale actions before mutation. Creation, pose, ownership and payload move together.
    pub fn drop_equipped(&mut self, drop: EquipmentDrop<'_>) -> Result<bool, String> {
        if self.equipped_item(drop.owner, drop.slot) != Some(drop.item)
            || self.equipped_generation(drop.owner, drop.slot) != drop.generation
        {
            return Ok(false);
        }
        drop.origin.initial_pose.validate()?;
        if drop.origin.owner_id != drop.owner || drop.origin.equipment_generation != drop.generation
        {
            return Err("equipment release provenance disagrees with ownership".into());
        }
        self.drop_inventory(
            drop.owner,
            drop.item,
            1,
            drop.pickup_id,
            drop.origin.initial_pose.position,
            drop.radius,
        )?;
        if let Some(state) = drop.state {
            self.pickup_states.insert(drop.pickup_id.into(), state);
        }
        self.body_poses
            .insert(drop.pickup_id.into(), drop.origin.initial_pose.clone());
        self.pickup_origins
            .insert(drop.pickup_id.into(), drop.origin);
        Ok(true)
    }

    pub(super) fn restore_transfer_state(
        &mut self,
        generations: BTreeMap<String, BTreeMap<String, u64>>,
        item_states: BTreeMap<String, BTreeMap<String, Value>>,
        pickup_states: BTreeMap<String, Value>,
        origins: BTreeMap<String, PickupOrigin>,
    ) -> Result<(), String> {
        for (owner, slots) in &generations {
            validate_id("equipment generation owner", owner)?;
            for slot in slots.keys() {
                validate_id("equipment generation slot", slot)?;
            }
        }
        for (owner, entries) in &item_states {
            for item in entries.keys() {
                if self.inventory_quantity(owner, item) != 1
                    || self.definitions.get(item).is_none_or(|d| d.max_stack != 1)
                {
                    return Err("saved instance state has no owned unique item".into());
                }
            }
        }
        if pickup_states
            .keys()
            .chain(origins.keys())
            .any(|id| !self.pickups.contains_key(id))
        {
            return Err("saved pickup state has no pickup".into());
        }
        for origin in origins.values() {
            validate_id("pickup source owner", &origin.owner_id)?;
            origin.initial_pose.validate()?;
        }
        self.equipment_generations = generations;
        self.item_states = item_states;
        self.pickup_states = pickup_states;
        self.pickup_origins = origins;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ready() -> ItemsRuntime {
        let mut r = ItemsRuntime::default();
        r.upsert_definition(ItemDefinition {
            id: "gun".into(),
            display_name: "gun".into(),
            category: "weapon".into(),
            max_stack: 1,
            tags: vec![],
            metadata: Value::Null,
        })
        .unwrap();
        r.add_inventory("npc", "gun", 1).unwrap();
        r.equip("npc", "hands", "gun").unwrap();
        r
    }
    fn pose() -> PickupBodyPose {
        PickupBodyPose {
            position: [1., 2., 3.],
            rotation: [0., 0., 0., 1.],
            linear_velocity: [2., 3., 4.],
            angular_velocity: [0., 1., 0.],
        }
    }
    fn release<'a>(generation: u64, id: &'a str) -> EquipmentDrop<'a> {
        EquipmentDrop {
            owner: "npc",
            slot: "hands",
            item: "gun",
            generation,
            pickup_id: id,
            radius: 2.,
            origin: PickupOrigin {
                owner_id: "npc".into(),
                equipment_generation: generation,
                reason: "support_failure".into(),
                pose_source: "weapon_attachment".into(),
                initial_pose: pose(),
            },
            state: Some(json!({"weapon":{"magazine":2,"reserve":3}})),
        }
    }
    #[test]
    fn release_moves_momentum_and_ammunition_and_rejects_stale_repeats() {
        let mut r = ready();
        let token = r.equipped_generation("npc", "hands");
        assert!(r.drop_equipped(release(token, "drop.1")).unwrap());
        let before = r.checkpoint().unwrap();
        assert!(!r.drop_equipped(release(token, "drop.2")).unwrap());
        assert_eq!(r.checkpoint().unwrap(), before);
        assert_eq!(r.body_poses["drop.1"], pose());
        r.collect("drop.1", "player", [1., 2., 3.]).unwrap();
        assert_eq!(
            r.item_state("player", "gun").unwrap()["weapon"]["magazine"],
            2
        );
        assert!(r.item_state("npc", "gun").is_none());
        let restored = ItemsRuntime::from_checkpoint(r.checkpoint().unwrap()).unwrap();
        assert_eq!(
            restored.item_state("player", "gun"),
            r.item_state("player", "gun")
        );
    }
    #[test]
    fn reacquired_same_weapon_has_a_new_generation_and_may_be_released_again() {
        let mut r = ready();
        let token = r.equipped_generation("npc", "hands");
        r.equip("npc", "hands", "gun").unwrap();
        assert_eq!(r.equipped_generation("npc", "hands"), token);
        r.drop_equipped(release(token, "drop.1")).unwrap();
        r.collect("drop.1", "npc", [1., 2., 3.]).unwrap();
        r.equip("npc", "hands", "gun").unwrap();
        let new = r.equipped_generation("npc", "hands");
        assert_ne!(token, new);
        assert!(!r.drop_equipped(release(token, "stale")).unwrap());
        assert!(r.drop_equipped(release(new, "drop.2")).unwrap());
    }
    #[test]
    fn invalid_release_pose_cannot_remove_equipment() {
        let mut r = ready();
        let before = r.checkpoint().unwrap();
        let mut request = release(r.equipped_generation("npc", "hands"), "drop");
        request.origin.initial_pose.rotation = [0.; 4];
        assert!(r.drop_equipped(request).is_err());
        assert_eq!(r.checkpoint().unwrap(), before);
    }
}
