//! Project-authored startup/menu flow, independent of gameplay rules and UI provider.
use super::*;
use newviso_input_client::key;
use newviso_ui_client::ink_navigation::{InkScreenRegistry, InkUiNavigation, InkOpenMode};
use newviso_ui_client::ink_virtual_lists::{self, InkListKind, InkVirtualListState};
use super::application_main_menu_settings::{self, MenuPreferences};

#[derive(Clone, Debug)]
pub(super) struct MainMenuState {
    pub surface_id: String,
    pub visible: bool,
    pub started: bool,
    pub launch_requested: bool,
    pub settings_open: bool,
    pub reduced_motion: bool,
    pub prefs: MenuPreferences,
    pub applied_camera_multiplier: f32,
    pub applied_zoom_multiplier: f32,
    pub category: String,
    pub ink: Option<application_ink_animation::InkMenuPlayback>,
    pub hovered_action: Option<String>,
    pub ink_registry_asset: Option<String>,
    pub ink_navigation: Option<InkUiNavigation>,
    pub ink_registry_attempted: bool,
    pub ink_preview_published: bool,
    pub ink_preview_name: Option<String>,
    pub ink_preview_source: Option<Value>,
    pub ink_preview_playback: Option<application_ink_animation::InkMenuPlayback>,
    pub ink_preview_revision: Option<u64>,
    pub ink_preview_list: InkVirtualListState,
    pub ink_preview_visible_rows: usize,
    pub ink_preview_total_rows: usize,
    pub ink_preview_save_status: Option<String>,
    /// Brief UI feedback window before committing a menu transition.
    pub pending_action: Option<String>,
    pub transition_remaining: f32,
    pub click_transition_seconds: f32,
}

impl MainMenuState {
    fn queue_action(&mut self, action: &str) {
        if self.pending_action.is_none() {
            self.pending_action = Some(action.to_owned());
            self.transition_remaining = self.click_transition_seconds;
        }
    }

    fn advance_transition(&mut self, dt: f32) -> Option<String> {
        if self.pending_action.is_none() {
            return None;
        }
        self.transition_remaining -= dt.max(0.0);
        if self.transition_remaining <= 0.0 {
            self.pending_action.take()
        } else {
            None
        }
    }

    pub fn from_template(template: Option<&Value>) -> Option<Self> {
        let template = template?;
        if template.pointer("/metrics/startup_menu").and_then(Value::as_bool) != Some(true) {
            return None;
        }
        let surface_id = template.get("surface_id")?.as_str()?.trim();
        if surface_id.is_empty() {
            return None;
        }
        let prefs = if surface_id == "kayla.nightcity.main_menu" {
            MenuPreferences::load()
        } else {
            MenuPreferences::default()
        };
        Some(Self {
            surface_id: surface_id.to_owned(),
            visible: true,
            started: false,
            launch_requested: false,
            settings_open: false,
            reduced_motion: false,
            applied_camera_multiplier: prefs.camera_multiplier(),
            applied_zoom_multiplier: prefs.zoom_sensitivity,
            prefs,
            category: "graphics".to_owned(),
            ink: application_ink_animation::InkMenuPlayback::from_template(template),
            hovered_action: None,
            ink_registry_asset: template.pointer("/metrics/ink_menu_registry")
                .and_then(Value::as_str).filter(|asset| !asset.is_empty()).map(str::to_owned),
            ink_navigation: None,
            ink_registry_attempted: false,
            ink_preview_published: false,
            ink_preview_name: None,
            ink_preview_source: None,
            ink_preview_playback: None,
            ink_preview_revision: None,
            ink_preview_list: InkVirtualListState::default(),
            ink_preview_visible_rows: 0,
            ink_preview_total_rows: 0,
            ink_preview_save_status: None,
            pending_action: None,
            transition_remaining: 0.0,
            click_transition_seconds: (template.pointer("/metrics/click_transition_ms")
                .and_then(Value::as_f64).unwrap_or(210.0).clamp(0.0, 600.0) as f32) / 1000.0,
        })
    }
}

