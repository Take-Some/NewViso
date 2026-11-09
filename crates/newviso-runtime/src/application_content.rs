use super::*;

impl EngineApplication {
    pub(super) fn publish_content_manager_if_changed(&mut self) -> Result<(), String> {
        let Some(manager) = self.content_manager.as_ref() else {
            return Ok(());
        };

        let revision = manager.surface_revision();
        if self.last_content_surface_revision == Some(revision) {
            return Ok(());
        }
        let surfaces = manager.surfaces();
        if self.last_content_surfaces == surfaces {
            self.last_content_surface_revision = Some(revision);
            return Ok(());
        }

        let ui = UiClient::new();
        let mut published = 0;
        for surface in &surfaces {
            if self
                .last_content_surfaces
                .iter()
                .find(|old| old["surface_id"] == surface["surface_id"])
                == Some(surface)
            {
                continue;
            }
            published += 1;
            ui.publish_surface(surface)
                .map_err(|error| format!("content manager UI publish failed: {error}"))?;
        }

        host::debug(
            "newviso.ui",
            format!("published content-manager surfaces count={}", published),
        );
        self.last_content_surfaces = surfaces;
        self.last_content_surface_revision = Some(revision);
        Ok(())
    }
    pub(super) fn apply_content_dispatch(&mut self, dispatch: &Value) -> Result<(), String> {
        let effects = match self.content_manager.as_mut() {
            Some(manager) => match manager.apply_dispatch(dispatch) {
                Ok(effects) => effects,
                Err(error) => {
                    host::warn("newviso.editor", &error);
                    manager.report_status(error);
                    Vec::new()
                }
            },
            None => Vec::new(),
        };

        for effect in effects {
            match effect {
                ContentEffect::RuntimeSettingsApply { patch, expected } => {
                    let result = (|| {
                        let current =
                            serde_json::to_value(&self.settings).map_err(|e| e.to_string())?;
                        for (path, value) in &expected {
                            if current.pointer(path) != Some(value) {
                                return Err(format!("Live setting {path} changed since the last Apply; restart/reopen before replacing it"));
                            }
                        }
                        if self.editor_mode
                            && patch.pointer("/scheduling/native_navigation_enabled")
                                == Some(&Value::Bool(false))
                        {
                            return Err("Editor navigation stays enabled; save this game setting and restart the game to apply".into());
                        }
                        self.configure_runtime(&patch)
                    })();
                    match result {
                        Ok(()) => {
                            if let Some(manager) = self.content_manager.as_mut() {
                                manager.settings_applied();
                                manager.report_status("Live project settings applied. Restart fields remain pending; Save Settings persists edits.");
                            }
                            host::info(
                                "newviso.editor",
                                format!("project settings applied live fields={}", expected.len()),
                            );
                        }
                        Err(error) => {
                            host::warn("newviso.editor", &error);
                            if let Some(manager) = self.content_manager.as_mut() {
                                manager.report_status(error);
                            }
                        }
                    }
                }
                ContentEffect::ScriptChanged {
                    logical_path,
                    bytes: _,
                } => {
                    let reload = self
                        .scripts
                        .as_mut()
                        .ok_or_else(|| "no active scripting runtime".to_owned())
                        .and_then(|scripts| scripts.reload_graph_after_asset_change(&logical_path));

                    match reload {
                        Ok(()) => {
                            host::info(
                                "newviso.content",
                                format!("saved and hot-reloaded script '{logical_path}'"),
                            );
                            if let Some(manager) = self.content_manager.as_mut() {
                                manager
                                    .report_status(format!("Saved + hot reloaded {logical_path}"));
                            }
                        }
                        Err(error) => {
                            host::warn(
                                "newviso.content",
                                format!(
                                    "saved script '{logical_path}' but hot reload failed: {error}"
                                ),
                            );
                            if let Some(manager) = self.content_manager.as_mut() {
                                manager.report_status(format!("Saved, reload failed: {error}"));
                            }
                        }
                    }
                }
            }
        }

        self.publish_content_manager_if_changed()
    }
    pub(super) fn publish_bound_ui_if_changed(&mut self) -> Result<(), String> {
        let Some(template) = self.ui_template.as_ref() else {
            return Ok(());
        };

        let materialized = materialize_ui_template(template, &self.ui_bindings);
        if self.last_ui_surface.as_ref() == Some(&materialized) {
            return Ok(());
        }

        UiClient::new()
            .publish_surface(&materialized)
            .map_err(|error| format!("project UI surface update failed: {error}"))?;
        self.last_ui_surface = Some(materialized);
        Ok(())
    }
}
