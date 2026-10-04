use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const CHECKPOINT_SCHEMA: &str = "newviso.items.checkpoint.v1";
pub const RUNTIME_SCHEMA: &str = "newviso.items.runtime.v1";

fn default_max_stack() -> u32 {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemDefinition {
    pub id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub category: String,
    #[serde(default = "default_max_stack")]
    pub max_stack: u32,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Data-driven gameplay/presentation descriptor. The item domain stores it
    /// verbatim; weapon/equipment/quest systems interpret their own namespaces.
    #[serde(default)]
    pub metadata: Value,
}

impl ItemDefinition {
    fn validate(&self) -> Result<(), String> {
        validate_id("item", &self.id)?;
        if self.max_stack == 0 {
            return Err(format!(
                "item '{}' max_stack must be greater than zero",
                self.id
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WorldPickup {
    pub id: String,
    pub item_id: String,
    pub quantity: u32,
    pub position: [f32; 3],
    pub collection_radius: f32,
    #[serde(default = "default_requires_interact")]
    pub requires_interact: bool,
    #[serde(default)]
    pub collected: bool,
}

fn default_requires_interact() -> bool {
    true
}

impl WorldPickup {
    fn validate(&self) -> Result<(), String> {
        validate_id("pickup", &self.id)?;
        validate_id("pickup item", &self.item_id)?;
        if self.quantity == 0 && !self.collected {
            return Err(format!(
                "pickup '{}' has zero quantity but is not collected",
                self.id
            ));
        }
        if self.position.iter().any(|value| !value.is_finite()) {
            return Err(format!("pickup '{}' position must be finite", self.id));
        }
        if !self.collection_radius.is_finite() || self.collection_radius <= 0.0 {
            return Err(format!(
                "pickup '{}' collection_radius must be finite and greater than zero",
                self.id
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CollectionStatus {
    Collected,
    Partial,
    NoCapacity,
    MissingPickup,
    AlreadyCollected,
    TooFar,
    MissingDefinition,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CollectionOutcome {
    pub status: CollectionStatus,
    pub pickup_id: String,
    pub owner_id: String,
    pub item_id: Option<String>,
    pub accepted_quantity: u32,
    pub remaining_quantity: u32,
    pub inventory_quantity: u32,
    pub distance: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PickupBodyPose {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub linear_velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
}
impl PickupBodyPose {
    fn validate(&self) -> Result<(), String> {
        if self
            .position
            .iter()
            .chain(&self.rotation)
            .chain(&self.linear_velocity)
            .chain(&self.angular_velocity)
            .any(|v| !v.is_finite())
        {
            return Err("pickup body pose must be finite".into());
        }
        let norm: f32 = self.rotation.iter().map(|v| v * v).sum();
        if (norm - 1.0).abs() > 0.01 {
            return Err("pickup body rotation must be normalized".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ItemsCheckpoint {
    schema: String,
    revision: u64,
    definitions: Vec<ItemDefinition>,
    inventories: BTreeMap<String, BTreeMap<String, u32>>,
    pickups: Vec<WorldPickup>,
    #[serde(default)]
    capacities: BTreeMap<String, u32>,
    #[serde(default)]
    body_poses: BTreeMap<String, PickupBodyPose>,
    #[serde(default)]
    equipment: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Default)]
pub struct ItemsRuntime {
    definitions: BTreeMap<String, ItemDefinition>,
    inventories: BTreeMap<String, BTreeMap<String, u32>>,
    pickups: BTreeMap<String, WorldPickup>,
    capacities: BTreeMap<String, u32>,
    body_poses: BTreeMap<String, PickupBodyPose>,
    body_bindings: BTreeMap<String, u64>,
    equipment: BTreeMap<String, BTreeMap<String, String>>,
    revision: u64,
    last_collection: Option<CollectionOutcome>,
}

impl ItemsRuntime {
    pub fn upsert_definition(&mut self, definition: ItemDefinition) -> Result<(), String> {
        definition.validate()?;
        if self.inventories.values().any(|inventory| {
            inventory.get(&definition.id).copied().unwrap_or(0) > definition.max_stack
        }) {
            return Err(format!(
                "item '{}' max_stack would invalidate an existing inventory",
                definition.id
            ));
        }
        self.definitions.insert(definition.id.clone(), definition);
        self.bump_revision();
        Ok(())
    }

    pub fn ensure_inventory(&mut self, owner_id: &str) -> Result<(), String> {
        validate_id("inventory owner", owner_id)?;
        if !self.inventories.contains_key(owner_id) {
            self.inventories
                .insert(owner_id.to_owned(), BTreeMap::new());
            self.bump_revision();
        }
        Ok(())
    }

    /// Capacity counts occupied item slots; max_stack remains the per-item limit.
    pub fn configure_inventory(&mut self, owner_id: &str, capacity: u32) -> Result<(), String> {
        validate_id("inventory owner", owner_id)?;
        if capacity == 0 || self.used_slots(owner_id) > capacity {
            return Err("inventory capacity must be positive and contain its current items".into());
        }
        self.ensure_inventory(owner_id)?;
        if self.capacities.get(owner_id) != Some(&capacity) {
            self.capacities.insert(owner_id.into(), capacity);
            self.bump_revision();
        }
        Ok(())
    }

    pub fn used_slots(&self, owner_id: &str) -> u32 {
        self.inventories
            .get(owner_id)
            .map(|entries| entries.values().filter(|quantity| **quantity > 0).count() as u32)
            .unwrap_or(0)
    }

    fn available_quantity(&self, owner_id: &str, item_id: &str, max_stack: u32) -> u32 {
        let current = self.inventory_quantity(owner_id, item_id);
        if current == 0
            && self
                .capacities
                .get(owner_id)
                .is_some_and(|limit| self.used_slots(owner_id) >= *limit)
        {
            return 0;
        }
        max_stack.saturating_sub(current)
    }

    pub fn equipped_item(&self, owner_id: &str, slot: &str) -> Option<&str> {
        self.equipment
            .get(owner_id)
            .and_then(|slots| slots.get(slot))
            .map(String::as_str)
    }

    pub fn equip(&mut self, owner_id: &str, slot: &str, item_id: &str) -> Result<(), String> {
        validate_id("inventory owner", owner_id)?;
        validate_id("equipment slot", slot)?;
        validate_id("item", item_id)?;
        if self.inventory_quantity(owner_id, item_id) == 0 {
            return Err(format!("cannot equip unowned item '{item_id}'"));
        }
        if self.equipped_item(owner_id, slot) != Some(item_id) {
            self.equipment
                .entry(owner_id.into())
                .or_default()
                .insert(slot.into(), item_id.into());
            self.bump_revision();
        }
        Ok(())
    }

    pub fn unequip(&mut self, owner_id: &str, slot: &str) -> Result<bool, String> {
        validate_id("inventory owner", owner_id)?;
        validate_id("equipment slot", slot)?;
        let removed = self
            .equipment
            .get_mut(owner_id)
            .and_then(|slots| slots.remove(slot))
            .is_some();
        if removed {
            self.bump_revision();
        }
        Ok(removed)
    }

    pub fn inventory_quantity(&self, owner_id: &str, item_id: &str) -> u32 {
        self.inventories
            .get(owner_id)
            .and_then(|inventory| inventory.get(item_id))
            .copied()
            .unwrap_or(0)
    }

    pub fn add_inventory(
        &mut self,
        owner_id: &str,
        item_id: &str,
        quantity: u32,
    ) -> Result<u32, String> {
        validate_id("inventory owner", owner_id)?;
        let definition = self
            .definitions
            .get(item_id)
            .ok_or_else(|| format!("unknown item definition '{item_id}'"))?;
        if quantity == 0 {
            return Ok(0);
        }
        let current = self.inventory_quantity(owner_id, item_id);
        let accepted =
            quantity.min(self.available_quantity(owner_id, item_id, definition.max_stack));
        if accepted == 0 {
            return Ok(0);
        }
        let inventory = self.inventories.entry(owner_id.to_owned()).or_default();
        inventory.insert(item_id.to_owned(), current.saturating_add(accepted));
        self.bump_revision();
        Ok(accepted)
    }

    pub fn remove_inventory(
        &mut self,
        owner_id: &str,
        item_id: &str,
        quantity: u32,
    ) -> Result<u32, String> {
        validate_id("inventory owner", owner_id)?;
        validate_id("item", item_id)?;
        if quantity == 0 {
            return Ok(0);
        }
        let Some(inventory) = self.inventories.get_mut(owner_id) else {
            return Ok(0);
        };
        let current = inventory.get(item_id).copied().unwrap_or(0);
        let removed = quantity.min(current);
        if removed == 0 {
            return Ok(0);
        }
        let remaining = current - removed;
        if remaining == 0 {
            inventory.remove(item_id);
            if let Some(slots) = self.equipment.get_mut(owner_id) {
                slots.retain(|_, equipped| equipped != item_id);
            }
        } else {
            inventory.insert(item_id.to_owned(), remaining);
        }
        self.bump_revision();
        Ok(removed)
    }

    /// Register or refresh an authored pickup without resurrecting persisted state.
    ///
    /// When a pickup with the same id and item already exists (for example after
    /// loading a checkpoint), authored transform/radius policy is refreshed while
    /// remaining quantity and collected state are retained.
    pub fn upsert_pickup(&mut self, mut pickup: WorldPickup) -> Result<(), String> {
        pickup.validate()?;
        if !self.definitions.contains_key(&pickup.item_id) {
            return Err(format!(
                "pickup '{}' references unknown item definition '{}'",
                pickup.id, pickup.item_id
            ));
        }

        if let Some(existing) = self.pickups.get(&pickup.id) {
            if existing.item_id == pickup.item_id {
                pickup.quantity = existing.quantity;
                pickup.collected = existing.collected;
            }
        }

        self.pickups.insert(pickup.id.clone(), pickup);
        self.bump_revision();
        Ok(())
    }

    pub fn bind_pickup_body(&mut self, id: &str, entity: u64) -> Result<(), String> {
        if !self.pickups.contains_key(id) || entity == 0 {
            return Err("pickup body binding requires an existing pickup and entity".into());
        }
        self.body_bindings.insert(id.into(), entity);
        Ok(())
    }

    pub fn pickup_body(&self, id: &str) -> Option<u64> {
        self.body_bindings.get(id).copied()
    }
    pub fn pickup_bodies(&self) -> Vec<(String, u64)> {
        self.body_bindings
            .iter()
            .map(|(id, entity)| (id.clone(), *entity))
            .collect()
    }
    pub fn unbind_pickup_body(&mut self, id: &str) {
        self.body_bindings.remove(id);
    }

    pub fn sync_pickup_body(&mut self, id: &str, pose: PickupBodyPose) -> Result<(), String> {
        pose.validate()?;
        let Some(pickup) = self.pickups.get_mut(id) else {
            return Ok(());
        };
        if pickup.collected {
            return Ok(());
        }
        if self.body_poses.get(id) != Some(&pose) {
            pickup.position = pose.position;
            self.body_poses.insert(id.into(), pose);
            self.bump_revision();
        }
        Ok(())
    }

    /// Inverse of collection: inventory removal and world creation form one transaction.
    pub fn drop_inventory(
        &mut self,
        owner: &str,
        item: &str,
        quantity: u32,
        id: &str,
        position: [f32; 3],
        radius: f32,
    ) -> Result<(), String> {
        validate_id("inventory owner", owner)?;
        let pickup = WorldPickup {
            id: id.into(),
            item_id: item.into(),
            quantity,
            position,
            collection_radius: radius,
            requires_interact: true,
            collected: false,
        };
        pickup.validate()?;
        if self.pickups.contains_key(id) {
            return Err("drop pickup id already exists".into());
        }
        if !self.definitions.contains_key(item) || self.inventory_quantity(owner, item) < quantity {
            return Err("cannot drop an unowned item or excessive quantity".into());
        }
        self.remove_inventory(owner, item, quantity)?;
        self.pickups.insert(id.into(), pickup);
        self.bump_revision();
        Ok(())
    }

    pub fn remove_pickup(&mut self, pickup_id: &str) -> bool {
        self.body_bindings.remove(pickup_id);
        self.body_poses.remove(pickup_id);
        let removed = self.pickups.remove(pickup_id).is_some();
        if removed {
            self.bump_revision();
        }
        removed
    }

    pub fn collect(
        &mut self,
        pickup_id: &str,
        owner_id: &str,
        collector_position: [f32; 3],
    ) -> Result<CollectionOutcome, String> {
        validate_id("inventory owner", owner_id)?;
        if collector_position.iter().any(|value| !value.is_finite()) {
            return Err("collector position must be finite".to_owned());
        }

        let Some(snapshot) = self.pickups.get(pickup_id).cloned() else {
            return Ok(self.record_collection(CollectionOutcome {
                status: CollectionStatus::MissingPickup,
                pickup_id: pickup_id.to_owned(),
                owner_id: owner_id.to_owned(),
                item_id: None,
                accepted_quantity: 0,
                remaining_quantity: 0,
                inventory_quantity: 0,
                distance: None,
            }));
        };

        if snapshot.collected || snapshot.quantity == 0 {
            return Ok(self.record_collection(CollectionOutcome {
                status: CollectionStatus::AlreadyCollected,
                pickup_id: pickup_id.to_owned(),
                owner_id: owner_id.to_owned(),
                item_id: Some(snapshot.item_id.clone()),
                accepted_quantity: 0,
                remaining_quantity: 0,
                inventory_quantity: self.inventory_quantity(owner_id, &snapshot.item_id),
                distance: Some(distance(snapshot.position, collector_position)),
            }));
        }

        let distance = distance(snapshot.position, collector_position);
        if distance > snapshot.collection_radius {
            return Ok(self.record_collection(CollectionOutcome {
                status: CollectionStatus::TooFar,
                pickup_id: pickup_id.to_owned(),
                owner_id: owner_id.to_owned(),
                item_id: Some(snapshot.item_id.clone()),
                accepted_quantity: 0,
                remaining_quantity: snapshot.quantity,
                inventory_quantity: self.inventory_quantity(owner_id, &snapshot.item_id),
                distance: Some(distance),
            }));
        }

        let Some(definition) = self.definitions.get(&snapshot.item_id).cloned() else {
            return Ok(self.record_collection(CollectionOutcome {
                status: CollectionStatus::MissingDefinition,
                pickup_id: pickup_id.to_owned(),
                owner_id: owner_id.to_owned(),
                item_id: Some(snapshot.item_id.clone()),
                accepted_quantity: 0,
                remaining_quantity: snapshot.quantity,
                inventory_quantity: self.inventory_quantity(owner_id, &snapshot.item_id),
                distance: Some(distance),
            }));
        };

        let current = self.inventory_quantity(owner_id, &snapshot.item_id);
        let capacity = self.available_quantity(owner_id, &snapshot.item_id, definition.max_stack);
        let accepted = snapshot.quantity.min(capacity);
        if accepted == 0 {
            return Ok(self.record_collection(CollectionOutcome {
                status: CollectionStatus::NoCapacity,
                pickup_id: pickup_id.to_owned(),
                owner_id: owner_id.to_owned(),
                item_id: Some(snapshot.item_id.clone()),
                accepted_quantity: 0,
                remaining_quantity: snapshot.quantity,
                inventory_quantity: current,
                distance: Some(distance),
            }));
        }

        let new_inventory_quantity = current.saturating_add(accepted);
        self.inventories
            .entry(owner_id.to_owned())
            .or_default()
            .insert(snapshot.item_id.clone(), new_inventory_quantity);

        let remaining = snapshot.quantity - accepted;
        let pickup = self
            .pickups
            .get_mut(pickup_id)
            .ok_or_else(|| "pickup disappeared during collection transaction".to_owned())?;
        pickup.quantity = remaining;
        pickup.collected = remaining == 0;

        self.bump_revision();
        let status = if remaining == 0 {
            CollectionStatus::Collected
        } else {
            CollectionStatus::Partial
        };
        Ok(self.record_collection(CollectionOutcome {
            status,
            pickup_id: pickup_id.to_owned(),
            owner_id: owner_id.to_owned(),
            item_id: Some(snapshot.item_id),
            accepted_quantity: accepted,
            remaining_quantity: remaining,
            inventory_quantity: new_inventory_quantity,
            distance: Some(distance),
        }))
    }

    pub fn runtime_state(&self) -> Value {
        let inventories = self
            .inventories
            .iter()
            .map(|(owner_id, entries)| {
                json!({
                    "owner_id": owner_id,
                    "capacity": self.capacities.get(owner_id),
                    "used_slots": self.used_slots(owner_id),
                    "equipped": self.equipment.get(owner_id).cloned().unwrap_or_default(),
                    "entries": entries
                        .iter()
                        .map(|(item_id, quantity)| json!({
                            "item_id": item_id,
                            "quantity": quantity
                        }))
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        json!({
            "schema": RUNTIME_SCHEMA,
            "revision": self.revision,
            "definitions": self.definitions.values().collect::<Vec<_>>(),
            "inventories": inventories,
            "pickups": self.pickups.values().map(|pickup| {
                let mut value = serde_json::to_value(pickup).expect("valid pickup");
                value["physical_pose"] = serde_json::to_value(self.body_poses.get(&pickup.id)).expect("valid body pose");
                value["body_entity"] = self.body_bindings.get(&pickup.id).map(|entity|Value::String(entity.to_string())).unwrap_or(Value::Null);
                value
            }).collect::<Vec<_>>(),
            "last_collection": self.last_collection
        })
    }

    pub fn checkpoint(&self) -> Result<Value, String> {
        serde_json::to_value(ItemsCheckpoint {
            schema: CHECKPOINT_SCHEMA.to_owned(),
            revision: self.revision,
            definitions: self.definitions.values().cloned().collect(),
            inventories: self.inventories.clone(),
            pickups: self.pickups.values().cloned().collect(),
            capacities: self.capacities.clone(),
            body_poses: self.body_poses.clone(),
            equipment: self.equipment.clone(),
        })
        .map_err(|error| error.to_string())
    }

    pub fn from_checkpoint(value: Value) -> Result<Self, String> {
        if value.is_null() {
            return Ok(Self::default());
        }
        let checkpoint: ItemsCheckpoint =
            serde_json::from_value(value).map_err(|error| error.to_string())?;
        if checkpoint.schema != CHECKPOINT_SCHEMA {
            return Err(format!(
                "unsupported items checkpoint schema '{}'",
                checkpoint.schema
            ));
        }

        let mut runtime = Self::default();
        for definition in checkpoint.definitions {
            definition.validate()?;
            if runtime
                .definitions
                .insert(definition.id.clone(), definition)
                .is_some()
            {
                return Err("items checkpoint contains duplicate definitions".to_owned());
            }
        }
        for (owner, entries) in checkpoint.inventories {
            validate_id("inventory owner", &owner)?;
            for (item_id, quantity) in &entries {
                validate_id("item", item_id)?;
                let definition = runtime.definitions.get(item_id).ok_or_else(|| {
                    format!(
                        "inventory owner '{}' references unknown item '{}'",
                        owner, item_id
                    )
                })?;
                if *quantity > definition.max_stack {
                    return Err(format!(
                        "inventory owner '{}' item '{}' quantity {} exceeds max_stack {}",
                        owner, item_id, quantity, definition.max_stack
                    ));
                }
            }
            runtime.inventories.insert(owner, entries);
        }
        for pickup in checkpoint.pickups {
            pickup.validate()?;
            if !runtime.definitions.contains_key(&pickup.item_id) {
                return Err(format!(
                    "pickup '{}' references unknown item '{}'",
                    pickup.id, pickup.item_id
                ));
            }
            if runtime.pickups.insert(pickup.id.clone(), pickup).is_some() {
                return Err("items checkpoint contains duplicate pickups".to_owned());
            }
        }
        for (id, pose) in checkpoint.body_poses {
            if !runtime.pickups.contains_key(&id) {
                return Err("pickup body pose has no pickup".into());
            }
            runtime.sync_pickup_body(&id, pose)?;
        }
        for (owner, capacity) in checkpoint.capacities {
            runtime.configure_inventory(&owner, capacity)?;
        }
        for (owner, slots) in checkpoint.equipment {
            for (slot, item) in slots {
                runtime.equip(&owner, &slot, &item)?;
            }
        }
        runtime.revision = checkpoint.revision;
        Ok(runtime)
    }

    fn record_collection(&mut self, outcome: CollectionOutcome) -> CollectionOutcome {
        self.last_collection = Some(outcome.clone());
        outcome
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

fn validate_id(label: &str, id: &str) -> Result<(), String> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} id must not be empty"));
    }
    if trimmed != id {
        return Err(format!("{label} id must not have surrounding whitespace"));
    }
    if trimmed.len() > 160 {
        return Err(format!("{label} id exceeds 160 bytes"));
    }
    if trimmed.chars().any(|ch| ch.is_control()) {
        return Err(format!("{label} id contains control characters"));
    }
    Ok(())
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(max_stack: u32) -> ItemDefinition {
        ItemDefinition {
            id: "office.phone".into(),
            display_name: "Office phone".into(),
            category: "misc".into(),
            max_stack,
            tags: vec!["pickable".into()],
            metadata: Value::Null,
        }
    }

    fn pickup(quantity: u32) -> WorldPickup {
        WorldPickup {
            id: "pickup.phone.01".into(),
            item_id: "office.phone".into(),
            quantity,
            position: [1.0, 0.0, 0.0],
            collection_radius: 2.0,
            requires_interact: true,
            collected: false,
        }
    }

    #[test]
    fn collection_is_atomic_and_persists() {
        let mut runtime = ItemsRuntime::default();
        runtime.upsert_definition(definition(4)).unwrap();
        runtime.upsert_pickup(pickup(2)).unwrap();

        let result = runtime
            .collect("pickup.phone.01", "player", [0.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(result.status, CollectionStatus::Collected);
        assert_eq!(result.accepted_quantity, 2);
        assert_eq!(runtime.inventory_quantity("player", "office.phone"), 2);

        let checkpoint = runtime.checkpoint().unwrap();
        let mut restored = ItemsRuntime::from_checkpoint(checkpoint).unwrap();
        restored.upsert_definition(definition(4)).unwrap();
        restored.upsert_pickup(pickup(2)).unwrap();

        let result = restored
            .collect("pickup.phone.01", "player", [0.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(result.status, CollectionStatus::AlreadyCollected);
        assert_eq!(restored.inventory_quantity("player", "office.phone"), 2);
    }

    #[test]
    fn partial_collection_respects_stack_capacity() {
        let mut runtime = ItemsRuntime::default();
        runtime.upsert_definition(definition(3)).unwrap();
        runtime.add_inventory("player", "office.phone", 2).unwrap();
        runtime.upsert_pickup(pickup(2)).unwrap();

        let result = runtime
            .collect("pickup.phone.01", "player", [0.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(result.status, CollectionStatus::Partial);
        assert_eq!(result.accepted_quantity, 1);
        assert_eq!(result.remaining_quantity, 1);
        assert_eq!(runtime.inventory_quantity("player", "office.phone"), 3);
    }

    #[test]
    fn rejected_collection_does_not_mutate_inventory_or_pickup() {
        let mut runtime = ItemsRuntime::default();
        runtime.upsert_definition(definition(1)).unwrap();
        runtime.upsert_pickup(pickup(1)).unwrap();

        let result = runtime
            .collect("pickup.phone.01", "player", [20.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(result.status, CollectionStatus::TooFar);
        assert_eq!(runtime.inventory_quantity("player", "office.phone"), 0);
        assert_eq!(runtime.runtime_state()["pickups"][0]["collected"], false);
    }

    #[test]
    fn capacity_rejection_keeps_pickup_and_equipment_persists() {
        let mut runtime = ItemsRuntime::default();
        runtime.upsert_definition(definition(3)).unwrap();
        let mut second = definition(1);
        second.id = "weapon.test".into();
        runtime.upsert_definition(second).unwrap();
        runtime.configure_inventory("player", 1).unwrap();
        runtime.add_inventory("player", "weapon.test", 1).unwrap();
        assert!(runtime.equip("player", "hands", "office.phone").is_err());
        runtime.equip("player", "hands", "weapon.test").unwrap();
        runtime.upsert_pickup(pickup(2)).unwrap();
        assert_eq!(
            runtime
                .collect("pickup.phone.01", "player", [1., 0., 0.])
                .unwrap()
                .status,
            CollectionStatus::NoCapacity
        );
        assert_eq!(runtime.runtime_state()["pickups"][0]["quantity"], 2);
        let mut restored = ItemsRuntime::from_checkpoint(runtime.checkpoint().unwrap()).unwrap();
        assert_eq!(
            restored.equipped_item("player", "hands"),
            Some("weapon.test")
        );
        restored
            .remove_inventory("player", "weapon.test", 1)
            .unwrap();
        assert_eq!(restored.equipped_item("player", "hands"), None);
        assert_eq!(
            restored
                .collect("pickup.phone.01", "player", [1., 0., 0.])
                .unwrap()
                .accepted_quantity,
            2
        );
    }

    #[test]
    fn legacy_checkpoint_and_invalid_equipment_are_checked() {
        let mut runtime = ItemsRuntime::default();
        runtime.upsert_definition(definition(3)).unwrap();
        runtime.add_inventory("player", "office.phone", 2).unwrap();
        assert!(runtime.configure_inventory("player", 0).is_err());
        assert!(runtime.upsert_definition(definition(1)).is_err());
        let mut checkpoint = runtime.checkpoint().unwrap();
        checkpoint.as_object_mut().unwrap().remove("capacities");
        checkpoint.as_object_mut().unwrap().remove("equipment");
        assert_eq!(
            ItemsRuntime::from_checkpoint(checkpoint.clone())
                .unwrap()
                .inventory_quantity("player", "office.phone"),
            2
        );
        checkpoint["equipment"] = json!({"player":{"hands":"missing.item"}});
        assert!(ItemsRuntime::from_checkpoint(checkpoint).is_err());
    }

    #[test]
    fn moving_pickup_uses_physics_position_and_drop_is_atomic() {
        let mut runtime = ItemsRuntime::default();
        runtime.upsert_definition(definition(3)).unwrap();
        runtime.add_inventory("player", "office.phone", 1).unwrap();
        runtime.equip("player", "hands", "office.phone").unwrap();
        assert!(runtime
            .drop_inventory("player", "office.phone", 2, "drop.1", [0.; 3], 2.)
            .is_err());
        assert_eq!(runtime.inventory_quantity("player", "office.phone"), 1);
        runtime
            .drop_inventory("player", "office.phone", 1, "drop.1", [0.; 3], 2.)
            .unwrap();
        assert_eq!(runtime.equipped_item("player", "hands"), None);
        runtime.bind_pickup_body("drop.1", 900001).unwrap();
        runtime
            .sync_pickup_body(
                "drop.1",
                PickupBodyPose {
                    position: [10., 0., 0.],
                    rotation: [0., 0., 0., 1.],
                    linear_velocity: [0.; 3],
                    angular_velocity: [0.; 3],
                },
            )
            .unwrap();
        assert_eq!(
            runtime.collect("drop.1", "player", [0.; 3]).unwrap().status,
            CollectionStatus::TooFar
        );
        let mut restored = ItemsRuntime::from_checkpoint(runtime.checkpoint().unwrap()).unwrap();
        assert_eq!(restored.pickup_body("drop.1"), None);
        assert_eq!(
            restored
                .collect("drop.1", "player", [10., 0., 0.])
                .unwrap()
                .status,
            CollectionStatus::Collected
        );
    }
}
