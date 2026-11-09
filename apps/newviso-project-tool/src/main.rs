mod assets;
mod package;
mod scene;
use newviso_config::{parse_engine_config_xml, ResolvedBootstrapConfig};
use newviso_project::{
    ProjectRuntimeSettings, ResolvedProject, ResolvedRuntimeConfiguration,
    RuntimeConfigurationRequest,
};
use serde_json::{json, Value};
use std::{io::Write, process::ExitCode};

fn execute() -> Result<Value, String> {
    let mut raw = std::env::args_os().skip(1);
    let command=raw.next().ok_or("usage: newviso-project-tool <resolve|validate|profiles|schema|types|scene-inspect|scene-edit|scene-validate|package-plan|package-build|package-verify>")?.to_string_lossy().into_owned();
    if command == "schema" {
        return Ok(ProjectRuntimeSettings::schema().clone());
    }
    if command == "types" {
        return serde_json::to_value(newviso_scene_document::SceneTypeRegistry::builtin())
            .map_err(|e| e.to_string());
    }
    let mut options = std::collections::BTreeMap::new();
    let mut args = Vec::new();
    while let Some(arg) = raw.next() {
        let s = arg.to_string_lossy();
        if [
            "--scene",
            "--edits",
            "--entity",
            "--output",
            "--package-dir",
            "--engine",
        ]
        .contains(&s.as_ref())
        {
            let value = raw.next().ok_or_else(|| format!("{s} requires a value"))?;
            if options.insert(s.into_owned(), value).is_some() {
                return Err("duplicate tool option".into());
            }
        } else {
            args.push(arg);
        }
    }
    if command == "package-verify" {
        return package::verify(std::path::Path::new(
            options
                .get("--package-dir")
                .ok_or("--package-dir is required")?,
        ));
    }
    if !matches!(
        command.as_str(),
        "resolve"
            | "validate"
            | "profiles"
            | "scene-inspect"
            | "scene-edit"
            | "scene-validate"
            | "package-plan"
            | "package-build"
    ) {
        return Err(format!("unknown project-tool command '{command}'"));
    }
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let bootstrap = ResolvedBootstrapConfig::load_for_executable_and_args(executable, args)
        .map_err(|e| e.to_string())?;
    let project = ResolvedProject::load(
        bootstrap
            .project_path
            .as_ref()
            .ok_or("--project is required")?,
    )
    .map_err(|e| e.to_string())?;
    if command == "profiles" {
        return Ok(
            json!({"project":project.manifest.project,"configuration":project.manifest.configuration}),
        );
    }
    let shared_path = bootstrap
        .assets_dir
        .join("config/engine/runtime.defaults.xml");
    let shared = parse_engine_config_xml(
        &std::fs::read(&shared_path).map_err(|e| format!("{}: {e}", shared_path.display()))?,
        &shared_path,
    )?;
    let request = RuntimeConfigurationRequest {
        platform: std::env::consts::OS.to_owned(),
        profile: bootstrap.runtime_profile.clone(),
        layers: bootstrap.runtime_layers.clone(),
        overrides: bootstrap.runtime_overrides.clone(),
    };
    let mut assets = assets::DirectoryAssets::new(&project, &bootstrap.assets_dir)?;
    if command.starts_with("scene-") {
        return scene::execute(
            &command,
            &project,
            &mut assets,
            options.get("--scene").and_then(|v| v.to_str()),
            options.get("--edits").map(std::path::Path::new),
            options
                .get("--entity")
                .map(|v| v.to_string_lossy().parse::<usize>())
                .transpose()
                .map_err(|e| e.to_string())?
                .unwrap_or(0),
        );
    }
    let resolved = ResolvedRuntimeConfiguration::resolve(
        &project.manifest.configuration,
        &project.manifest.files.runtime,
        shared,
        "config/engine/runtime.defaults.xml",
        &request,
        |p| assets.json(p),
    )
    .map_err(|e| e.to_string())?;
    if command.starts_with("package-") {
        let scene = assets.json(&project.manifest.files.scene)?;
        newviso_scene_document::validate_scene(
            &scene,
            newviso_scene_document::SceneTypeRegistry::builtin(),
        )?;
        let default_engine = bootstrap.executable_dir.join("newviso.exe");
        let engine = options
            .get("--engine")
            .map(std::path::Path::new)
            .unwrap_or(&default_engine);
        let plan = package::PackagePlan::new(
            &project,
            &bootstrap,
            &resolved,
            engine,
            assets
                .bindings
                .get(&project.manifest.files.runtime)
                .ok_or("runtime settings source not resolved")?,
        )?;
        if command == "package-plan" {
            return Ok(plan.summary());
        }
        return plan.build(std::path::Path::new(
            options.get("--output").ok_or("--output is required")?,
        ));
    }
    if command == "validate" {
        return Ok(
            json!({"passed":true,"validated":"manifest_and_runtime_configuration", "project":project.manifest.project,"resolution":resolved.report}),
        );
    }
    Ok(
        json!({"project":project.manifest.project,"settings":resolved.settings,
        "resolution":resolved.report,"physical_sources":assets.bindings}),
    )
}

fn main() -> ExitCode {
    match execute() {
        Ok(value) => match serde_json::to_vec_pretty(&value) {
            Ok(mut bytes) => {
                bytes.push(b'\n');
                if std::io::stdout().lock().write_all(&bytes).is_ok() {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(1)
                }
            }
            Err(error) => {
                let _ = writeln!(std::io::stderr().lock(), "{error}");
                ExitCode::from(1)
            }
        },
        Err(error) => {
            let _ = writeln!(std::io::stderr().lock(), "project tool failed: {error}");
            ExitCode::from(2)
        }
    }
}
