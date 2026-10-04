use super::engine_scripts::{compose_engine_scripts, default_player_requires_physics};
use super::*;

const SHARED_RUNTIME_DEFAULTS: &str = "config/engine/runtime.defaults.xml";
const SHARED_ENVIRONMENT_DEFAULTS: &str = "config/engine/environment.defaults.xml";
const SHARED_RENDER_DEFAULTS: &str = "config/engine/render.defaults.xml";
const SHARED_VFS_MOUNTS: &str = "config/engine/vfs_mounts.xml";
const SHARED_SCRIPTS_DEFAULTS: &str = "config/engine/scripts.defaults.xml";

#[derive(Debug)]
struct EngineConfigXmlNode {
    name: String,
    attrs: BTreeMap<String, String>,
    children: Vec<EngineConfigXmlNode>,
}

fn load_shared_engine_xml(
    shared_assets_dir: &Path,
    relative_path: &str,
    label: &str,
) -> Result<Value, String> {
    let path = shared_assets_dir.join(relative_path);
    let bytes = fs::read(&path).map_err(|error| {
        format!(
            "Shared Assets {label} defaults are required at '{}': {error}",
            path.display()
        )
    })?;
    parse_engine_config_xml(&bytes, &path)
        .map_err(|error| format!("Shared Assets {label} defaults XML is invalid: {error}"))
}

