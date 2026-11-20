//! Provider-neutral view-model adapters for Ink menus backed by NewViso
//! game state. The input is the engine's typed runtime snapshot, never private
//! REDengine controller implementation or forged save/quest content.
//!
//! This intentionally adapts *view data*, not privileged actions. Inventory
//! mutations, save/load, crafting and quest state changes remain owned by
//! engine services and must be explicitly authorized through their APIs.
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MAX_ITEMS: usize = 1024;
const MAX_DEFINITIONS: usize = 8192;

fn unavailable(controller: &str, reason: &str) -> Value {
    json!({"controller_class":controller,"available":false,"reason":reason})
}

fn inventory_view(items: Option<&Value>, owner: &str) -> Value {
    let Some(items) = items else {
        return unavailable("gameuiInventoryGameController","items-runtime-unavailable");
    };
    let Some(inventories) = items.get("inventories").and_then(Value::as_array) else {
        return unavailable("gameuiInventoryGameController","items-inventory-schema-unsupported");
    };
    let owner_inventory = inventories.iter().find(|inv| {
        inv.get("owner_id").and_then(Value::as_str) == Some(owner)
    });
    let Some(inventory) = owner_inventory else {
        // A fresh game with zero player items is a valid, empty inventory.
        // Distinguish it from a missing runtime provider.
        return json!({
            "controller_class":"gameuiInventoryGameController",
            "available":true,"owner_id":owner,"entries":[],"equipped":{},
            "capacity":null,"used_slots":0,"revision":items.get("revision")
        });
    };
    let definitions = items.get("definitions").and_then(Value::as_array);
    let mut lookup = BTreeMap::<String, &Value>::new();
    for definition in definitions.into_iter().flatten().take(MAX_DEFINITIONS) {
        if let Some(id) = definition.get("id").and_then(Value::as_str) {
            lookup.insert(id.to_owned(), definition);
        }
    }
    let mut entries = Vec::new();
    for row in inventory.get("entries").and_then(Value::as_array)
        .into_iter().flatten().take(MAX_ITEMS)
    {
        let Some(id) = row.get("item_id").and_then(Value::as_str) else {continue};
        let Some(quantity) = row.get("quantity").and_then(Value::as_u64) else {continue};
        if quantity==0 {continue}
        let definition = lookup.get(id);
        entries.push(json!({
            "id":id,
            "quantity":quantity,
            "name":definition.and_then(|v|v.get("display_name"))
                .and_then(Value::as_str).unwrap_or(id),
            "category":definition.and_then(|v|v.get("category"))
                .and_then(Value::as_str).unwrap_or("unknown"),
            "tags":definition.and_then(|v|v.get("tags"))
                .cloned().unwrap_or_else(||json!([])),
            "state":row.get("state").cloned().unwrap_or(Value::Null)
        }));
    }
    entries.sort_by(|a,b| {
        let key = |row: &Value| (
            row["category"].as_str().unwrap_or("").to_owned(),
            row["name"].as_str().unwrap_or("").to_owned(),
            row["id"].as_str().unwrap_or("").to_owned()
        );
        key(a).cmp(&key(b))
    });
    json!({
        "controller_class":"gameuiInventoryGameController",
        "available":true,"owner_id":owner,
        "entries":entries,
        "equipped":inventory.get("equipped").cloned().unwrap_or_else(||json!({})),
        "capacity":inventory.get("capacity").cloned().unwrap_or(Value::Null),
        "used_slots":inventory.get("used_slots").cloned().unwrap_or(Value::Null),
        "revision":items.get("revision")
    })
}

/// This is mounted as /ink_ui in the script runtime snapshot. Game scripts can
/// bind these actual values to source-derived widgets. Unsupported domain
/// controllers remain explicitly unavailable rather than displaying fabricated
/// emails, quests, perks or crafting recipes.
pub fn view_models(frame: &Value, player_owner: &str) -> Value {
    view_models_from_sources(frame.get("items"),frame.get("vehicles"),player_owner)
}