/// Keep menu layouts declarative: only engine state determines which action
/// is available. The UI provider receives a standard surface with standard
/// action_id fields and has no knowledge of gameplay lifecycle.
pub(super) fn prepare_menu_surface(
    surface: &mut Value,
    started: bool,
    settings_open: bool,
    _reduced_motion: bool,
) {
    fn filter_nodes(
        nodes: &mut Vec<Value>,
        started: bool,
        settings_open: bool,
        _reduced_motion: bool,
    ) {
        nodes.retain(|node| {
            let page = node.pointer("/props/menu_page").and_then(Value::as_str);
            if matches!(page, Some("main")) && settings_open
                || matches!(page, Some("settings")) && !settings_open
            {
                return false;
            }
            match node.get("action_id").and_then(Value::as_str) {
                Some("newviso.menu.start") => !started,
                Some("newviso.menu.resume") => started,
                _ => true,
            }
        });
        for node in nodes {
            if let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) {
                filter_nodes(children, started, settings_open, false);
            }
        }
    }
    if let Some(props) = surface.pointer_mut("/metrics/presentation_props") {
        props["reduce_motion"] = Value::Bool(false); // authored game animation is immutable
    }
    if let Some(components) = surface.get_mut("components").and_then(Value::as_array_mut) {
        filter_nodes(components, started, settings_open, false);
    }
}

fn valid_ink_preview_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64
        && name.bytes().all(|v| v.is_ascii_alphanumeric() || v == b'_')
}

impl EngineApplication {
    pub(super) fn start_project_scripts(&mut self) -> Result<(), String> {
        if self.main_menu.as_ref().is_some_and(|menu| menu.started) {
            return Ok(());
        }
        if self.scripts.is_some() {
            bugtrap::set_phase("scripts.start");
            let runtime_state = self.script_frame_state();
            let control = {
                let scripts = self.scripts.as_mut().ok_or_else(|| {
                    "script runtime disappeared during game start".to_owned()
                })?;
                scripts.start_with_runtime(&self.project_context, &runtime_state)?
            };
            self.exit_requested |= control.exit_requested;
            self.ui_bindings.extend(control.ui_bindings);
            self.apply_script_commands(&control.commands)?;
            self.sync_world_actor_presentations()?;
        }
        if let Some(menu) = self.main_menu.as_mut() {
            menu.started = true;
        }
        self.world_save_allowed = true;
        Ok(())
    }

    fn apply_menu_actions(&mut self, dispatch: &Value) -> Result<(), String> {
        let Some(menu) = self.main_menu.as_ref() else { return Ok(()); };
        let surface_id = menu.surface_id.clone();
        let Some(actions) = dispatch.get("actions").and_then(Value::as_array) else {
            return Ok(());
        };
        let action = actions.iter().filter(|entry| {
            entry.get("surface_id").and_then(Value::as_str) == Some(surface_id.as_str())
        }).filter_map(|entry| entry.get("action_id").and_then(Value::as_str)).find(|id| {
            matches!(*id, "newviso.menu.start" | "newviso.menu.resume"
                | "newviso.menu.quit" | "newviso.menu.settings"
                | "newviso.menu.back")
                || id.starts_with("newviso.menu.category.")

        }).map(str::to_owned);
        if let Some(action) = action {
            if let Some(menu) = self.main_menu.as_mut() {
                menu.queue_action(&action);
            }
        }
        Ok(())
    }

