//! Provider-neutral runtime map-loading UI. The bootstrap/Winit loading presenter
//! owns the process startup gate only; a New Game requested after renderer handoff
//! must use engine.ui (System domain), without claiming the HWND a second time.
use super::*;

const LOADING_SURFACE_ID: &str = "engine.runtime.map_loading";
const DESIGN_W: f32 = 1600.0;
const DESIGN_H: f32 = 900.0;

fn rect(id: &str, x: f32, y: f32, w: f32, h: f32, rgba: [u8; 4]) -> Value {
    json!({
        "id": id, "component_id": "panel",
        "props": {
            "position": "absolute", "x_px": x, "y_px": y, "w_px": w,
            "h_px": h, "draw_panel": true, "fill_rgba": rgba,
            "border_px": 0, "padding_px": 0
        }
    })
}

fn label(id: &str, value: &str, x: f32, y: f32, w: f32, h: f32, font: f32, rgba: [u8; 4]) -> Value {
    json!({
        "id": id, "component_id": "text", "text": value,
        "props": {
            "position": "absolute", "x_px": x, "y_px": y, "w_px": w, "h_px": h,
            "font_px": font, "text_rgba": rgba
        }
    })
}

/// Consume actual counters from the normal startup residency path; never
/// substitute time-based progress percentages for unknown totals.
fn asset_categories(source: &str) -> Vec<(String, String, usize, usize)> {
    serde_json::from_str::<Value>(source)
        .ok()
        .and_then(|value| value.get("asset_progress_bars")?.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| Some((
            item.get("id")?.as_str()?.to_owned(),
            item.get("label")?.as_str()?.to_owned(),
            usize::try_from(item.get("completed")?.as_u64()?).ok()?,
            usize::try_from(item.get("total")?.as_u64()?).ok()?,
        )))
        .take(4)
        .collect()
}

fn map_loading_ui_surface(
    progress: f32,
    status: &str,
    asset_json: &str,
    visible: bool,
) -> Value {
    let mut components = vec![
        rect("loading.background", 0.0, 0.0, DESIGN_W, DESIGN_H, [9, 14, 21, 255]),
        rect("loading.corner", 0.0, 0.0, 14.0, DESIGN_H, [245, 211, 40, 255]),
        rect("loading.separator", 105.0, 248.0, 1390.0, 2.0, [170, 46, 62, 255]),
        label("loading.engine", "NIGHT CITY // 2077", 105.0, 153.0, 1100.0, 65.0, 55.0, [245, 211, 40, 255]),
        label("loading.heading", "ЗАГРУЗКА МИРА", 105.0, 343.0, 1150.0, 72.0, 48.0, [242, 237, 220, 255]),
        label("loading.status", status, 105.0, 438.0, 1370.0, 48.0, 24.0, [214, 221, 221, 255]),
        rect("loading.track", 105.0, 543.0, 1390.0, 13.0, [49, 58, 65, 255]),
        rect("loading.top.bar", 105.0, 543.0, 1390.0 * progress.clamp(0.0, 1.0), 13.0, [245, 211, 40, 255]),
        label("loading.percent", &format!("{:.0}%", progress.clamp(0.0, 1.0) * 100.0), 1350.0, 490.0, 150.0, 45.0, 28.0, [245, 211, 40, 255]),
        rect("loading.bottom.separator", 105.0, 816.0, 1390.0, 2.0, [159, 47, 62, 255]),
        label("loading.engine.bottom", "NEWVISO  //  ASSET STREAMING", 105.0, 831.0, 800.0, 42.0, 18.0, [152, 172, 181, 255]),
    ];

    for (index, (id, title, completed, total)) in asset_categories(asset_json).iter().enumerate() {
        let x = 105.0 + index as f32 * 353.0;
        let y = 637.0;
        let width = 312.0;
        let fill = if *total > 0 {
            (*completed).min(*total) as f32 / *total as f32
        } else {
            0.0
        };
        components.push(label(&format!("loading.{id}.label"), title, x, y, width, 40.0, 19.0, [223, 226, 218, 255]));
        components.push(rect(&format!("loading.{id}.track"), x, y + 51.0, width, 6.0, [54, 65, 71, 255]));
        if fill > 0.0 {
            components.push(rect(&format!("loading.{id}.fill"), x, y + 51.0, width * fill, 6.0, [241, 203, 47, 255]));
        }
        components.push(label(
            &format!("loading.{id}.count"),
            if *total == 0 {
                "ОЖИДАНИЕ".to_owned()
            } else {
                format!("{}/{}", completed.min(total), total)
            }.as_str(),
            x, y + 69.0, width, 34.0, 17.0, [163, 173, 178, 255],
        ));
    }

    json!({
        "version": 1, "surface_id": LOADING_SURFACE_ID,
        "source": "newviso-runtime", "visible": visible, "modal": true,
        "z_order": 6000, "title": "", "subtitle": "",
        "style_tags": ["fullscreen", "responsive-cover", "surface-chrome:none"],
        "metrics": {"chrome": false, "runtime_loading": true},
        "style": {
            "theme_id": "newviso-loading", "min_size_px": [DESIGN_W, DESIGN_H],
            "max_size_px": [DESIGN_W, DESIGN_H], "margin_px": [0, 0],
            "padding_px": [0, 0, 0, 0], "border_px": 0,
            "backdrop_rgba": [0, 0, 0, 0],
            "panel_rgba": [9, 14, 21, 255], "accent_rgba": [245, 211, 40, 255],
            "text_rgba": [235, 235, 220, 255]
        },
        "components": components
    })
}


