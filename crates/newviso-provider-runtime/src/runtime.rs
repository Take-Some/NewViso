use abi_stable::std_types::RVec;
use libloading::{Library, Symbol};
use newviso_compat_abi::{
    provider::{ConfigBlobV1, PluginDescriptor, PluginModuleDyn, PluginRootV1Ref},
    signature::ROOT_SYMBOL,
};
use newviso_host as host_runtime;
use std::path::{Path, PathBuf};

type RootFn = extern "C" fn() -> PluginRootV1Ref;

pub struct RunningProvider {
    // Keep the dynamic library last so the ABI trait object is destroyed first.
    module: PluginModuleDyn<'static>,
    descriptor: PluginDescriptor,
    path: PathBuf,
    started: bool,
    library: Library,
}

impl RunningProvider {
    pub fn load_current(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let library = unsafe { Library::new(path) }
            .map_err(|error| format!("DLL load failed for {}: {error}", path.display()))?;

        let root: Symbol<RootFn> = unsafe {
            library.get(ROOT_SYMBOL).map_err(|error| {
                format!(
                    "current plugin root missing for {}: {error}",
                    path.display()
                )
            })?
        };

        let root = root();
        let create = root.create();
        let module = create();
        let descriptor = module.descriptor();

        Ok(Self {
            module,
            descriptor,
            path: path.to_path_buf(),
            started: false,
            library,
        })
    }

    pub fn id(&self) -> &str {
        self.descriptor.id.as_str()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    pub fn initialize(&mut self) -> Result<(), String> {
        let defaults: ConfigBlobV1 = self
            .module
            .config_defaults()
            .into_result()
            .map_err(|error| format!("{} config_defaults failed: {error}", self.id()))?;

        let effective = self
            .module
            .config_apply_patches(&defaults, RVec::new())
            .into_result()
            .map_err(|error| format!("{} config_apply_patches failed: {error}", self.id()))?
            .effective;

        self.module
            .init(host_runtime::host_api(), effective)
            .into_result()
            .map_err(|error| format!("{} init failed: {error}", self.id()))?;

        self.module
            .start()
            .into_result()
            .map_err(|error| format!("{} start failed: {error}", self.id()))?;

        self.started = true;
        Ok(())
    }

    pub fn fixed_update(&mut self, dt: f32) -> Result<(), String> {
        self.module
            .fixed_update(dt)
            .into_result()
            .map_err(|error| format!("{} fixed_update failed: {error}", self.id()))
    }

    pub fn update(&mut self, dt: f32) -> Result<(), String> {
        self.module
            .update(dt)
            .into_result()
            .map_err(|error| format!("{} update failed: {error}", self.id()))
    }

    pub fn render(&mut self, dt: f32) -> Result<(), String> {
        self.module
            .render(dt)
            .into_result()
            .map_err(|error| format!("{} render failed: {error}", self.id()))
    }

    pub fn shutdown(&mut self) {
        if self.started {
            self.module.shutdown();
            self.started = false;
        }
    }
}

impl Drop for RunningProvider {
    fn drop(&mut self) {
        self.shutdown();
        let _keep_library_alive = &self.library;
    }
}