pub(super) fn parse_engine_config_xml(bytes: &[u8], path: &Path) -> Result<Value, String> {
    use quick_xml::{events::Event, Reader, XmlVersion};

    fn attrs(
        event: &quick_xml::events::BytesStart<'_>,
    ) -> Result<BTreeMap<String, String>, String> {
        let mut out = BTreeMap::new();
        for attribute in event.attributes().with_checks(false) {
            let attribute = attribute.map_err(|error| format!("XML attribute error: {error}"))?;
            let key = attribute.key.into_inner().to_owned();
            let value = attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .map_err(|error| format!("XML attribute decode failed: {error}"))?
                .into_owned();
            out.insert(key, value);
        }
        Ok(out)
    }

    fn scalar(raw: &str) -> Value {
        let raw = raw.trim();
        if raw.eq_ignore_ascii_case("true") {
            return Value::Bool(true);
        }
        if raw.eq_ignore_ascii_case("false") {
            return Value::Bool(false);
        }
        if let Ok(value) = raw.parse::<i64>() {
            return Value::Number(value.into());
        }
        if let Ok(value) = raw.parse::<u64>() {
            return Value::Number(value.into());
        }
        if let Ok(value) = raw.parse::<f64>() {
            if let Some(value) = serde_json::Number::from_f64(value) {
                return Value::Number(value);
            }
        }
        Value::String(raw.to_owned())
    }

    fn node_value(node: EngineConfigXmlNode) -> Result<Value, String> {
        let kind = node.attrs.get("type").map(String::as_str);
        if kind == Some("array") {
            return node
                .children
                .into_iter()
                .map(node_value)
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array);
        }
        if kind == Some("object") && node.children.is_empty() {
            return Ok(Value::Object(serde_json::Map::new()));
        }
        if node
            .attrs
            .get("null")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
        {
            return Ok(Value::Null);
        }

        let axes = ["x", "y", "z", "w"]
            .into_iter()
            .filter_map(|axis| node.attrs.get(axis).map(|value| scalar(value)))
            .collect::<Vec<_>>();
        if axes.len() >= 2 {
            return Ok(Value::Array(axes));
        }

        if node.children.is_empty() {
            if let Some(value) = node.attrs.get("value") {
                return Ok(scalar(value));
            }
            if node.attrs.is_empty() {
                return Ok(Value::Object(serde_json::Map::new()));
            }
        }

        let mut object = serde_json::Map::new();
        for (key, value) in node.attrs {
            if key != "type" && key != "null" && key != "value" {
                object.insert(key, scalar(&value));
            }
        }
        for child in node.children {
            let key = child.name.clone();
            let value = node_value(child)?;
            if let Some(previous) = object.remove(&key) {
                let mut values = match previous {
                    Value::Array(values) => values,
                    other => vec![other],
                };
                values.push(value);
                object.insert(key, Value::Array(values));
            } else {
                object.insert(key, value);
            }
        }
        Ok(Value::Object(object))
    }

    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut stack = Vec::<EngineConfigXmlNode>::new();
    let mut root = None::<EngineConfigXmlNode>;

    loop {
        match reader
            .read_event_into(&mut buffer)
            .map_err(|error| format!("XML read failed path='{}': {error}", path.display()))?
        {
            Event::Start(event) => {
                let name = event.name().into_inner().to_owned();
                stack.push(EngineConfigXmlNode {
                    name,
                    attrs: attrs(&event)?,
                    children: Vec::new(),
                });
            }
            Event::Empty(event) => {
                let name = event.name().into_inner().to_owned();
                let node = EngineConfigXmlNode {
                    name,
                    attrs: attrs(&event)?,
                    children: Vec::new(),
                };
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else if root.replace(node).is_some() {
                    return Err("XML contains more than one root element".to_owned());
                }
            }
            Event::End(_) => {
                let node = stack
                    .pop()
                    .ok_or_else(|| "XML closing element has no matching start".to_owned())?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else if root.replace(node).is_some() {
                    return Err("XML contains more than one root element".to_owned());
                }
            }
            Event::Text(text) if !text.as_ref().chars().all(char::is_whitespace) => {
                return Err(
                    "engine defaults XML uses attribute values; non-whitespace text is not allowed"
                        .to_owned(),
                );
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }

    if !stack.is_empty() {
        return Err("XML ended with unclosed elements".to_owned());
    }
    let root = root.ok_or_else(|| "XML contains no root element".to_owned())?;
    node_value(root)
}

fn merge_engine_defaults(target: &mut Value, patch: &Value) {
    if let (Some(target), Some(patch)) = (target.as_object_mut(), patch.as_object()) {
        for (key, value) in patch {
            match target.get_mut(key) {
                Some(current) => merge_engine_defaults(current, value),
                None => {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
    } else {
        *target = patch.clone();
    }
}

pub(super) fn shared_vfs_mount_policy(shared_assets_dir: &Path) -> Result<VfsMountPolicy, String> {
    let value = load_shared_engine_xml(shared_assets_dir, SHARED_VFS_MOUNTS, "VFS mount policy")?;
    let policy: VfsMountPolicy = serde_json::from_value(value)
        .map_err(|error| format!("Shared Assets VFS mount policy decode failed: {error}"))?;
    if policy.schema != "newviso.runtime.vfs_mount_policy.v1" {
        return Err(format!(
            "unsupported Shared Assets VFS mount policy schema '{}'",
            policy.schema
        ));
    }
    Ok(policy)
}
pub(super) fn resolve_project_capabilities(
    project: &ResolvedProject,
    providers: &[ProviderInfo],
    safe_mode: bool,
    runtime_settings: &ProjectRuntimeSettings,
) -> Result<Vec<ResolvedCapability>, String> {
    let player_requires_physics = default_player_requires_physics(runtime_settings);
    let mut required: Vec<_> = project
        .manifest
        .capabilities
        .required
        .iter()
        .map(|request| CapabilityNeed {
            id: request.id.clone(),
            min_version: request.min_version,
            provider: request.provider.clone(),
            required: true,
        })
        .collect();
    if player_requires_physics
        && !safe_mode
        && !required.iter().any(|need| need.id == "physics.backend")
    {
        let authored = project
            .manifest
            .capabilities
            .optional
            .iter()
            .find(|need| need.id == "physics.backend");
        required.push(CapabilityNeed {
            id: "physics.backend".to_owned(),
            min_version: authored.map(|need| need.min_version).unwrap_or(1),
            provider: authored.and_then(|need| need.provider.clone()).or_else(|| {
                runtime_settings
                    .variables
                    .get("engine_player")
                    .and_then(|player| player.get("physics_provider"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            }),
            required: true,
        });
    }
    let optional = project
        .manifest
        .capabilities
        .optional
        .iter()
        .map(|request| CapabilityNeed {
            id: request.id.clone(),
            min_version: request.min_version,
            provider: request.provider.clone(),
            required: false,
        });

    resolve_capabilities(
        providers,
        required.iter().cloned().chain(
            optional.filter(|need| !safe_mode && !required.iter().any(|base| base.id == need.id)),
        ),
    )
}
pub(super) fn activate_project_capabilities(
    resolved: &[ResolvedCapability],
    running_providers: &mut Vec<RunningProvider>,
) -> Result<(), String> {
    for item in resolved {
        let Some(candidate) = &item.candidate else {
            host::warn(
                "newviso.capabilities",
                format!(
                    "optional capability '{}' version>={} unavailable",
                    item.need.id, item.need.min_version
                ),
            );
            continue;
        };

        if candidate.bootstrap_phase == BootstrapPhase::Platform
            || candidate.engine_gateway.as_deref() == Some("engine.platform")
            || candidate.engine_gateway.as_deref() == Some("engine.render")
        {
            host::info(
                "newviso.capabilities",
                format!(
                    "capability '{}' resolved provider='{}' gateway={} phase={} activation=deferred",
                    candidate.capability_id,
                    candidate.provider_id,
                    candidate.engine_gateway.as_deref().unwrap_or("<none>"),
                    candidate.bootstrap_phase.label()
                ),
            );
            continue;
        }

        let already_running = running_providers
            .iter()
            .any(|provider| provider.id() == candidate.provider_id);

        if !already_running {
            let mut running = RunningProvider::load_current(&candidate.provider_path)?;
            running.initialize()?;
            host::info(
                "newviso.capabilities",
                format!(
                    "activated capability '{}' provider='{}' version={} priority={} path={}",
                    candidate.capability_id,
                    running.id(),
                    candidate.version,
                    candidate.priority,
                    candidate.provider_path.display()
                ),
            );
            running_providers.push(running);
        }

        if let (Some(gateway), Some(service_id)) =
            (&candidate.engine_gateway, &candidate.service_id)
        {
            let registered = host::registered_service_ids()
                .iter()
                .any(|registered| registered == service_id);
            if !registered {
                let message = format!(
                    "capability '{}' provider='{}' did not register descriptor service '{}'",
                    candidate.capability_id, candidate.provider_id, service_id
                );
                if item.need.required {
                    return Err(message);
                }
                host::warn("newviso.capabilities", message);
                continue;
            }

            host::add_alias(gateway.clone(), service_id.clone());
            host::info(
                "newviso.capabilities",
                format!(
                    "bound capability '{}' gateway='{}' -> service='{}' provider='{}'",
                    candidate.capability_id, gateway, service_id, candidate.provider_id
                ),
            );
        }
    }

    Ok(())
}
pub(super) fn log_resolved_capabilities(resolved: &[ResolvedCapability]) {
    for item in resolved {
        match &item.candidate {
            Some(candidate) => host::info(
                "newviso.capabilities",
                format!(
                    "resolved {} capability='{}' min_version={} provider='{}' version={} priority={} gateway={} service={}",
                    if item.need.required { "required" } else { "optional" },
                    item.need.id,
                    item.need.min_version,
                    candidate.provider_id,
                    candidate.version,
                    candidate.priority,
                    candidate.engine_gateway.as_deref().unwrap_or("<none>"),
                    candidate.service_id.as_deref().unwrap_or("<none>")
                ),
            ),
            None => host::warn(
                "newviso.capabilities",
                format!(
                    "unresolved optional capability='{}' min_version={}",
                    item.need.id, item.need.min_version
                ),
            ),
        }
    }
}
pub(super) fn mount_project_files(
    project: &ResolvedProject,
    shared_assets_dir: &Path,
) -> Result<(), String> {
    let assets = AssetClient::new();
    let policy = shared_vfs_mount_policy(shared_assets_dir)?;

    if shared_assets_dir.is_dir() {
        assets.mount_filesystem(
            shared_assets_dir,
            &policy.shared_assets.mount,
            policy.shared_assets.priority,
        )?;
        host::info(
            "newviso.project",
            format!(
                "mounted engine Shared Assets at VFS root path={} priority={}",
                shared_assets_dir.display(),
                policy.shared_assets.priority
            ),
        );
    } else {
        host::warn(
            "newviso.project",
            format!(
                "engine Shared Assets directory is missing path={}",
                shared_assets_dir.display()
            ),
        );
    }

    assets.mount_filesystem(
        &project.root,
        &policy.project_root.mount,
        policy.project_root.priority,
    )?;
    host::info(
        "newviso.project",
        format!(
            "mounted project root at VFS root path={} priority={}",
            project.root.display(),
            policy.project_root.priority
        ),
    );

    if project.assets_dir.is_dir() {
        assets.mount_filesystem(
            &project.assets_dir,
            &policy.project_assets.mount,
            policy.project_assets.priority,
        )?;
        host::info(
            "newviso.project",
            format!(
                "mounted project assets overlay at VFS root path={} priority={}",
                project.assets_dir.display(),
                policy.project_assets.priority
            ),
        );
    }

    Ok(())
}
pub(super) fn load_effective_runtime_settings(
    project: &ResolvedProject,
    shared_assets_dir: &Path,
) -> Result<ProjectRuntimeSettings, String> {
    let shared = load_shared_engine_xml(shared_assets_dir, SHARED_RUNTIME_DEFAULTS, "runtime")?;
    let authored = AssetClient::new()
        .json(&project.manifest.files.runtime)
        .map_err(|error| format!("runtime settings asset load failed: {error}"))?;
    ProjectRuntimeSettings::from_base_and_override(shared, authored)
        .map_err(|error| error.to_string())
}

pub(super) fn load_project_files(
    project: &ResolvedProject,
    shared_assets_dir: &Path,
) -> Result<LoadedProjectFiles, String> {
    let assets = AssetClient::new();

    // Engine defaults are canonical Shared Assets, read from their physical
    // authority rather than through the project-overlaid VFS. This prevents a
    // project from replacing the base document wholesale; project files are
    // explicitly deep-merged below instead.
    let runtime = load_effective_runtime_settings(project, shared_assets_dir)?;

    let mut environment = load_shared_engine_xml(
        shared_assets_dir,
        SHARED_ENVIRONMENT_DEFAULTS,
        "environment",
    )?;
    let project_environment = assets
        .json(&project.manifest.files.environment)
        .map_err(|error| format!("environment asset load failed: {error}"))?;
    merge_engine_defaults(&mut environment, &project_environment);
    let environment =
        ProjectEnvironment::from_value(environment).map_err(|error| error.to_string())?;

    let render_defaults =
        load_shared_engine_xml(shared_assets_dir, SHARED_RENDER_DEFAULTS, "render")?;
    if !render_defaults.is_object() {
        return Err("Shared Assets render defaults must be a JSON object".to_owned());
    }

    let project_scripts = if let Some(manifest_scripts) = project.manifest.scripts.as_ref() {
        Some(manifest_scripts.as_runtime_config())
    } else {
        project
            .manifest
            .files
            .scripts
            .as_deref()
            .map(|logical_path| {
                assets
                    .json(logical_path)
                    .map_err(|error| format!("legacy scripts config asset load failed: {error}"))
                    .and_then(|value| {
                        ProjectScripts::from_value(value).map_err(|error| error.to_string())
                    })
            })
            .transpose()?
    };

    let shared_scripts = ProjectScripts::from_value(load_shared_engine_xml(
        shared_assets_dir,
        SHARED_SCRIPTS_DEFAULTS,
        "scripts",
    )?)
    .map_err(|error| error.to_string())?;
    let scripts = Some(compose_engine_scripts(shared_scripts, project_scripts)?);

    let ui_surface = project
        .manifest
        .files
        .ui
        .as_deref()
        .map(|logical_path| {
            assets
                .decode_json(logical_path, "ui.surface.v1", Value::Null)
                .map_err(|error| format!("UI semantic asset load failed: {error}"))
        })
        .transpose()?;

    let context = json!({
        "identity": {
            "id": project.manifest.project.id,
            "name": project.manifest.project.name,
            "version": project.manifest.project.version,
        },
        "files": project.manifest.files,
        "scripts": project.manifest.scripts,
        "runtime": runtime,
        "environment": environment,
        "render_defaults": render_defaults,
        "engine_defaults": {
            "authority": "shared_assets",
            "runtime": SHARED_RUNTIME_DEFAULTS,
            "environment": SHARED_ENVIRONMENT_DEFAULTS,
            "render": SHARED_RENDER_DEFAULTS,
            "scripts": SHARED_SCRIPTS_DEFAULTS
        },
        "capabilities": project.manifest.capabilities,
    });

    host::info(
        "newviso.project",
        format!(
            "project files loaded runtime='{}' environment='{}' scene='{}' scripts={} ui={} defaults='Shared/Content/config/engine'",
            project.manifest.files.runtime,
            project.manifest.files.environment,
            project.manifest.files.scene,
            project
                .manifest
                .scripts
                .as_ref()
                .map(|scripts| scripts.entrypoint.as_str())
                .or_else(|| project.manifest.files.scripts.as_deref())
                .unwrap_or("<none>"),
            project.manifest.files.ui.as_deref().unwrap_or("<none>")
        ),
    );

    Ok(LoadedProjectFiles {
        runtime,
        environment,
        render_defaults,
        scripts,
        ui_surface,
        context,
    })
}
pub(super) fn start_project_scripting(
    scripts: Option<&ProjectScripts>,
    providers: &[ProviderInfo],
    running_providers: &mut Vec<RunningProvider>,
    event_queue_capacity: usize,
) -> Result<Option<ScriptRuntime>, String> {
    let Some(scripts) = scripts else {
        return Ok(None);
    };
    if !scripts.enabled || scripts.modules.is_empty() {
        host::info("newviso.scripting", "project scripting disabled or empty");
        return Ok(None);
    }

    let provider = require_provider(providers, &scripts.provider).map_err(|_| {
        format!(
            "project requires scripting provider '{}' but its DLL is not deployed",
            scripts.provider
        )
    })?;

    let mut running = RunningProvider::load_current(&provider.path)?;
    running.initialize()?;
    host::info(
        "newviso.scripting",
        format!(
            "started scripting provider '{}' from {}",
            running.id(),
            running.path().display()
        ),
    );
    running_providers.push(running);

    let modules = scripts
        .modules
        .iter()
        .map(|module| ScriptModuleSpec {
            asset: module.asset.clone(),
            on_start: module.on_start.clone(),
            on_frame: module.on_frame.clone(),
            on_event: module.on_event.clone(),
            on_shutdown: module.on_shutdown.clone(),
            permissions: module
                .permissions
                .iter()
                .map(|permission| ScriptPermission {
                    id: permission.id.clone(),
                    scope: permission.scope.clone(),
                })
                .collect(),
        })
        .collect();

    let runtime = ScriptRuntime::load(modules, event_queue_capacity)?;
    host::info(
        "newviso.scripting",
        format!(
            "loaded engine/project script graph='{}'",
            scripts
                .modules
                .iter()
                .map(|module| module.asset.as_str())
                .collect::<Vec<_>>()
                .join(" -> ")
        ),
    );
    Ok(Some(runtime))
}
pub(super) fn require_provider<'a>(
    providers: &'a [ProviderInfo],
    id: &str,
) -> Result<&'a ProviderInfo, String> {
    providers
        .iter()
        .find(|provider| provider.id == id)
        .ok_or_else(|| format!("required provider '{id}' is missing"))
}
pub(super) fn log_provider_inventory(providers: &[ProviderInfo]) {
    host::info(
        "newviso.providers",
        format!("discovered {} providers", providers.len()),
    );

    for provider in providers {
        host::debug(
            "newviso.providers",
            format!(
                "provider='{}' version={} kind={} phase={} root={} descriptor_v2={} path={}",
                provider.id,
                provider.version,
                provider.kind.label(),
                provider.bootstrap_phase.label(),
                provider.root_symbol.label(),
                provider.has_descriptor_v2,
                provider.path.display()
            ),
        );
    }
}
pub(super) fn log_bootstrap(bootstrap: &ResolvedBootstrapConfig) {
    let config_source = if bootstrap.config_loaded {
        bootstrap.config_path.display().to_string()
    } else {
        format!("defaults ({})", bootstrap.config_path.display())
    };

    host::info(
        "newviso.bootstrap",
        format!(
            "starting config={} base={} providers={} safe_mode={}",
            config_source,
            bootstrap.base_dir.display(),
            bootstrap.provider_dir.display(),
            bootstrap.safe_mode
        ),
    );

    if let Some(project_path) = &bootstrap.project_path {
        host::info(
            "newviso.bootstrap",
            format!("project request={}", project_path.display()),
        );
    }

    if bootstrap.safe_mode {
        host::warn(
            "newviso.bootstrap",
            "safe mode enabled: optional capabilities, project scripts/UI startup logic and world persistence are suppressed",
        );
    }

    if let Some(max_frames) = bootstrap.max_frames {
        host::warn(
            "newviso.bootstrap",
            format!("runtime frame limit enabled explicitly: {max_frames}"),
        );
    }

    for item in &bootstrap.cli_overrides {
        host::debug("newviso.bootstrap", format!("CLI override {item}"));
    }
}
