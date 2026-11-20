//! Night City player graphics, window, sound and camera preferences.
//! Cosmetic menu animations/Ink playback are immutable parts of the game.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct MenuPreferences {
    /// "default" uses the Shared Assets renderer policy.
    pub shadow_resolution: String,
    /// Configurable GPU PCF/PCSS penumbra parameter, initialized before Vulkan.
    pub shadow_softness: Option<f32>,
    /// "default" preserves the size authored by project runtime settings.
    pub resolution: String,
    pub camera_sensitivity: f32,
    pub zoom_sensitivity: f32,
    pub mouse_zoom_enabled: bool,
    pub ui_volume: f32,
    pub ui_sounds_enabled: bool,
}

impl Default for MenuPreferences {
    fn default() -> Self {
        Self {
            shadow_resolution: "default".into(),
            shadow_softness: None,
            resolution: "default".into(),
            camera_sensitivity: 1.0,
            zoom_sensitivity: 1.0,
            mouse_zoom_enabled: true,
            ui_volume: 1.0,
            ui_sounds_enabled: true,
        }
    }
}

impl MenuPreferences {
    pub(super) fn path() -> Option<PathBuf> {
        let root = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("APPDATA"))
            .or_else(|| std::env::var_os("HOME"))?;
        Some(PathBuf::from(root).join("NewViso").join("KaylaNightCity")
            .join("menu-settings.json"))
    }

    fn clamp(&mut self) {
        if !matches!(self.shadow_resolution.as_str(),
                     "default" | "1024" | "2048" | "4096" | "8192") {
            self.shadow_resolution = "default".into();
        }
        if !matches!(self.resolution.as_str(),
                     "default" | "1280x720" | "1440x900" | "1600x900" | "1920x1080") {
            self.resolution = "default".into();
        }
        self.shadow_softness = self.shadow_softness
            .filter(|v| v.is_finite())
            .map(|v| v.clamp(0.25, 4.0));
        if !self.camera_sensitivity.is_finite() { self.camera_sensitivity = 1.0; }
        if !self.zoom_sensitivity.is_finite() { self.zoom_sensitivity = 1.0; }
        if !self.ui_volume.is_finite() { self.ui_volume = 1.0; }
        self.camera_sensitivity = self.camera_sensitivity.clamp(0.5, 2.0);
        self.zoom_sensitivity = self.zoom_sensitivity.clamp(0.5, 2.0);
        self.ui_volume = self.ui_volume.clamp(0.0, 1.0);
    }

    pub(super) fn load() -> Self {
        let Some(path) = Self::path() else { return Self::default() };
        let Ok(bytes) = std::fs::read(path) else { return Self::default() };
        if bytes.len() > 16384 { return Self::default() }
        let Ok(mut preferences) = serde_json::from_slice::<Self>(&bytes) else {
            return Self::default();
        };
        preferences.clamp();
        preferences
    }

    pub(super) fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("player settings directory is unavailable")?;
        let parent = path.parent().ok_or("player settings parent is unavailable")?;
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create player settings directory: {e}"))?;
        let staging = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&staging, bytes)
            .map_err(|e| format!("write player settings: {e}"))?;
        // A failed write never truncates the previously committed preferences.
        if let Err(error) = std::fs::rename(&staging, &path) {
            if path.exists() {
                std::fs::remove_file(&path).map_err(|e| e.to_string())?;
                std::fs::rename(&staging, &path).map_err(|e| e.to_string())?;
            } else {
                return Err(format!("commit player settings: {error}"));
            }
        }
        Ok(())
    }

    pub(super) fn apply_value(&mut self, key: &str, value: &Value) -> bool {
        let before = self.clone();
        match key {
            "shadow_resolution" => {
                let Some(s) = value.as_str() else { return false };
                self.shadow_resolution = s.to_owned();
            }
            "shadow_softness" => {
                let Some(v) = value.as_f64().filter(|v| v.is_finite()) else { return false };
                self.shadow_softness = Some(v as f32);
            }
            "resolution" => {
                let Some(s) = value.as_str() else { return false };
                self.resolution = s.to_owned();
            }
            "camera_sensitivity" => {
                let Some(v) = value.as_f64().filter(|v| v.is_finite()) else { return false };
                self.camera_sensitivity = v as f32;
            }
            "zoom_sensitivity" => {
                let Some(v) = value.as_f64().filter(|v| v.is_finite()) else { return false };
                self.zoom_sensitivity = v as f32;
            }
            "mouse_zoom_enabled" => {
                let Some(v) = value.as_bool() else { return false };
                self.mouse_zoom_enabled = v;
            }
            "ui_volume" => {
                let Some(v) = value.as_f64().filter(|v| v.is_finite()) else { return false };
                self.ui_volume = v as f32;
            }
            "ui_sounds_enabled" => {
                let Some(v) = value.as_bool() else { return false };
                self.ui_sounds_enabled = v;
            }
            _ => return false,
        }
        self.clamp();
        *self != before
    }

    pub(super) fn control_value(&self, key: &str) -> Option<Value> {
        Some(match key {
            "shadow_resolution" => json!(self.shadow_resolution),
            "shadow_softness" => json!(self.shadow_softness.unwrap_or(1.0)),
            "resolution" => json!(self.resolution),
            "camera_sensitivity" => json!(self.camera_sensitivity),
            "zoom_sensitivity" => json!(self.zoom_sensitivity),
            "mouse_zoom_enabled" => json!(self.mouse_zoom_enabled),
            "ui_volume" => json!(self.ui_volume),
            "ui_sounds_enabled" => json!(self.ui_sounds_enabled),
            _ => return None,
        })
    }

    pub(super) fn restart_required(&self, key: &str) -> bool {
        matches!(key, "shadow_resolution" | "shadow_softness" | "resolution")
    }

    pub(super) fn window_size(&self) -> Option<(u32, u32)> {
        match self.resolution.as_str() {
            "1280x720" => Some((1280, 720)),
            "1440x900" => Some((1440, 900)),
            "1600x900" => Some((1600, 900)),
            "1920x1080" => Some((1920, 1080)),
            _ => None,
        }
    }

    pub(super) fn renderer_policy_patch(&self) -> Value {
        let mut patch = serde_json::Map::new();
        if self.shadow_resolution != "default" {
            if let Ok(resolution) = self.shadow_resolution.parse::<u32>() {
                patch.insert("shadow_resolution".into(), json!(resolution));
            }
        }
        if let Some(softness) = self.shadow_softness {
            patch.insert("shadow_softness".into(), json!(softness));
        }
        Value::Object(patch)
    }

    pub(super) fn camera_multiplier(&self) -> f32 {
        self.camera_sensitivity
    }
    pub(super) fn zoom_multiplier(&self) -> f32 {
        if self.mouse_zoom_enabled { self.zoom_sensitivity } else { 0.0 }
    }
}