    fn apply_menu_value_patches(&mut self, dispatch: &Value) -> Result<(), String> {
        let Some(menu) = self.main_menu.as_ref() else { return Ok(()) };
        if !menu.settings_open { return Ok(()) }
        let surface_id = menu.surface_id.clone();
        let category = menu.category.clone();
        let Some(patches) = dispatch.get("state_patches").and_then(Value::as_array) else {
            return Ok(());
        };
        let mut change_count = 0;
        let mut camera_changed = false;
        for patch in patches {
            if patch.get("surface_id").and_then(Value::as_str) != Some(surface_id.as_str()) {
                continue;
            }
            let Some(changes) = patch.get("changes").and_then(Value::as_array) else { continue };
            for change in changes {
                let Some(path) = change.get("path").and_then(Value::as_str) else { continue };
                let Some(path) = path.strip_prefix("nodes.settings.control.") else { continue };
                let Some(path) = path.strip_suffix("/value") else { continue };
                let Some((control_category, key)) = path.split_once('.') else { continue };
                if control_category != category || !application_main_menu_settings::is_category(control_category) {
                    continue;
                }
                let Some(value) = change.get("value") else { continue };
                if let Some(menu) = self.main_menu.as_mut() {
                    if menu.prefs.apply_value(key, value) {
                        camera_changed |= matches!(key,
                            "camera_sensitivity" | "zoom_sensitivity" | "mouse_zoom_enabled");
                        change_count += 1;
                    }
                }
            }
        }
        if change_count > 0 {
            let prefs = self.main_menu.as_ref().expect("menu checked").prefs.clone();
            if camera_changed {
                let menu = self.main_menu.as_ref().expect("menu checked");
                let old_rotate = menu.applied_camera_multiplier.max(0.00001);
                let old_zoom = menu.applied_zoom_multiplier.max(0.00001);
                self.settings.camera.rotate_sensitivity *= prefs.camera_multiplier() / old_rotate;
                self.settings.camera.zoom_sensitivity *= prefs.zoom_sensitivity / old_zoom;
                self.scene.configure_orbit(
                    self.settings.camera.rotate_sensitivity,
                    self.settings.camera.zoom_sensitivity,
                    self.settings.camera.min_distance,
                    self.settings.camera.max_distance,
                )?;
                self.scene.set_orbit_zoom_enabled(prefs.mouse_zoom_enabled);
                if let Some(menu) = self.main_menu.as_mut() {
                    menu.applied_camera_multiplier = prefs.camera_multiplier();
                    menu.applied_zoom_multiplier = prefs.zoom_sensitivity;
                }
            }
            if let Err(error) = prefs.save() {
                host::warn("newviso.menu", format!("save user settings failed: {error}"));
            }
        }
        Ok(())
    }

    /// Load the source Ink menu graph once; providers remain responsible only
    /// for painting the active project-authored UI surfaces.
    fn initialize_source_ink_navigation(&mut self) {
        let Some(menu) = self.main_menu.as_mut() else { return; };
        if menu.ink_registry_attempted { return; }
        menu.ink_registry_attempted = true;
        let Some(asset) = menu.ink_registry_asset.as_deref() else { return; };
        let result = AssetClient::new().json(asset)
            .and_then(|source| InkScreenRegistry::from_source(&source));
        match result {
            Ok(registry) => {
                let screen_count = registry.screen_count();
                let scenarios = registry.scenario_count();
                let mut navigation = InkUiNavigation::new(registry);
                if let Err(error) = navigation.open("singleplayer_menu", InkOpenMode::Replace) {
                    host::warn("newviso.ink", format!("no initial source Ink screen: {error}"));
                }
                host::info("newviso.ink", format!(
                    "registered original Ink menu graph screens={} scenarios={} source='{}'",
                    screen_count, scenarios, asset
                ));
                menu.ink_navigation = Some(navigation);
            }
            Err(error) => {
                host::warn("newviso.ink", format!(
                    "original Ink menu graph '{}' was not mounted: {}", asset, error
                ));
            }
        }
    }

