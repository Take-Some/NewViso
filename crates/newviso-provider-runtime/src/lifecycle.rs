use libloading::{Library, Symbol};
use newviso_compat_abi::{
    provider::{
        CapabilityKind, CapabilityRole, ConfigBlobV1, PluginDescriptor, PluginModuleDyn,
        PluginRootV1Ref,
    },
    signature::{LEGACY_ROOT_SYMBOL, ROOT_SYMBOL},
};
use std::path::{Path, PathBuf};

type RootFn = extern "C" fn() -> PluginRootV1Ref;

#[derive(Clone, Debug)]
pub struct CapabilityProbe {
    pub id: String,
    pub role: CapabilityRole,
    pub kind: CapabilityKind,
    pub version: u32,
    pub describe_json: String,
}

#[derive(Debug)]
pub struct LifecycleProbe {
    pub path: PathBuf,
    pub descriptor_id: String,
    pub descriptor_name: String,
    pub descriptor_version: String,
    pub capability_count: usize,
    pub capabilities: Vec<CapabilityProbe>,
    pub default_content_type: String,
    pub default_bytes: usize,
    pub default_format_version: u32,
}

pub fn probe_lifecycle(path: impl AsRef<Path>) -> Result<LifecycleProbe, String> {
    let path = path.as_ref();
    let library = unsafe { Library::new(path) }
        .map_err(|error| format!("DLL load failed for {}: {error}", path.display()))?;

    let root: Symbol<RootFn> = unsafe {
        library
            .get(ROOT_SYMBOL)
            .or_else(|_| library.get(LEGACY_ROOT_SYMBOL))
            .map_err(|error| format!("plugin root missing for {}: {error}", path.display()))?
    };

    let root = root();
    let create = root.create();
    let module: PluginModuleDyn<'static> = create();

    let descriptor: PluginDescriptor = module.descriptor();
    let defaults: ConfigBlobV1 = module
        .config_defaults()
        .into_result()
        .map_err(|error| format!("config_defaults failed for {}: {error}", path.display()))?;

    let report = LifecycleProbe {
        path: path.to_path_buf(),
        descriptor_id: descriptor.id.to_string(),
        descriptor_name: descriptor.name.to_string(),
        descriptor_version: descriptor.version.to_string(),
        capability_count: descriptor.capabilities.len(),
        capabilities: descriptor
            .capabilities
            .iter()
            .map(|capability| CapabilityProbe {
                id: capability.id.to_string(),
                role: capability.role,
                kind: capability.kind,
                version: capability.version,
                describe_json: capability.describe_json.to_string(),
            })
            .collect(),
        default_content_type: defaults.content_type.to_string(),
        default_bytes: defaults.bytes.len(),
        default_format_version: defaults.format_version,
    };

    drop(defaults);
    drop(descriptor);
    drop(module);
    drop(library);
    Ok(report)
}
