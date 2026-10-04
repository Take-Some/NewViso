use super::*;

/// Engine libraries always run first. Project hooks run later and can override
/// their commands, or replace a shared logical script through the project VFS.
pub(super) fn compose_engine_scripts(
    mut shared: ProjectScripts,
    project: Option<ProjectScripts>,
) -> Result<ProjectScripts, String> {
    if !shared.enabled || shared.modules.is_empty() {
        return Err("Shared engine scripts must be enabled and contain a base lifecycle".into());
    }
    if let Some(project) = project.filter(|scripts| scripts.enabled && !scripts.modules.is_empty())
    {
        if project.provider != shared.provider {
            return Err(format!(
                "project scripting provider '{}' cannot compose with Shared provider '{}'",
                project.provider, shared.provider
            ));
        }
        for module in project.modules {
            if !shared.modules.iter().any(|base| base.asset == module.asset) {
                shared.modules.push(module);
            }
        }
    }
    Ok(shared)
}

pub(super) fn default_player_requires_physics(settings: &ProjectRuntimeSettings) -> bool {
    let player = settings.variables.get("engine_player");
    player
        .and_then(|v| v.get("enabled"))
        .and_then(Value::as_bool)
        != Some(false)
        && player
            .and_then(|v| v.get("controller"))
            .and_then(Value::as_str)
            != Some("project")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scripts(asset: &str, enabled: bool) -> ProjectScripts {
        ProjectScripts::from_value(json!({
            "schema": "newviso.scripts.v1",
            "provider": "engine.scripting.typescript",
            "enabled": enabled,
            "modules": [{
                "asset": asset,
                "on_start": "on_start",
                "on_frame": "on_frame",
                "on_shutdown": "on_shutdown"
            }]
        }))
        .unwrap()
    }

    #[test]
    fn project_without_scripts_still_loads_shared_lifecycle() {
        let result =
            compose_engine_scripts(scripts("scripts/newviso/default_game.ysc", true), None)
                .unwrap();
        assert!(result.enabled);
        assert_eq!(result.modules.len(), 1);
    }

    #[test]
    fn shared_hooks_precede_project_overrides() {
        let result = compose_engine_scripts(
            scripts("scripts/newviso/default_game.ysc", true),
            Some(scripts("scripts/main.ysc", true)),
        )
        .unwrap();
        assert_eq!(result.modules.len(), 2);
        assert_eq!(result.modules[0].asset, "scripts/newviso/default_game.ysc");
        assert_eq!(result.modules[1].asset, "scripts/main.ysc");
    }

    #[test]
    fn disabling_project_scripts_keeps_engine_basics() {
        let result = compose_engine_scripts(
            scripts("scripts/newviso/default_game.ysc", true),
            Some(scripts("scripts/main.ysc", false)),
        )
        .unwrap();
        assert_eq!(result.modules.len(), 1);
    }

    #[test]
    fn an_overlaid_shared_root_is_not_invoked_twice() {
        let result = compose_engine_scripts(
            scripts("scripts/newviso/default_game.ysc", true),
            Some(scripts("scripts/newviso/default_game.ysc", true)),
        )
        .unwrap();
        assert_eq!(result.modules.len(), 1);
    }

    #[test]
    fn mismatched_script_providers_fail_explicitly() {
        let mut project = scripts("scripts/main.ysc", true);
        project.provider = "engine.scripting.other".into();
        assert!(compose_engine_scripts(
            scripts("scripts/newviso/default_game.ysc", true),
            Some(project)
        )
        .is_err());
    }
}