const CATEGORIES: &[&str] = &["graphics", "display", "controls", "sound"];
pub(super) fn is_category(category: &str) -> bool {
    CATEGORIES.contains(&category)
}

pub(super) fn apply_to_surface(
    surface: &mut Value,
    prefs: &MenuPreferences,
    settings_open: bool,
    active_category: &str,
) {
    if let Some(props) = surface.pointer_mut("/metrics/presentation_props") {
        // Menu look and original Ink animation are NEVER derived from settings.
        props["reduce_motion"] = Value::Bool(false);
    }
    fn update(nodes: &mut Vec<Value>, prefs: &MenuPreferences,
              settings_open: bool, active_category: &str) {
        nodes.retain(|node| {
            node.pointer("/props/menu_category").and_then(Value::as_str)
                .is_none_or(|category| !settings_open || category == active_category)
        });
        for node in nodes {
            let id = node.get("id").and_then(Value::as_str).unwrap_or("").to_owned();
            if let Some(category) = node.get("action_id").and_then(Value::as_str)
                .and_then(|id| id.strip_prefix("newviso.menu.category.")) {
                let selected = category == active_category;
                node["state_tags"] = if selected {
                    json!(["selected"])
                } else {
                    json!([])
                };
                if selected {
                    node["props"]["cyberpunk_accent_rgba"] = json!([255, 227, 47, 255]);
                }
            }
            if let Some(key) = node.pointer("/props/menu_setting").and_then(Value::as_str) {
                if let Some(value) = prefs.control_value(key) {
                    node["props"]["value"] = value;
                }
            }
            if id == "settings.chrome.save" {
                node["text"] = Value::String(
                    "ПАРАМЕТРЫ СОХРАНЯЮТСЯ АВТОМАТИЧЕСКИ  //  ВИДЕО И ТЕНИ — ПОСЛЕ ПЕРЕЗАПУСКА".into()
                );
            }
            if matches!(node.get("component_id").and_then(Value::as_str),
                        Some("button" | "toggle" | "checkbox" | "slider" | "select")) {
                // Only sound levels are adjustable. UI appearance and animations remain fixed.
                let gain = prefs.ui_volume * if prefs.ui_sounds_enabled { 1.0 } else { 0.0 };
                for (property, default_gain) in [
                    ("sound_hover_gain", 0.30),
                    ("sound_press_gain", 0.19),
                    ("sound_click_gain", 0.54),
                ] {
                    if node["props"].get(property).is_some() {
                        node["props"][property] = json!(gain * default_gain);
                    }
                }
            }
            if let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) {
                update(children, prefs, settings_open, active_category);
            }
        }
    }
    if let Some(nodes) = surface.get_mut("components").and_then(Value::as_array_mut) {
        update(nodes, prefs, settings_open, active_category);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_validate_and_do_not_include_animation_options() {
        let mut prefs = MenuPreferences::default();
        assert!(prefs.apply_value("ui_volume", &json!(0.65)));
        assert!((prefs.ui_volume - 0.65).abs() < 0.001);
        assert!(prefs.apply_value("mouse_zoom_enabled", &json!(false)));
        assert_eq!(prefs.zoom_multiplier(), 0.0);
        assert!(!prefs.apply_value("reduced_motion", &json!(true)));
        assert!(!prefs.apply_value("logo_motion", &json!(false)));
        assert!(!prefs.apply_value("shadow_resolution", &json!("99999")));
        assert_eq!(prefs.shadow_resolution, "default");
        assert!(prefs.apply_value("shadow_resolution", &json!("4096")));
        assert_eq!(prefs.renderer_policy_patch()["shadow_resolution"], 4096);
        assert!(prefs.apply_value("resolution", &json!("1920x1080")));
        assert_eq!(prefs.window_size(), Some((1920, 1080)));
    }

    #[test]
    fn surface_values_are_real_and_immutable_menu_fx_remain_enabled() {
        let mut doc = json!({
            "metrics": {"presentation_props": {"reduce_motion": true}},
            "components": [
                {"id":"tab","component_id":"button",
                 "action_id":"newviso.menu.category.sound","props":{}},
                {"id":"volume","component_id":"slider",
                 "props":{"menu_setting":"ui_volume","menu_category":"sound"}},
                {"id":"resolution","component_id":"select",
                 "props":{"menu_setting":"resolution","menu_category":"display"}}
            ]
        });
        apply_to_surface(&mut doc, &MenuPreferences::default(), true, "sound");
        assert_eq!(doc["metrics"]["presentation_props"]["reduce_motion"], false);
        assert_eq!(doc["components"].as_array().unwrap().len(), 2);
        assert_eq!(doc["components"][1]["props"]["value"], 1.0);
        assert_eq!(doc["components"][0]["state_tags"][0], "selected");
    }
}