    /// Developer-only source graph preview, isolated from the working menu.
    /// Publishing here exercises the actual engine.ui renderer with the
    /// original tree and source-bound UV crops, without changing gameplay.
    fn publish_source_ink_preview_if_requested(&mut self, dt: f32) -> Result<(), String> {
        let Some(menu) = self.main_menu.as_ref() else { return Ok(()); };
        let Ok(name) = std::env::var("NEWVISO_INK_PREVIEW_SCREEN") else {
            return Ok(());
        };
        if !valid_ink_preview_name(&name) {
            host::warn("newviso.ink", "invalid source screen preview identifier");
            return Ok(());
        }
        let kind=InkListKind::from_screen(&name);
        let items_state=kind.map(|_|self.items.runtime_state());
        let revision=items_state.as_ref().and_then(|v|v["revision"].as_u64());
        let same_name=menu.ink_preview_name.as_deref()==Some(name.as_str());
        let running_animation=same_name && menu.ink_preview_playback.is_some();
        if menu.ink_preview_published && same_name
            && menu.ink_preview_revision==revision && !running_animation
        {
            return Ok(());
        }
        // Cache the original widget tree. Re-publishing a fully reparsed
        // multi-megabyte source surface every frame would cause CPU stalls.
        if !same_name || menu.ink_preview_source.is_none() {
            let source_ref=format!("ui/ink/generated/{name}.surface.json");
            let source=match AssetClient::new().json(&source_ref) {
                Ok(source)=>source,
                Err(error)=>{
                    host::warn("newviso.ink",format!(
                        "source preview '{source_ref}' unavailable: {error}"
                    ));
                    return Ok(());
                }
            };
            if source["source"]!="cyberpunk-ink-cr2w"
                || !source["components"].as_array().is_some_and(|v|!v.is_empty())
            {
                host::warn("newviso.ink",format!(
                    "source preview '{name}' has no draw-ready content"
                ));
                return Ok(());
            }
            let menu=self.main_menu.as_mut().expect("menu checked");
            menu.ink_preview_playback=
                application_ink_animation::InkMenuPlayback::from_template(&source);
            if let Some(playback)=menu.ink_preview_playback.as_mut() {
                playback.ensure_loaded();
            }
            menu.ink_preview_source=Some(source);
            menu.ink_preview_name=Some(name.clone());
            menu.ink_preview_list=InkVirtualListState::default();
        }
        let mut source=self.main_menu.as_ref().expect("menu checked")
            .ink_preview_source.as_ref().expect("source loaded").clone();
        let mut virtual_metrics=None;
        if let (Some(kind),Some(items))=(kind,items_state.as_ref()){
            let models=newviso_ui_client::ink_game_state::view_models_from_sources(
                Some(items),None,"player",
            );
            let menu=self.main_menu.as_mut().expect("menu checked");
            match ink_virtual_lists::compose_inventory_list(
                &mut source,&models,&mut menu.ink_preview_list,kind
            ){
                Ok(metrics)=>virtual_metrics=Some(metrics),
                Err(error)=>host::warn("newviso.ink",format!(
                    "source preview virtual list '{name}': {error}"
                )),
            }
        }
        {
            let menu=self.main_menu.as_mut().expect("menu checked");
            if let Some(playback)=menu.ink_preview_playback.as_mut() {
                playback.advance(dt,menu.hovered_action.as_deref());
                playback.apply(&mut source,menu.reduced_motion);
            }
        }
        if name=="save_game_menu" {
            let available=self.ready && self.world_save_allowed
                && self.world_persistence.is_some()
                && self.main_menu.as_ref().is_some_and(|menu|menu.started);
            let label=if available {
                self.main_menu.as_ref()
                    .and_then(|menu|menu.ink_preview_save_status.as_deref())
                    .unwrap_or("СОХРАНИТЬ ТЕКУЩИЙ МИР")
            }else{"СОХРАНЕНИЕ НЕДОСТУПНО"};
            if let Some(nodes)=source.get_mut("components").and_then(Value::as_array_mut) {
                nodes.push(json!({
                    "id":"ink.source.save.action","component_id":"button",
                    "text":label,"action_id":"newviso.ink.save.quick",
                    "props":{"position":"absolute","x_px":360,"y_px":675,
                        "w_px":630,"h_px":65,"font_px":26,"enabled":available,
                        "draw_panel":true,"fill_rgba":[23,36,44,230],
                        "border_rgba":[248,213,70,255],"border_px":2,
                        "font_ref":"font://cyberpunk/Raj (Rus)/Semi-Bold"}
                }));
            }
        }
        source["visible"]=Value::Bool(true);
        source["modal"]=Value::Bool(true);
        UiClient::new().publish_surface(&source)?;
        let menu=self.main_menu.as_mut().expect("menu checked");
        let was_published=menu.ink_preview_published;
        menu.ink_preview_published=true;
        menu.ink_preview_revision=revision;
        if let Some(metrics)=virtual_metrics {
            menu.ink_preview_visible_rows=metrics.rendered_rows.max(1);
            menu.ink_preview_total_rows=metrics.total_rows;
        }
        if !was_published {
            host::info("newviso.ink",format!(
                "source-preview-mounted name='{name}' nodes={} item_rows={} animation={}",
                source["components"].as_array().map_or(0,Vec::len),
                menu.ink_preview_visible_rows,menu.ink_preview_playback.is_some(),
            ));
        }
        Ok(())
    }

    fn hide_source_ink_preview(&mut self) -> Result<(), String> {
        let Some(menu)=self.main_menu.as_ref() else{return Ok(());};
        if !menu.ink_preview_published {return Ok(());}
        let Some(mut source)=menu.ink_preview_source.clone() else{return Ok(());};
        source["visible"]=Value::Bool(false);
        source["modal"]=Value::Bool(false);
        UiClient::new().publish_surface(&source)?;
        self.main_menu.as_mut().expect("menu checked").ink_preview_published=false;
        Ok(())
    }

