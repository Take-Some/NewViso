//! REDengine Ink HUD eligibility and placement contract.
//! A HUD .inkhud registers widget resources and visibility conditions; it
//! does not contain the native controller implementations for those widgets.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct InkHudEntry {
    pub name: String,
    pub widget: String,
    pub scene_tiers: BTreeSet<String>,
    pub game_contexts: BTreeSet<String>,
    pub enabled: bool,
    pub asset_present: bool,
    pub anchor: String,
    pub anchor_point: Value,
    pub attach_to_slot: bool,
    pub spawn_before_slots: bool,
    pub separate_window: bool,
    pub spawn_mode: String,
    pub slot_id: String,
    pub slot_layout: Value,
    pub margins: Value,
    pub glitch_effect: bool,
    pub blackwall_effect: bool,
}

#[derive(Clone, Debug)]
pub struct InkHudDocument {
    pub source: String,
    pub root_widget: String,
    pub entries: Vec<InkHudEntry>,
}

#[derive(Clone, Debug)]
pub struct InkHudCatalog {
    documents: BTreeMap<String, InkHudDocument>,
}

fn words(value: &Value) -> BTreeSet<String> {
    value.as_array().into_iter().flatten()
        .filter_map(Value::as_str).filter(|s| !s.trim().is_empty())
        .map(str::to_owned).collect()
}

impl InkHudCatalog {
    pub fn from_source(source: &Value) -> Result<Self, String> {
        if source.get("schema").and_then(Value::as_str)
            != Some("newviso.ink.hud_registry.v1")
        {
            return Err("unsupported Ink HUD registry schema".into());
        }
        let docs = source.get("documents").and_then(Value::as_array)
            .ok_or("Ink HUD registry has no documents")?;
        if docs.len() > 128 { return Err("Ink HUD registry exceeds document budget".into()); }
        let mut documents = BTreeMap::new();
        for doc in docs {
            let source = doc.get("source").and_then(Value::as_str)
                .filter(|s| !s.is_empty()).ok_or("Ink HUD has no source path")?;
            let rows = doc.get("entries").and_then(Value::as_array)
                .ok_or("Ink HUD has no entry list")?;
            if rows.len() > 2048 { return Err("Ink HUD exceeds entry budget".into()); }
            let mut entries = Vec::with_capacity(rows.len());
            for row in rows {
                entries.push(InkHudEntry {
                    name: row.get("name").and_then(Value::as_str)
                        .unwrap_or("").to_owned(),
                    widget: row.get("widget").and_then(Value::as_str)
                        .unwrap_or("").to_owned(),
                    scene_tiers: words(&row["scene_tiers"]),
                    game_contexts: words(&row["game_contexts"]),
                    enabled: row["enabled"].as_bool().unwrap_or(false),
                    asset_present: row["asset_present"].as_bool().unwrap_or(false),
                    anchor: row["anchor"].as_str().unwrap_or("Centered").to_owned(),
                    anchor_point: row["anchor_point"].clone(),
                    attach_to_slot: row["attach_to_slot"].as_bool().unwrap_or(false),
                    spawn_before_slots: row["spawn_before_slots"].as_bool().unwrap_or(false),
                    separate_window: row["separate_window"].as_bool().unwrap_or(false),
                    spawn_mode: row["spawn_mode"].as_str().unwrap_or("SingleAndMultiplayer").to_owned(),
                    slot_id: row["slot_id"].as_str().unwrap_or("").to_owned(),
                    slot_layout: row["slot_layout"].clone(),
                    margins: row["margins"].clone(),
                    glitch_effect: row["glitch_effect"].as_bool().unwrap_or(false),
                    blackwall_effect: row["blackwall_effect"].as_bool().unwrap_or(false),
                });
            }
            if documents.insert(source.to_owned(), InkHudDocument {
                source: source.to_owned(),
                root_widget: doc["root_widget"].as_str().unwrap_or("").to_owned(),
                entries,
            }).is_some() {
                return Err(format!("duplicate Ink HUD source '{source}'"));
            }
        }
        Ok(Self { documents })
    }