/// Project-owned loading screens have the same provider-neutral surface
/// contract as menus. Only telemetry is engine-authored. Node ids define the
/// binding points, so game-specific branding never lives in the engine.
fn materialize_authored_loading_surface(
    template: &Value,
    progress: f32,
    status: &str,
    asset_json: &str,
    visible: bool,
) -> Result<Value, String> {
    let mut surface = template.clone();
    if surface.get("surface_id").and_then(Value::as_str).is_none_or(str::is_empty) {
        return Err("authored loading surface requires a surface_id".into());
    }
    // A project skin replaces the engine loading screen in-place. Publishing
    // a separate project surface_id makes the UI provider retain two fullscreen
    // surfaces, which may alternate on consecutive frames during the override.
    // Keep one canonical slot for the whole loading lifecycle.
    surface["surface_id"] = json!(LOADING_SURFACE_ID);
    surface["visible"] = Value::Bool(visible);
    let components = surface.get_mut("components").and_then(Value::as_array_mut)
        .ok_or("authored loading surface requires components array")?;
    let get_width = |id: &str| -> f64 {
        components.iter()
            .find(|node| node.get("id").and_then(Value::as_str) == Some(id))
            .and_then(|node| node.pointer("/props/w_px").and_then(Value::as_f64))
            .unwrap_or(0.0)
    };
    let width = get_width("loading.progress.track");
    let groups: Vec<_> = asset_categories(asset_json);
    let mut category_widths = std::collections::BTreeMap::new();
    for (id, _, _, _) in &groups {
        category_widths.insert(id.clone(), get_width(&format!("loading.categories.{id}.track")));
    }

    for node in components {
        let id = node.get("id").and_then(Value::as_str).unwrap_or("").to_owned();
        match id.as_str() {
            "loading.progress.fill" => {
                node["props"]["w_px"] = json!((width * f64::from(progress.clamp(0.0, 1.0))).max(1.0));
            }
            "loading.progress.percent" => {
                node["text"] = json!(format!("{:.0}%", progress.clamp(0.0, 1.0) * 100.0));
            }
            "loading.status" => node["text"] = json!(status),
            _ => {
                for (key, title, completed, total) in &groups {
                    if id == format!("loading.categories.{key}.label") {
                        node["text"] = json!(title);
                    } else if id == format!("loading.categories.{key}.count") {
                        node["text"] = json!(if *total == 0 {
                            "WAIT".to_owned()
                        } else {
                            format!("{}/{}", completed.min(total), total)
                        });
                    } else if id == format!("loading.categories.{key}.fill") {
                        let ratio = if *total > 0 {
                            *completed.min(total) as f64 / *total as f64
                        } else {
                            0.0
                        };
                        node["props"]["w_px"] = json!((category_widths.get(key).copied().unwrap_or(0.0) * ratio).max(1.0));
                    }
                }
            }
        }
    }
    Ok(surface)
}