    /// Provider actions from source Ink virtual lists operate on the current
    /// ItemsRuntime inventory, not on serialized user-supplied item IDs.
    fn apply_source_ink_preview_actions(&mut self,dispatch: &Value)->Result<(),String>{
        let Some(menu)=self.main_menu.as_ref() else{return Ok(());};
        let Some(name)=menu.ink_preview_name.clone() else{return Ok(());};
        let kind=InkListKind::from_screen(&name);
        let id=format!("newviso.ink.source.{name}");
        let Some(actions)=dispatch["actions"].as_array() else{return Ok(());};
        let action=actions.iter().filter(|entry|
            entry["surface_id"].as_str()==Some(id.as_str())
        ).filter_map(|entry|entry["action_id"].as_str())
          .find(|action|action.starts_with("newviso.ink.item.")
               || action.starts_with("newviso.ink.list.")
               || *action=="newviso.ink.save.quick")
          .map(str::to_owned);
        let Some(action)=action else{return Ok(());};
        if action=="newviso.ink.save.quick" {
            if name!="save_game_menu" {return Ok(());}
            let result=self.save_world_from_ink_ui();
            let ok=result.is_ok();
            match result {
                Ok(())=>host::info("newviso.ink","manual world checkpoint saved"),
                Err(error)=>host::warn("newviso.ink",
                    format!("manual Ink world save failed: {error}")),
            }
            host::publish_event_json("newviso.ui.ink.event","newviso.ui",
                json!({"schema":"newviso.ink.event.v1",
                    "kind":"save_result","screen":name,"ok":ok}))?;
            let menu=self.main_menu.as_mut().expect("checked");
            menu.ink_preview_save_status=Some(if ok {
                "СОХРАНЕНИЕ ЗАВЕРШЕНО".to_owned()
            }else{
                "ОШИБКА СОХРАНЕНИЯ".to_owned()
            });
            menu.ink_preview_published=false;
            return Ok(());
        }
        let Some(kind)=kind else{return Ok(());};
        if action=="newviso.ink.list.previous" || action=="newviso.ink.list.next" {
            let menu=self.main_menu.as_mut().expect("checked");
            let delta=if action.ends_with("previous"){-1}else{1};
            menu.ink_preview_list.scroll_pages(
                delta,menu.ink_preview_total_rows,menu.ink_preview_visible_rows,
            );
            menu.ink_preview_published=false;
            return Ok(());
        }
        if action.starts_with("newviso.ink.item.select.") {
            let items=self.items.runtime_state();
            let models=newviso_ui_client::ink_game_state::view_models_from_sources(
                Some(&items),None,"player",
            );
            match ink_virtual_lists::selected_item_from_action(&action,&models,kind) {
                Ok(Some(item))=>{
                    let menu=self.main_menu.as_mut().expect("checked");
                    menu.ink_preview_list.selected_item=Some(item.to_owned());
                    menu.ink_preview_published=false;
                }
                Ok(None)=>{},
                Err(error)=>host::warn("newviso.ink",format!(
                    "rejected virtual inventory selection: {error}"
                )),
            }
        }else if action=="newviso.ink.item.equip.hands" {
            let (started,selected)={
                let menu=self.main_menu.as_ref().expect("checked");
                (menu.started,menu.ink_preview_list.selected_item.clone())
            };
            if !started {
                host::warn("newviso.ink","inventory equip requires active gameplay");
                return Ok(());
            }
            let Some(selected)=selected else{return Ok(());};
            let items=self.items.runtime_state();
            let models=newviso_ui_client::ink_game_state::view_models_from_sources(
                Some(&items),None,"player",
            );
            let entries=models.pointer(&format!("/screens/{name}/entries"))
                .and_then(Value::as_array);
            let allowed=entries.is_some_and(|entries|entries.iter().any(|row|
                row["id"].as_str()==Some(selected.as_str())
                && row["category"].as_str()==Some("weapon")
            ));
            if !allowed {
                host::warn("newviso.ink","equip rejected: selected weapon no longer owned");
                return Ok(());
            }
            self.items.equip("player","hands",&selected)?;
            self.main_menu.as_mut().expect("checked").ink_preview_published=false;
            host::info("newviso.ink",format!("equipped player hands item='{selected}'"));
        }
        Ok(())
    }

