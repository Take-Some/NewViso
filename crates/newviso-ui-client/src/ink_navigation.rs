//! Provider-independent REDengine Ink menu graph and input routing.
//!
//! Ink resources describe widget structure, named controller classes, menu
//! registrations and animation references. They do NOT ship the native
//! implementation of the corresponding Cyberpunk 2077 controllers.
//! This module handles engine-owned navigation, focus, state and events.
//! Gameplay controllers must be explicitly bound to the engine's services.

use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

const MAX_MENU_ENTRIES: usize = 2048;
const MAX_STACK_DEPTH: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkRegisteredScreen {
    pub name: String,
    pub widget: String,
    pub source_menu: String,
    pub input_context: String,
    pub depth: i32,
    pub fadeout: bool,
    pub asset_present: bool,
}

#[derive(Clone, Debug, Default)]
pub struct InkScreenRegistry {
    screens: BTreeMap<String, InkRegisteredScreen>,
    scenarios: BTreeSet<String>,
}

impl InkScreenRegistry {
    pub fn from_source(value: &Value) -> Result<Self, String> {
        if value.get("schema").and_then(Value::as_str) != Some("newviso.ink.registry.v1") {
            return Err("unsupported Ink screen registry schema".to_owned());
        }
        let documents = value
            .get("menu_documents").and_then(Value::as_array)
            .ok_or("Ink registry has no menu documents")?;
        let mut registry = Self::default();
        let mut count = 0usize;
        for document in documents {
            let source = document.get("source").and_then(Value::as_str)
                .ok_or("Ink menu source missing")?;
            if source.is_empty() { return Err("empty Ink menu source".to_owned()); }
            for scenario in document.get("scenarios").and_then(Value::as_array)
                .into_iter().flatten()
            {
                if let Some(name) = scenario.as_str() {
                    if !name.is_empty() { registry.scenarios.insert(name.to_owned()); }
                }
            }
            let entries = document.get("entries").and_then(Value::as_array)
                .ok_or_else(|| format!("Ink menu '{source}' has no entries"))?;
            for entry in entries {
                count += 1;
                if count > MAX_MENU_ENTRIES {
                    return Err("Ink menu registry exceeds entry limit".to_owned());
                }
                let name = entry.get("name").and_then(Value::as_str)
                    .ok_or("Ink menu entry has no name")?;
                let Some(widget) = entry.get("widget").and_then(Value::as_str) else {
                    continue;
                };
                if name.trim().is_empty() || widget.is_empty() {
                    continue;
                }
                // Fail closed: metadata for a screen not found in the
                // original extraction must never become an active page.
                let asset_present = entry.get("asset_present")
                    .and_then(Value::as_bool).unwrap_or(false);
                let screen = InkRegisteredScreen {
                    name: name.to_owned(),
                    widget: widget.to_owned(),
                    source_menu: source.to_owned(),
                    input_context: entry.get("input_context")
                        .and_then(Value::as_str).unwrap_or("None").to_owned(),
                    depth: entry.get("depth").and_then(Value::as_i64)
                        .unwrap_or(0).clamp(i64::from(i32::MIN),i64::from(i32::MAX)) as i32,
                    fadeout: entry.get("fadeout").and_then(Value::as_bool)
                        .unwrap_or(false),
                    asset_present,
                };
                // The original shares settings, load, controller and other
                // pages across pregame / in-game registries. That is a
                // multi-parent route, not an ambiguous resource override.
                if let Some(previous) = registry.screens.get(name) {
                    if previous.widget != screen.widget {
                        return Err(format!(
                            "Ink route '{name}' has conflicting widget paths: '{}' vs '{}'",
                            previous.widget, screen.widget
                        ));
                    }
                } else {
                    registry.screens.insert(name.to_owned(), screen);
                }
            }
        }
        Ok(registry)
    }

    pub fn get(&self, name: &str) -> Option<&InkRegisteredScreen> {
        self.screens.get(name)
    }

