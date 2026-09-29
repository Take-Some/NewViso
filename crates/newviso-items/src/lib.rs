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

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ItemsCheckpoint {
    schema: String,
    revision: u64,
    definitions: Vec<ItemDefinition>,
    inventories: BTreeMap<String, BTreeMap<String, u32>>,
    pickups: Vec<WorldPickup>,
}

#[derive(Clone, Debug, Default)]
pub struct ItemsRuntime {
    definitions: BTreeMap<String, ItemDefinition>,
    inventories: BTreeMap<String, BTreeMap<String, u32>>,
    pickups: BTreeMap<String, WorldPickup>,
    revision: u64,
    last_collection: Option<CollectionOutcome>,
}

impl ItemsRuntime {
    pub fn upsert_definition(&mut self, definition: ItemDefinition) -> Result<(), String> {
        definition.validate()?;
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
        let accepted = quantity.min(definition.max_stack.saturating_sub(current));
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

    pub fn remove_pickup(&mut self, pickup_id: &str) -> bool {
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
        let capacity = definition.max_stack.saturating_sub(current);
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
            "pickups": self.pickups.values().collect::<Vec<_>>(),
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
}
