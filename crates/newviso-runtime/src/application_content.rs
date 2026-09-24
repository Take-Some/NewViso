use super::*;

impl EngineApplication {
    pub(super) fn publish_content_manager_if_changed(&mut self) -> Result<(), String> {
        let Some(manager) = self.content_manager.as_ref() else {
            return Ok(());
        };

        let surfaces = manager.surfaces();
        if self.last_content_surfaces == surfaces {
            return Ok(());
        }

        let ui = UiClient::new();
        for surface in &surfaces {
            ui.publish_surface(surface)
                .map_err(|error| format!("content manager UI publish failed: {error}"))?;
        }

        host::debug(
            "newviso.ui",
            format!(
                "published content-manager surfaces count={}",
                surfaces.len()
            ),
        );
        self.last_content_surfaces = surfaces;
        Ok(())
    }
    pub(super) fn apply_content_dispatch(&mut self, dispatch: &Value) -> Result<(), String> {
        let effects = match self.content_manager.as_mut() {
            Some(manager) => manager.apply_dispatch(dispatch)?,
            None => Vec::new(),
        };

        for effect in effects {
            match effect {
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