/// Borrow directly from an existing live runtime snapshot without cloning
/// potentially enormous scene/world structures into another JSON object.
pub fn view_models_from_sources(
    items: Option<&Value>, vehicles: Option<&Value>, player_owner: &str
) -> Value {
    let inventory = inventory_view(items,player_owner);
    let items_ready = inventory["available"].as_bool()==Some(true);
    let mut backpack=inventory.clone();
    backpack["controller_class"]=json!("gameuiBackpackGameController");
    let vehicle_snapshot=vehicles.cloned();
    json!({
        "schema":"newviso.ink.viewmodel.v1",
        "screens":{
            "inventory_screen":inventory,
            "backpack":backpack,
            "crafting_main":{
                "controller_class":"CraftingMainGameController",
                "available":false,
                "inventory_available":items_ready,
                "reason":"recipe-authority-not-connected"
            },
            "world_map":unavailable(
                "gameuiWorldMapMenuGameController",
                "map-mappin-controller-not-connected"
            ),
            "quest_log_v2":unavailable(
                "questLogV2GameController","quest-journal-not-connected"
            ),
            "phone":unavailable(
                "gameuiPhoneMenuGameController","phone-contacts-not-connected"
            ),
            "scanner":unavailable(
                "gameuiScannerHudGameController","scanner-controller-not-connected"
            ),
            "vehicle_hud":{
                "controller_class":"gameuiVehicleHudController",
                "available":vehicle_snapshot.is_some(),
                "source":"newviso.vehicles.runtime_state",
                "vehicle_snapshot":vehicle_snapshot
            }
        },
        "model_source":"engine.runtime_state"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_item_definitions_join_owner_quantities_and_equipment() {
        let state=json!({"items":{
            "revision":12,
            "definitions":[
                {"id":"weapon.test","display_name":"Unity","category":"weapon","tags":["pistol"]},
                {"id":"office.phone","display_name":"Office phone","category":"misc","tags":[]}
            ],
            "inventories":[{"owner_id":"player","capacity":10,"used_slots":2,
                "equipped":{"hands":"weapon.test"},
                "entries":[
                    {"item_id":"weapon.test","quantity":1,"state":{"durability":0.8}},
                    {"item_id":"office.phone","quantity":2}
                ]}]}
        });
        let output=view_models(&state,"player");
        let inv=&output["screens"]["inventory_screen"];
        assert_eq!(inv["available"],true);
        assert_eq!(inv["entries"][0]["name"],"Office phone");
        assert_eq!(inv["entries"][1]["category"],"weapon");
        assert_eq!(inv["equipped"]["hands"],"weapon.test");
        assert_eq!(inv["revision"],12);
        assert_eq!(output["screens"]["backpack"]["entries"][0]["quantity"],2);
    }
    #[test]
    fn empty_player_inventory_is_valid_but_missing_item_service_is_not() {
        let no_items=view_models(&json!({}),"player");
        assert_eq!(no_items["screens"]["inventory_screen"]["available"],false);
        let empty=view_models(&json!({"items":{"inventories":[]}}),"player");
        assert_eq!(empty["screens"]["inventory_screen"]["available"],true);
        assert_eq!(empty["screens"]["inventory_screen"]["entries"].as_array().unwrap().len(),0);
    }
    #[test]
    fn unsupported_gameplay_controllers_never_forge_quests_or_recipes() {
        let output=view_models(&json!({"items":{"inventories":[]}}),"player");
        assert_eq!(output["screens"]["crafting_main"]["available"],false);
        assert_eq!(output["screens"]["quest_log_v2"]["available"],false);
        assert_eq!(output["screens"]["phone"]["available"],false);
        assert_eq!(output["screens"]["world_map"]["available"],false);
    }
    #[test]
    fn inventory_adapter_never_exceeds_bounded_item_limit() {
        let rows=(0..2000).map(|i|json!({"item_id":format!("item.{i}"),"quantity":1}))
            .collect::<Vec<_>>();
        let state=json!({"items":{"inventories":[{"owner_id":"player","entries":rows}]}});
        let view=view_models(&state,"player");
        assert_eq!(view["screens"]["inventory_screen"]["entries"]
            .as_array().unwrap().len(),MAX_ITEMS);
    }
}
