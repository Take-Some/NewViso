use super::*;

impl EngineApplication {
    pub(super) fn apply_items_command(
        &mut self,
        command: &Value,
        index: usize,
        op: &str,
    ) -> Result<(), String> {
        match op {
            "items.definition.upsert" => {
                let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                    format!("script command[{index}] items.definition.upsert requires string 'id'")
                })?;
                let display_name = command
                    .get("display_name")
                    .and_then(Value::as_str)
                    .unwrap_or(id)
                    .to_owned();
                let category = command
                    .get("category")
                    .and_then(Value::as_str)
                    .unwrap_or("misc")
                    .to_owned();
                let max_stack = command
                    .get("max_stack")
                    .map(|_| command_u32(command, "max_stack", index))
                    .transpose()?
                    .unwrap_or(1);
                let tags = command
                .get("tags")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .map(|value| {
                            value.as_str().map(str::to_owned).ok_or_else(|| {
                                format!(
                                    "script command[{index}] items.definition.upsert tags must contain strings"
                                )
                            })
                        })
                        .collect::<Result<Vec<_>, String>>()
                })
                .transpose()?
                .unwrap_or_default();
                self.items.upsert_definition(ItemDefinition {
                    id: id.to_owned(),
                    display_name,
                    category,
                    max_stack,
                    tags,
                    metadata: command.get("metadata").cloned().unwrap_or(Value::Null),
                })?;
            }
            "items.inventory.ensure" => {
                let owner_id =
                    command
                        .get("owner_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                        "script command[{index}] items.inventory.ensure requires string 'owner_id'"
                    )
                        })?;
                self.items.ensure_inventory(owner_id)?;
            }
            "items.inventory.configure" => {
                let owner = command
                    .get("owner_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!("script command[{index}] inventory requires owner_id")
                    })?;
                self.items
                    .configure_inventory(owner, command_u32(command, "capacity", index)?)?;
            }
            "items.inventory.equip" | "items.inventory.unequip" => {
                let owner = command
                    .get("owner_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!("script command[{index}] equipment requires owner_id")
                    })?;
                let slot = command
                    .get("slot")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("script command[{index}] equipment requires slot"))?;
                if op == "items.inventory.equip" {
                    let item = command
                        .get("item_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!("script command[{index}] equipment requires item_id")
                        })?;
                    self.items.equip(owner, slot, item)?;
                } else {
                    self.items.unequip(owner, slot)?;
                }
                host::publish_event_json(
                    "gameplay.inventory.equipment_changed",
                    "newviso.items",
                    json!({"owner_id": owner, "slot": slot, "item_id": self.items.equipped_item(owner, slot)}),
                )?;
            }
            "items.inventory.add" => {
                let owner_id =
                    command
                        .get("owner_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                        "script command[{index}] items.inventory.add requires string 'owner_id'"
                    )
                        })?;
                let item_id = command
                    .get("item_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!(
                            "script command[{index}] items.inventory.add requires string 'item_id'"
                        )
                    })?;
                let quantity = command_u32(command, "quantity", index)?;
                let accepted = self.items.add_inventory(owner_id, item_id, quantity)?;
                host::publish_event_json(
                    "gameplay.inventory.changed",
                    "newviso.items",
                    json!({
                        "operation": "add",
                        "owner_id": owner_id,
                        "item_id": item_id,
                        "requested_quantity": quantity,
                        "applied_quantity": accepted,
                        "quantity": self.items.inventory_quantity(owner_id, item_id)
                    }),
                )?;
            }
            "items.inventory.remove" => {
                let owner_id =
                    command
                        .get("owner_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                        "script command[{index}] items.inventory.remove requires string 'owner_id'"
                    )
                        })?;
                let item_id = command
                    .get("item_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!(
                        "script command[{index}] items.inventory.remove requires string 'item_id'"
                    )
                    })?;
                let quantity = command_u32(command, "quantity", index)?;
                let removed = self.items.remove_inventory(owner_id, item_id, quantity)?;
                host::publish_event_json(
                    "gameplay.inventory.changed",
                    "newviso.items",
                    json!({
                        "operation": "remove",
                        "owner_id": owner_id,
                        "item_id": item_id,
                        "requested_quantity": quantity,
                        "applied_quantity": removed,
                        "quantity": self.items.inventory_quantity(owner_id, item_id)
                    }),
                )?;
            }
            "items.pickup.upsert" => {
                let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                    format!("script command[{index}] items.pickup.upsert requires string 'id'")
                })?;
                let item_id = command
                    .get("item_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!(
                            "script command[{index}] items.pickup.upsert requires string 'item_id'"
                        )
                    })?;
                let quantity = command
                    .get("quantity")
                    .map(|_| command_u32(command, "quantity", index))
                    .transpose()?
                    .unwrap_or(1);
                let collection_radius = command
                    .get("collection_radius")
                    .map(|_| command_number(command, "collection_radius", index))
                    .transpose()?
                    .unwrap_or(2.0);
                let requires_interact = command
                    .get("requires_interact")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                self.items.upsert_pickup(WorldPickup {
                    id: id.to_owned(),
                    item_id: item_id.to_owned(),
                    quantity,
                    position: command_vec3(command, "position", index)?,
                    collection_radius,
                    requires_interact,
                    collected: false,
                })?;
            }
            "items.pickup.bind_body" => {
                let id = command
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("script command[{index}] pickup binding requires id"))?;
                let visual = command
                    .get("visual_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!("script command[{index}] pickup binding requires visual_id")
                    })?;
                let entity = self
                    .scene
                    .runtime_entity_stable_id(visual)
                    .ok_or_else(|| format!("pickup visual '{visual}' does not exist"))?;
                self.items.bind_pickup_body(id, entity)?;
            }
            "items.inventory.drop" => {
                let text = |field: &str| {
                    command
                        .get(field)
                        .and_then(Value::as_str)
                        .ok_or_else(|| format!("script command[{index}] drop requires {field}"))
                };
                let owner = text("owner_id")?;
                let item = text("item_id")?;
                self.items.drop_inventory(
                    owner,
                    item,
                    command_u32(command, "quantity", index)?,
                    text("id")?,
                    command_vec3(command, "position", index)?,
                    command
                        .get("collection_radius")
                        .map(|_| command_number(command, "collection_radius", index))
                        .transpose()?
                        .unwrap_or(1.65),
                )?;
                host::publish_event_json(
                    "gameplay.inventory.changed",
                    "newviso.items",
                    json!({"operation":"drop","owner_id":owner,"item_id":item,
                    "quantity":self.items.inventory_quantity(owner,item)}),
                )?;
                host::publish_event_json(
                    "gameplay.inventory.equipment_changed",
                    "newviso.items",
                    json!({"owner_id":owner,"slot":"hands","item_id":self.items.equipped_item(owner,"hands")}),
                )?;
            }
            "items.pickup.remove" => {
                let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                    format!("script command[{index}] items.pickup.remove requires string 'id'")
                })?;
                self.items.remove_pickup(id);
            }
            "items.pickup.collect" => {
                let id = command.get("id").and_then(Value::as_str).ok_or_else(|| {
                    format!("script command[{index}] items.pickup.collect requires string 'id'")
                })?;
                let owner_id =
                    command
                        .get("owner_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            format!(
                        "script command[{index}] items.pickup.collect requires string 'owner_id'"
                    )
                        })?;
                let outcome =
                    self.items
                        .collect(id, owner_id, command_vec3(command, "position", index)?)?;
                if outcome.remaining_quantity == 0 && outcome.accepted_quantity > 0 {
                    if let Some(entity) = self.items.pickup_body(id) {
                        if let Some(physics) = self.physics.as_mut() {
                            physics.destroy_body_from_script(&json!({"entity":entity}), index)?;
                        }
                        self.scene.set_physics_process_active(entity, false)?;
                        self.items.unbind_pickup_body(id);
                    }
                }
                if outcome.accepted_quantity > 0 {
                    if let (Some(slot), Some(item)) = (
                        command.get("equip_slot").and_then(Value::as_str),
                        outcome.item_id.as_deref(),
                    ) {
                        self.items.equip(owner_id, slot, item)?;
                        host::publish_event_json(
                            "gameplay.inventory.equipment_changed",
                            "newviso.items",
                            json!({"owner_id": owner_id, "slot": slot, "item_id": item}),
                        )?;
                    }
                    host::publish_event_json(
                        "gameplay.inventory.changed",
                        "newviso.items",
                        json!({"operation":"collect", "owner_id":owner_id, "item_id":outcome.item_id,
                        "applied_quantity":outcome.accepted_quantity,"quantity":outcome.inventory_quantity}),
                    )?;
                }
                host::publish_event_json(
                    "gameplay.pickup.collection",
                    "newviso.items",
                    serde_json::to_value(&outcome).map_err(|error| error.to_string())?,
                )?;
            }
            _ => return Err(unsupported_command(op, index)),
        }
        Ok(())
    }
}