    pub fn documents(&self) -> impl Iterator<Item = &InkHudDocument> {
        self.documents.values()
    }

    pub fn get(&self, source: &str) -> Option<&InkHudDocument> {
        self.documents.get(source)
    }

    pub fn visible<'a>(
        &'a self,
        source: &str,
        scene_tier: &'a str,
        game_context: &'a str,
    ) -> impl Iterator<Item = &'a InkHudEntry> + 'a {
        self.documents.get(source).into_iter()
            .flat_map(|document| document.entries.iter())
            .filter(move |entry| {
                entry.enabled && entry.asset_present && !entry.widget.is_empty()
                    && (entry.scene_tiers.is_empty()
                        || entry.scene_tiers.contains(scene_tier))
                    && (entry.game_contexts.is_empty()
                        || entry.game_contexts.contains(game_context))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn context_and_scene_tier_gate_individual_hud_widgets() {
        let source = json!({"schema":"newviso.ink.hud_registry.v1",
            "documents":[{"source":"hud_camera.inkhud","entries":[
                {"name":"crosshair","widget":"ui/crosshair.inkwidget",
                 "spawn_before_slots":true,"separate_window":true,
                 "spawn_mode":"SingleAndMultiplayer","anchor_point":{"X":0.25,"Y":0.75},
                 "enabled":true,"asset_present":true,
                 "scene_tiers":["SceneTier4"],"game_contexts":["Default","Scanning"]},
                {"name":"vehicle","widget":"ui/speed.inkwidget",
                 "enabled":true,"asset_present":true,
                 "scene_tiers":["SceneTier4"],"game_contexts":["VehicleMounted"]},
                {"name":"missing","widget":"ui/missing.inkwidget",
                 "enabled":true,"asset_present":false,
                 "scene_tiers":[],"game_contexts":[]}
            ]}]});
        let hud = InkHudCatalog::from_source(&source).unwrap();
        let original = &hud.get("hud_camera.inkhud").unwrap().entries[0];
        assert!(original.spawn_before_slots);
        assert!(original.separate_window);
        assert_eq!(original.spawn_mode, "SingleAndMultiplayer");
        assert_eq!(original.anchor_point["X"], 0.25);
        assert_eq!(hud.visible("hud_camera.inkhud","SceneTier4","Default")
            .map(|e| e.name.as_str()).collect::<Vec<_>>(),vec!["crosshair"]);
        assert_eq!(hud.visible("hud_camera.inkhud","SceneTier4","VehicleMounted")
            .map(|e| e.name.as_str()).collect::<Vec<_>>(),vec!["vehicle"]);
        assert_eq!(hud.visible("hud_camera.inkhud","SceneTier2","Default").count(),0);
    }

    #[test]
    fn actual_16_hud_resources_parse_when_present() {
        let Ok(path) = std::env::var("NEWVISO_ORIGINAL_INK_HUD_REGISTRY_TEST_PATH") else {
            return;
        };
        let content = std::fs::read(path).unwrap();
        let source: Value = serde_json::from_slice(&content).unwrap();
        let hud = InkHudCatalog::from_source(&source).unwrap();
        assert_eq!(hud.documents().count(),16);
        let count: usize = hud.documents().map(|d| d.entries.len()).sum();
        assert!(count >= 500);
        assert!(hud.documents().any(|d| d.source.ends_with("hud_camera.inkhud")));
    }

    #[test]
    fn unknown_hud_mode_renders_no_unrelated_widgets() {
        let hud = InkHudCatalog::from_source(
            &json!({"schema":"newviso.ink.hud_registry.v1","documents":[]})
        ).unwrap();
        assert_eq!(hud.visible("not.installed","SceneTier4","Default").count(),0);
    }
}