    pub fn contains_scenario(&self, name: &str) -> bool {
        self.scenarios.contains(name)
    }

    pub fn screen_count(&self) -> usize { self.screens.len() }
    pub fn scenario_count(&self) -> usize { self.scenarios.len() }

    pub fn screens(&self) -> impl Iterator<Item = &InkRegisteredScreen> {
        self.screens.values()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkOpenMode { Replace, Push, Overlay }

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InkActivePage {
    pub name: String,
    pub focused_widget: Option<String>,
    pub overlay: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InkNavigationEvent {
    Opened { screen: String, widget: String, overlay: bool },
    Closed { screen: String },
    FocusChanged { screen: String, widget: Option<String> },
    Action { screen: String, action_id: String },
    PropertyChanged { screen: String, path: String, value: Value },
}

impl InkNavigationEvent {
    /// Typed payload for the engine's event bus. Gameplay scripts can bind
    /// original controller behavior without depending on a concrete UI provider.
    pub fn to_payload(&self) -> Value {
        match self {
            Self::Opened { screen, widget, overlay } =>
                serde_json::json!({"schema":"newviso.ink.event.v1",
                    "kind":"opened","screen":screen,"widget":widget,"overlay":overlay}),
            Self::Closed { screen } =>
                serde_json::json!({"schema":"newviso.ink.event.v1",
                    "kind":"closed","screen":screen}),
            Self::FocusChanged { screen, widget } =>
                serde_json::json!({"schema":"newviso.ink.event.v1",
                    "kind":"focus","screen":screen,"widget":widget}),
            Self::Action { screen, action_id } =>
                serde_json::json!({"schema":"newviso.ink.event.v1",
                    "kind":"action","screen":screen,"action_id":action_id}),
            Self::PropertyChanged { screen, path, value } =>
                serde_json::json!({"schema":"newviso.ink.event.v1",
                    "kind":"property","screen":screen,"path":path,"value":value}),
        }
    }
}

#[derive(Clone, Debug)]
pub struct InkUiNavigation {
    registry: InkScreenRegistry,
    stack: Vec<InkActivePage>,
    properties: BTreeMap<String, Map<String, Value>>,
    pending: Vec<InkNavigationEvent>,
}

impl InkUiNavigation {
    pub fn new(registry: InkScreenRegistry) -> Self {
        Self { registry, stack: Vec::new(), properties: BTreeMap::new(),
               pending: Vec::new() }
    }

    pub fn registry(&self) -> &InkScreenRegistry { &self.registry }
    pub fn current(&self) -> Option<&InkActivePage> { self.stack.last() }
    pub fn stack(&self) -> &[InkActivePage] { &self.stack }

    pub fn open(&mut self, name: &str, mode: InkOpenMode) -> Result<(), String> {
        let source = self.registry.get(name)
            .ok_or_else(|| format!("unregistered Ink screen: '{name}'"))?;
        if !source.asset_present {
            return Err(format!("Ink screen '{}' lacks its original widget", name));
        }
        if matches!(mode, InkOpenMode::Push | InkOpenMode::Overlay)
            && self.stack.len() >= MAX_STACK_DEPTH
        {
            return Err("Ink navigation stack depth exceeded".to_owned());
        }
        let widget = source.widget.clone();
        if self.current().is_some_and(|screen| screen.name == name) {
            return Ok(());
        }
        if matches!(mode, InkOpenMode::Replace) {
            while let Some(page) = self.stack.pop() {
                self.pending.push(InkNavigationEvent::Closed { screen: page.name });
            }
        }
        self.stack.push(InkActivePage {
            name: name.to_owned(),
            focused_widget: None,
            overlay: matches!(mode, InkOpenMode::Overlay),
        });
        self.pending.push(InkNavigationEvent::Opened {
            screen: name.to_owned(), widget,
            overlay: matches!(mode, InkOpenMode::Overlay),
        });
        Ok(())
    }

    pub fn back(&mut self) -> bool {
        // The root screen is deliberately not dismissed by the generic Back
        // action: the caller must explicitly decide to exit/pause/unpause.
        if self.stack.len() <= 1 { return false; }
        let page = self.stack.pop().expect("checked stack");
        self.pending.push(InkNavigationEvent::Closed { screen: page.name });
        true
    }

    pub fn close(&mut self) -> Option<String> {
        let page = self.stack.pop()?;
        self.pending.push(InkNavigationEvent::Closed {
            screen: page.name.clone(),
        });
        Some(page.name)
    }

    pub fn focus(&mut self, widget: Option<&str>) -> Result<(), String> {
        let page = self.stack.last_mut().ok_or("Ink UI has no current screen")?;
        let new = widget.map(str::to_owned);
        if page.focused_widget != new {
            page.focused_widget = new.clone();
            self.pending.push(InkNavigationEvent::FocusChanged {
                screen: page.name.clone(), widget: new,
            });
        }
        Ok(())
    }

    pub fn handle_action(&mut self, action_id: &str) -> Result<(), String> {
        if action_id == "newviso.ink.back" {
            self.back();
            return Ok(());
        }
        if let Some(name) = action_id.strip_prefix("newviso.ink.open.") {
            return self.open(name, InkOpenMode::Push);
        }
        if let Some(name) = action_id.strip_prefix("newviso.ink.replace.") {
            return self.open(name, InkOpenMode::Replace);
        }
        if let Some(name) = action_id.strip_prefix("newviso.ink.overlay.") {
            return self.open(name, InkOpenMode::Overlay);
        }
        let current = self.current().ok_or("Ink action with no active screen")?;
        self.pending.push(InkNavigationEvent::Action {
            screen: current.name.clone(), action_id: action_id.to_owned(),
        });
        Ok(())
    }

    /// Directly route native engine.ui action events; no knowledge of Egui,
    /// Aurelia or input-device layout is encoded in the navigation layer.
    pub fn dispatch(&mut self, result: &Value, surface_id: &str) -> Result<(), String> {
        if let Some(actions) = result.get("actions").and_then(Value::as_array) {
            for action in actions {
                if action.get("surface_id").and_then(Value::as_str) != Some(surface_id) {
                    continue;
                }
                let Some(id) = action.get("action_id").and_then(Value::as_str) else {
                    continue;
                };
                self.handle_action(id)?;
            }
        }
        if let Some(patches) = result.get("state_patches").and_then(Value::as_array) {
            for patch in patches {
                if patch.get("surface_id").and_then(Value::as_str) != Some(surface_id) {
                    continue;
                }
                let Some(screen) = self.current().map(|screen| screen.name.clone()) else {
                    continue;
                };
                for change in patch.get("changes").and_then(Value::as_array)
                    .into_iter().flatten()
                {
                    let (Some(path), Some(value)) = (
                        change.get("path").and_then(Value::as_str), change.get("value"),
                    ) else { continue };
                    if path.len() > 1024 { return Err("Ink property path exceeds limit".into()); }
                    self.properties.entry(screen.clone()).or_default()
                        .insert(path.to_owned(), value.clone());
                    self.pending.push(InkNavigationEvent::PropertyChanged {
                        screen: screen.clone(), path: path.to_owned(),
                        value: value.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn property(&self, screen: &str, path: &str) -> Option<&Value> {
        self.properties.get(screen)?.get(path)
    }

    pub fn take_events(&mut self) -> Vec<InkNavigationEvent> {
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn registry() -> InkScreenRegistry {
        InkScreenRegistry::from_source(&json!({
            "schema":"newviso.ink.registry.v1",
            "menu_documents":[
                {"source":"pregame","scenarios":["MenuScenario_NewGame"],
                 "entries":[
                    {"name":"singleplayer_menu","widget":"base/menu.inkwidget",
                     "asset_present":true,"depth":100},
                    {"name":"settings_main","widget":"base/settings.inkwidget",
                     "asset_present":true}
                ]},
                {"source":"in_game","scenarios":["MenuScenario_Pause"],
                 "entries":[
                    {"name":"settings_main","widget":"base/settings.inkwidget",
                     "asset_present":true},
                    {"name":"pause_menu","widget":"base/pause.inkwidget",
                     "asset_present":true}
                ]}
            ]
        })).unwrap()
    }

    #[test]
    fn two_original_menu_registries_share_names_without_duplicate_screens() {
        let registry = registry();
        assert_eq!(registry.screen_count(),3);
        assert!(registry.contains_scenario("MenuScenario_NewGame"));
        assert_eq!(registry.scenario_count(),2);
    }

    #[test]
    fn push_and_back_restore_previous_screen_and_focus() {
        let mut state = InkUiNavigation::new(registry());
        state.open("singleplayer_menu",InkOpenMode::Replace).unwrap();
        state.focus(Some("mainListVert")).unwrap();
        state.open("settings_main",InkOpenMode::Push).unwrap();
        state.focus(Some("settings.volume")).unwrap();
        assert_eq!(state.current().unwrap().name,"settings_main");
        assert!(state.back());
        assert_eq!(state.current().unwrap().focused_widget.as_deref(),
                   Some("mainListVert"));
        assert!(!state.back());
    }

    #[test]
    fn overlay_and_property_events_are_provider_independent() {
        let mut state = InkUiNavigation::new(registry());
        state.open("singleplayer_menu",InkOpenMode::Replace).unwrap();
        state.open("settings_main",InkOpenMode::Overlay).unwrap();
        state.dispatch(&json!({
            "actions":[{"surface_id":"other","action_id":"newviso.ink.back"}],
            "state_patches":[{"surface_id":"actual","changes":[
                {"path":"nodes.volume/value","value":0.65}
            ]}]
        }),"actual").unwrap();
        assert!(state.current().unwrap().overlay);
        assert_eq!(state.property("settings_main","nodes.volume/value"),Some(&json!(0.65)));
        assert!(state.take_events().iter().any(|event|
            matches!(event,InkNavigationEvent::PropertyChanged{screen,..}
                if screen=="settings_main")));
    }

    #[test]
    fn unknown_screen_cannot_be_activated() {
        let mut state = InkUiNavigation::new(registry());
        assert!(state.open("missing",InkOpenMode::Push).is_err());
        assert!(state.stack().is_empty());
    }

    #[test]
    fn action_and_property_events_have_stable_gameplay_bus_payloads() {
        let action = InkNavigationEvent::Action {
            screen: "settings_main".into(),
            action_id: "settings.apply".into(),
        };
        assert_eq!(action.to_payload(),json!({
            "schema":"newviso.ink.event.v1",
            "kind":"action","screen":"settings_main","action_id":"settings.apply"
        }));
        let patch = InkNavigationEvent::PropertyChanged {
            screen:"settings_main".into(),
            path:"nodes.audio.volume/value".into(),
            value:json!(0.75),
        };
        assert_eq!(patch.to_payload()["value"],json!(0.75));
    }

    #[test]
    fn original_registry_can_be_loaded_from_local_game_when_supplied() {
        let Ok(path) = std::env::var("NEWVISO_ORIGINAL_INK_MENU_REGISTRY_TEST_PATH") else {
            return;
        };
        let content = std::fs::read(path).expect("original Ink registry file");
        let source: Value = serde_json::from_slice(&content).unwrap();
        let registry = InkScreenRegistry::from_source(&source).unwrap();
        assert!(registry.screen_count() >= 60);
        assert!(registry.scenario_count() >= 20);
        for name in ["singleplayer_menu","new_game_menu","settings_main","pause_menu",
                     "inventory_screen","world_map","crafting_main","load_game"] {
            assert!(registry.get(name).is_some(),"missing original route: {name}");
        }
        assert!(registry.get("singleplayer_menu").unwrap().asset_present);
    }
}