impl EngineApplication {
    fn loading_surface_payload(&mut self, visible: bool) -> Result<Value, String> {
        if self.loading_ui_template.is_none() {
            let requested = self.ui_template.as_ref()
                .and_then(|menu| menu.pointer("/metrics/loading_surface"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            if let Some(path) = requested {
                let loaded = AssetClient::new().json(&path)
                    .map_err(|error| format!("project loading surface '{path}' load failed: {error}"))?;
                if loaded.get("version").and_then(Value::as_u64) != Some(1)
                    || loaded.get("surface_id").and_then(Value::as_str)
                        .is_none_or(str::is_empty)
                    || loaded.get("components").and_then(Value::as_array).is_none()
                {
                    return Err(format!("invalid project loading UI surface '{path}'"));
                }
                host::info("newviso.ui", format!("using project-authored loading surface '{path}'"));
                self.loading_ui_template = Some(loaded);
            }
        }

        match &self.loading_ui_template {
            Some(template) => materialize_authored_loading_surface(
                template, self.startup_map_progress, &self.startup_map_status,
                &self.startup_map_asset_progress_json, visible,
            ),
            None => Ok(map_loading_ui_surface(
                self.startup_map_progress, &self.startup_map_status,
                &self.startup_map_asset_progress_json, visible,
            )),
        }
    }
}

impl EngineApplication {
    pub(super) fn render_startup_loading_ui(
        &mut self,
        dt: f32,
        surface: PlatformSurfaceMetricsV1,
    ) -> Result<(), String> {
        let payload = self.loading_surface_payload(true)?;
        let ui = UiClient::new();
        ui.publish_surface(&payload)
            .map_err(|e| format!("runtime loading UI publish failed: {e}"))?;
        if std::env::var("NEWVISO_LOADING_QA_TRACE").as_deref() == Ok("1") {
            host::info("newviso.loading.qa", format!(
                "event=present frame={} surface_id={} authored={} progress={:.4}",
                self.ui_frame_index,
                payload["surface_id"].as_str().unwrap_or("missing"),
                self.loading_ui_template.is_some(), self.startup_map_progress,
            ));
        }
        self.loading_ui_visible = true;
        let pixels = [surface.width.max(1), surface.height.max(1)];
        let frame = ui.frame(self.ui_frame_index, dt, pixels, surface.pixels_per_point)
            .map_err(|e| format!("runtime loading UI frame failed: {e}"))?;
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
        Ok(())
    }

    pub(super) fn hide_startup_loading_ui(&mut self) -> Result<(), String> {
        if !self.loading_ui_visible {
            return Ok(());
        }
        let hidden = self.loading_surface_payload(false)?;
        UiClient::new().publish_surface(&hidden)
            .map_err(|e| format!("runtime loading UI hide failed: {e}"))?;
        if std::env::var("NEWVISO_LOADING_QA_TRACE").as_deref() == Ok("1") {
            host::info("newviso.loading.qa", format!(
                "event=hide frame={} surface_id={}",
                self.ui_frame_index, hidden["surface_id"].as_str().unwrap_or("missing"),
            ));
        }
        self.loading_ui_visible = false;
        Ok(())
    }
}


#[cfg(test)]
mod authored_loading_tests {
    use super::*;

    #[test]
    fn override_replaces_loading_surface_in_place_on_every_frame() {
        let authored_a = json!({"surface_id":"project.loading.one","components":[
            {"id":"loading.status","text":"A"}
        ]});
        let authored_b = json!({"surface_id":"project.loading.two","components":[
            {"id":"loading.status","text":"B"}
        ]});
        let default = map_loading_ui_surface(0.1, "Loading", "", true);
        let mut owner_ids = std::collections::BTreeSet::new();
        owner_ids.insert(default["surface_id"].as_str().unwrap().to_owned());
        for frame in 0..120 {
            let selected = if frame < 60 { &authored_a } else { &authored_b };
            let result = materialize_authored_loading_surface(
                selected, frame as f32 / 120.0, "Preparing assets", "", true
            ).unwrap();
            owner_ids.insert(result["surface_id"].as_str().unwrap().to_owned());
            assert_eq!(result["visible"], true);
            assert_eq!(result["components"][0]["text"], "Preparing assets");
        }
        // Even if project authors use different IDs, there is never a second
        // concurrently registered fullscreen loading-surface identity.
        assert_eq!(owner_ids.len(), 1);
        assert!(owner_ids.contains(LOADING_SURFACE_ID));
        assert_eq!(authored_a["surface_id"], "project.loading.one");
        assert_eq!(authored_b["surface_id"], "project.loading.two");
        let hidden = materialize_authored_loading_surface(
            &authored_b, 1.0, "Done", "", false
        ).unwrap();
        assert_eq!(hidden["surface_id"], LOADING_SURFACE_ID);
        assert_eq!(hidden["visible"], false);
    }

    #[test]
    fn authored_loading_uses_exact_engine_progress_and_project_art() {
        let authored = json!({
            "version": 1, "surface_id": "project.loading", "visible": true,
            "components": [
                {"id":"loading.progress.track","props":{"w_px":800}},
                {"id":"loading.progress.fill","props":{"w_px":1}},
                {"id":"loading.progress.percent","text":"0%"},
                {"id":"loading.status","text":""},
                {"id":"loading.categories.models.track","props":{"w_px":250}},
                {"id":"loading.categories.models.fill","props":{"w_px":1}},
                {"id":"loading.categories.models.count","text":"WAIT"},
                {"id":"project.art","component_id":"panel","props":{"fill_rgba":[1,2,3,255]}}
            ]
        });
        let telemetry = r#"{"asset_progress_bars":[{"id":"models","label":"SCENE MODELS","completed":4,"total":10}]}"#;
        let value = materialize_authored_loading_surface(&authored, 0.375, "Loading...", telemetry, true).unwrap();
        let components = value["components"].as_array().unwrap();
        let find = |id: &str| components.iter().find(|node| node["id"] == id).unwrap();
        assert_eq!(find("loading.progress.fill")["props"]["w_px"], json!(300.0));
        assert_eq!(find("loading.progress.percent")["text"], "38%");
        assert_eq!(find("loading.status")["text"], "Loading...");
        assert_eq!(find("loading.categories.models.fill")["props"]["w_px"], json!(100.0));
        assert_eq!(find("loading.categories.models.count")["text"], "4/10");
        assert_eq!(find("project.art")["props"]["fill_rgba"], json!([1,2,3,255]));
        assert_eq!(authored["components"][1]["props"]["w_px"], 1);
    }

    #[test]
    fn loading_can_reopen_repeatedly_without_restoring_default_surface() {
        let authored = json!({"surface_id":"user.loading.fullscreen","components":[
            {"id":"loading.status","text":""},
            {"id":"loading.progress.track","props":{"w_px":900}},
            {"id":"loading.progress.fill","props":{"w_px":1}}
        ]});
        let mut registered = std::collections::BTreeMap::<String, bool>::new();
        for cycle in 0..4 {
            for frame in 0..90 {
                let payload = materialize_authored_loading_surface(
                    &authored, frame as f32 / 89.0, "Loading assets", "", true
                ).unwrap();
                assert_eq!(payload["surface_id"], LOADING_SURFACE_ID);
                registered.insert(
                    payload["surface_id"].as_str().unwrap().to_owned(), true
                );
                assert_eq!(registered.len(), 1, "loading surface duplicated in cycle {cycle}");
            }
            let hidden = materialize_authored_loading_surface(
                &authored, 1.0, "Done", "", false
            ).unwrap();
            assert_eq!(hidden["surface_id"], LOADING_SURFACE_ID);
            registered.insert(hidden["surface_id"].as_str().unwrap().to_owned(), false);
            assert_eq!(registered.len(), 1);
            assert_eq!(registered[LOADING_SURFACE_ID], false);
        }
    }

    #[test]
    fn authored_loading_clamps_overrun_and_hides_by_identity() {
        let authored = json!({"surface_id":"project.loading","visible":true,"components":[
            {"id":"loading.progress.track","props":{"w_px":900}},
            {"id":"loading.progress.fill","props":{"w_px":1}},
            {"id":"loading.progress.percent","text":"0%"},
            {"id":"loading.categories.models.track","props":{"w_px":180}},
            {"id":"loading.categories.models.fill","props":{"w_px":1}},
            {"id":"loading.categories.models.count","text":"WAIT"}
        ]});
        let telemetry = r#"{"asset_progress_bars":[{"id":"models","label":"SCENE MODELS","completed":999,"total":0}]}"#;
        let value = materialize_authored_loading_surface(&authored, 9.0, "Done", telemetry, false).unwrap();
        assert_eq!(value["surface_id"], LOADING_SURFACE_ID);
        assert_eq!(value["visible"], false);
        assert_eq!(value["components"][1]["props"]["w_px"], json!(900.0));
        assert_eq!(value["components"][2]["text"], "100%");
        assert_eq!(value["components"][4]["props"]["w_px"], json!(1.0));
        assert_eq!(value["components"][5]["text"], "WAIT");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loading_surface_is_fullscreen_and_has_real_progress() {
        let s = map_loading_ui_surface(0.25, "Loading models", r#"{"asset_progress_bars":[{"id":"models","label":"MODELS","completed":4,"total":16}]}"#, true);
        assert_eq!(s["surface_id"], LOADING_SURFACE_ID);
        assert!(s["style_tags"].as_array().unwrap().iter().any(|s| s == "responsive-cover"));
        assert_eq!(s["style"]["max_size_px"], json!([1600.0, 900.0]));
        let nodes = s["components"].as_array().unwrap();
        assert_eq!(nodes.iter().find(|n| n["id"] == "loading.top.bar").unwrap()["props"]["w_px"], json!(347.5));
        assert_eq!(nodes.iter().find(|n| n["id"] == "loading.models.fill").unwrap()["props"]["w_px"], json!(78.0));
    }

    #[test]
    fn unknown_asset_total_has_no_fake_progress() {
        let entries = asset_categories(r#"{"asset_progress_bars":[{"id":"deps","label":"DEPS","completed":0,"total":0}]}"#);
        assert_eq!(entries[0].3, 0);
        let s = map_loading_ui_surface(0.0, "Waiting", "", false);
        assert_eq!(s["visible"], false);
        assert_eq!(s["components"][7]["id"], "loading.top.bar");
    }
}