    fn finish_pending_menu_action(&mut self, action: &str) -> Result<(), String> {
        match action {
            "newviso.menu.settings" => {
                if let Some(menu) = self.main_menu.as_mut() {
                    menu.settings_open = true;
                    if let Some(navigation) = menu.ink_navigation.as_mut() {
                        if let Err(error) = navigation.open("settings_main", InkOpenMode::Push) {
                            host::warn("newviso.ink", error);
                        }
                    }
                }
            }
            "newviso.menu.back" => {
                if let Some(menu) = self.main_menu.as_mut() {
                    menu.settings_open = false;
                    if let Some(navigation) = menu.ink_navigation.as_mut() {
                        navigation.back();
                    }
                }
            }
            action if action.starts_with("newviso.menu.category.") => {
                let category = &action["newviso.menu.category.".len()..];
                if application_main_menu_settings::is_category(category) {
                    if let Some(menu) = self.main_menu.as_mut() {
                        menu.category = category.to_owned();
                    }
                }
            }
            "newviso.menu.quit" => self.exit_requested = true,
            "newviso.menu.start" | "newviso.menu.resume" => {
                if let Some(menu) = self.main_menu.as_mut() {
                    menu.launch_requested = true;
                    menu.visible = false;
                }
                self.hide_source_ink_preview()?;
                if self.startup_map_ready {
                    self.start_project_scripts()?;
                }
                host::info("newviso.menu", "project main menu -> map loading / gameplay");
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn step_main_menu(
        &mut self,
        dt: f32,
        surface: PlatformSurfaceMetricsV1,
        input: InputSnapshot,
    ) -> Result<bool, String> {
        let Some(menu) = self.main_menu.as_ref() else { return Ok(false); };
        if !menu.visible { return Ok(false); }
        // Escape dismisses the pause menu, but never bypasses the first launch.
        if input.key_pressed(key::ESCAPE) {
            if menu.settings_open {
                { let menu=self.main_menu.as_mut().expect("menu checked");
                  menu.settings_open=false;
                  if let Some(router)=menu.ink_navigation.as_mut(){router.back();}
                }
                self.publish_bound_ui_if_changed()?;
            } else if menu.started {
                { let menu=self.main_menu.as_mut().expect("menu checked");
                  menu.visible=false;
                  if let Some(router)=menu.ink_navigation.as_mut(){router.back();}
                }
                self.hide_source_ink_preview()?;
                self.publish_bound_ui_if_changed()?;
                return Ok(false);
            }
        }
        self.initialize_source_ink_navigation();
        self.publish_source_ink_preview_if_requested(dt)?;
        self.cursor_captured = false;
        let size = [surface.width.max(1), surface.height.max(1)];
        let ui = UiClient::new();

        // Never re-publish an animation surface between dispatch_input and
        // frame. Egui treats that dispatch result as a one-shot draw packet
        // with texture_delta: invalidating it drops the initial GPU uploads
        // and results in a completely blank window.
        if let Some(menu) = self.main_menu.as_mut() {
            if let Some(ink) = menu.ink.as_mut() {
                ink.advance(dt, menu.hovered_action.as_deref());
            }
        }
        self.publish_bound_ui_if_changed()?;

        let dispatch = ui.dispatch_input(
            self.ui_frame_index,
            &input.ui_input_frame(),
            size,
            surface.pixels_per_point,
        )?;
        if let Some(menu) = self.main_menu.as_mut() {
            menu.hovered_action = dispatch.pointer("/hovered_node/action_id")
                .and_then(Value::as_str).map(str::to_owned);
        }
        if let Some(menu) = self.main_menu.as_mut() {
            if let Some(router) = menu.ink_navigation.as_mut() {
                if let Err(error) = router.dispatch(&dispatch, &menu.surface_id) {
                    host::warn("newviso.ink", format!("Ink dispatch: {error}"));
                }
                // Gameplay controllers subscribe to this neutral event topic.
                // Do not emit these as visual UI provider callbacks.
                for event in router.take_events().into_iter().take(128) {
                    host::publish_event_json(
                        "newviso.ui.ink.event", "newviso.ui", event.to_payload()
                    )?;
                }
            }
        }
        self.apply_source_ink_preview_actions(&dispatch)?;
        self.apply_menu_actions(&dispatch)?;
        self.apply_menu_value_patches(&dispatch)?;
        if let Some(action) = self.main_menu.as_mut()
            .and_then(|menu| menu.advance_transition(dt)) {
            self.finish_pending_menu_action(&action)?;
        }
        // A New Game action must display a map-loading frame immediately.
        // Previously the closing menu was already hidden, leaving a blank
        // engine frame before the next streaming tick.
        if self.main_menu.as_ref().is_some_and(|menu| {
            menu.launch_requested && !menu.started && !menu.visible
        }) {
            self.publish_bound_ui_if_changed()?;
            self.render_startup_loading_ui(dt, surface)?;
            return Ok(false);
        }
        if self.exit_requested {
            return Ok(true);
        }
        let frame = ui.frame(self.ui_frame_index, dt, size, surface.pixels_per_point)?;
        self.ui_frame_index = self.ui_frame_index.wrapping_add(1);
        self.scene.render_frame_with_overlay(surface.width, surface.height, move || {
            RenderClient::new().composite_ui_draw_list(
                frame.draw_list,
                newviso_render_client::UiLayerDomain::System,
            )
        })?;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.update(dt)?;
            renderer.render(dt)?;
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn menu_is_opt_in_with_a_surface_identity() {
        assert!(MainMenuState::from_template(Some(&json!({
            "surface_id": "test.start", "metrics": {"startup_menu": true}
        }))).is_some());
        assert!(MainMenuState::from_template(Some(&json!({
            "surface_id": "test.hud", "metrics": {}
        }))).is_none());
        assert!(MainMenuState::from_template(Some(&json!({
            "surface_id": "  ", "metrics": {"startup_menu": true}
        }))).is_none());
    }

    #[test]
    fn transitions_keep_click_feedback_visible_before_action() {
        let mut menu = MainMenuState::from_template(Some(&json!({
            "surface_id": "test.start",
            "metrics": {"startup_menu": true, "click_transition_ms": 220}
        }))).unwrap();
        menu.queue_action("newviso.menu.start");
        menu.queue_action("newviso.menu.quit"); // a second input must not replace the first
        assert!(menu.advance_transition(0.05).is_none());
        assert!(menu.advance_transition(0.15).is_none());
        assert_eq!(menu.advance_transition(0.03), Some("newviso.menu.start".into()));
        assert_eq!(menu.advance_transition(1.0), None);
    }

    #[test]
    fn menu_actions_are_project_data_and_filtered_by_lifecycle() {
        let template = json!({"components": [{
            "id": "menu", "children": [
                {"id": "start", "action_id": "newviso.menu.start"},
                {"id": "resume", "action_id": "newviso.menu.resume"},
                {"id": "quit", "action_id": "newviso.menu.quit"}
            ]
        }]});
        let mut front = template.clone();
        prepare_menu_surface(&mut front, false, false, false);
        assert_eq!(front["components"][0]["children"].as_array().unwrap().len(), 2);
        assert_eq!(front["components"][0]["children"][0]["id"], "start");
        assert_eq!(front["components"][0]["children"][1]["id"], "quit");

        let mut pause = template;
        prepare_menu_surface(&mut pause, true, false, false);
        assert_eq!(pause["components"][0]["children"].as_array().unwrap().len(), 2);
        assert_eq!(pause["components"][0]["children"][0]["id"], "resume");
        assert_eq!(pause["components"][0]["children"][1]["id"], "quit");
    }

    #[test]
    fn settings_route_filters_main_page_without_exposing_animation_controls() {
        let template = json!({
            "metrics": {"presentation_props": {"reduce_motion": true}},
            "components": [
                {"id":"logo"},
                {"id":"start","action_id":"newviso.menu.start","props":{"menu_page":"main"}},
                {"id":"settings","action_id":"newviso.menu.settings","props":{"menu_page":"main"}},
                {"id":"settings.heading","props":{"menu_page":"settings"}},
                {"id":"settings.control.graphics.shadow_resolution","component_id":"select",
                 "props":{"menu_page":"settings","menu_category":"graphics"}},
                {"id":"settings.back","action_id":"newviso.menu.back","props":{"menu_page":"settings"}}
            ]
        });
        let mut front = template.clone();
        prepare_menu_surface(&mut front, false, false, false);
        let ids: Vec<_> = front["components"].as_array().unwrap().iter()
            .map(|v| v["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["logo", "start", "settings"]);
        let mut options = template;
        prepare_menu_surface(&mut options, false, true, true);
        let ids: Vec<_> = options["components"].as_array().unwrap().iter()
            .map(|v| v["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["logo", "settings.heading",
                             "settings.control.graphics.shadow_resolution", "settings.back"]);
        assert_eq!(options["metrics"]["presentation_props"]["reduce_motion"], false);
    }
}
