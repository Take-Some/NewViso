use newviso_compat_abi::platform::{PlatformSurfaceMetricsV1, PlatformWindowReadyV1};
use newviso_config::ResolvedBootstrapConfig;
use newviso_core::{EngineState, RuntimePhase};
use newviso_host as host;
use newviso_platform::{run_platform, PlatformApplication, PlatformRunConfig, PlatformRunReport};
use newviso_provider_runtime::{
    probe_directory, probe_lifecycle, ProviderInfo, RootSymbol, RunningProvider,
};
use newviso_scene::{Scene3dLoadReport, Scene3dRuntime};
use std::{env, path::PathBuf};

pub struct RuntimeReport {
    pub provider_count: usize,
    pub scene: Scene3dLoadReport,
    pub platform: Option<PlatformRunReport>,
}

struct EngineApplication {
    renderer_path: PathBuf,
    renderer: Option<RunningProvider>,
    scene: Scene3dRuntime,
    ready: bool,
}

impl EngineApplication {
    fn new(renderer_path: PathBuf, scene: Scene3dRuntime) -> Self {
        Self {
            renderer_path,
            renderer: None,
            scene,
            ready: false,
        }
    }
}

impl PlatformApplication for EngineApplication {
    fn on_window_ready(&mut self, _ready: PlatformWindowReadyV1) -> Result<(), String> {
        let mut renderer = RunningProvider::load_current(&self.renderer_path)?;
        renderer.initialize()?;

        host::info(
            "newviso.runtime",
            format!("renderer '{}' initialized on live window", renderer.id()),
        );

        self.renderer = Some(renderer);
        self.scene.initialize_renderer()?;
        self.ready = true;

        host::info(
            "newviso.runtime",
            format!("3D scene '{}' initialized", self.scene.title()),
        );
        Ok(())
    }

    fn step(&mut self, dt: f32, surface: PlatformSurfaceMetricsV1) -> Result<bool, String> {
        if !self.ready {
            return Ok(false);
        }

        self.scene.update_input()?;
        self.scene.render_frame(surface.width, surface.height)?;

        if let Some(renderer) = self.renderer.as_mut() {
            renderer.update(dt)?;
            renderer.render(dt)?;
        }

        Ok(false)
    }

    fn shutdown(&mut self) {
        self.scene.shutdown_renderer();
        if let Some(mut renderer) = self.renderer.take() {
            renderer.shutdown();
        }
        self.ready = false;
    }
}

pub fn run(bootstrap: ResolvedBootstrapConfig) -> Result<RuntimeReport, String> {
    host::reset();

    let mut state = EngineState::default();
    let mut bootstrap_providers = Vec::<RunningProvider>::new();

    let result = (|| -> Result<RuntimeReport, String> {
        log_bootstrap(&bootstrap);

        if !bootstrap.base_dir.is_dir() {
            return Err(format!(
                "bootstrap base directory does not exist: {}",
                bootstrap.base_dir.display()
            ));
        }

        env::set_current_dir(&bootstrap.base_dir).map_err(|error| {
            format!(
                "failed to switch process base directory to '{}': {error}",
                bootstrap.base_dir.display()
            )
        })?;

        state.set_phase(RuntimePhase::Boot);

        let providers = probe_directory(&bootstrap.provider_dir)
            .map_err(|error| format!("provider discovery failed: {error}"))?;
        log_provider_inventory(&providers);

        host::debug("newviso.runtime", "probing provider lifecycle ABI");
        for provider in &providers {
            if provider.root_symbol == RootSymbol::Legacy {
                host::warn(
                    "newviso.providers",
                    format!(
                        "legacy provider '{}' version={} root={} deferred to legacy adapter",
                        provider.id,
                        provider.version,
                        provider.root_symbol.label()
                    ),
                );
                continue;
            }

            let report = probe_lifecycle(&provider.path).map_err(|error| {
                format!("provider ABI probe failed for {}: {error}", provider.id)
            })?;

            host::debug(
                "newviso.providers",
                format!(
                    "ABI probe passed provider='{}' version={} capabilities={} defaults={} bytes={} format={}",
                    report.descriptor_id,
                    report.descriptor_version,
                    report.capability_count,
                    report.default_content_type,
                    report.default_bytes,
                    report.default_format_version
                ),
            );
        }

        state.set_phase(RuntimePhase::Engine);

        let bootstrap_provider_ids = [
            bootstrap.logging_provider.as_str(),
            bootstrap.input_provider.as_str(),
            bootstrap.assets_provider.as_str(),
            bootstrap.ecs_provider.as_str(),
        ];

        for id in bootstrap_provider_ids {
            let info = require_provider(&providers, id)?;
            let mut running = RunningProvider::load_current(&info.path)?;
            running.initialize()?;

            host::info(
                "newviso.providers",
                format!(
                    "started provider '{}' from {}",
                    running.id(),
                    running.path().display()
                ),
            );

            bootstrap_providers.push(running);
        }

        host::debug(
            "newviso.host",
            format!(
                "registry ready services={:?} event_sinks={}",
                host::registered_service_ids(),
                host::event_sink_count()
            ),
        );

        let (scene, scene_report) = Scene3dRuntime::load_first_scene()
            .map_err(|error| format!("3D scene load failed: {error}"))?;

        host::info(
            "newviso.scene",
            format!(
                "loaded scene '{}' entities={} camera='{}' mesh='{}'",
                scene_report.title,
                scene_report.entity_count,
                scene_report.camera_name,
                scene_report.mesh_name
            ),
        );

        let platform_report = if bootstrap.skip_platform {
            host::info(
                "newviso.runtime",
                "platform runtime skipped by configuration",
            );
            None
        } else {
            let platform = require_provider(&providers, &bootstrap.platform_provider)?;
            let renderer = require_provider(&providers, &bootstrap.renderer_provider)?;

            state.set_phase(RuntimePhase::Platform);
            let app = Box::new(EngineApplication::new(renderer.path.clone(), scene));
            state.set_phase(RuntimePhase::Running);

            let report = run_platform(
                &platform.path,
                PlatformRunConfig {
                    title: "NewViso".to_owned(),
                    width: 1280,
                    height: 720,
                    frame_limit: bootstrap.max_frames,
                },
                app,
            )?;

            if !report.window_ready {
                return Err("platform runtime exited before window_ready".to_owned());
            }
            if let Some(error) = &report.app_error {
                return Err(format!("runtime application failed: {error}"));
            }

            host::info(
                "newviso.runtime",
                format!(
                    "platform stopped frames={} events={} backend={:?} surface={}x{}",
                    report.frames,
                    report.emitted_events,
                    report.backend,
                    report.width,
                    report.height
                ),
            );

            Some(report)
        };

        Ok(RuntimeReport {
            provider_count: providers.len(),
            scene: scene_report,
            platform: platform_report,
        })
    })();

    if let Err(error) = &result {
        host::error("newviso.runtime", error.clone());
    }

    state.set_phase(RuntimePhase::Shutdown);
    host::info("newviso.runtime", "shutdown");

    for provider in bootstrap_providers.iter_mut().rev() {
        provider.shutdown();
    }

    // Host service/event handles are process-lifetime ABI objects. They are
    // intentionally not dropped here: the executable exits immediately after
    // runtime shutdown, while destroying provider-owned ABI trait objects at
    // this boundary is not safe for all deployed providers.

    result
}

fn require_provider<'a>(
    providers: &'a [ProviderInfo],
    id: &str,
) -> Result<&'a ProviderInfo, String> {
    providers
        .iter()
        .find(|provider| provider.id == id)
        .ok_or_else(|| format!("required provider '{id}' is missing"))
}

fn log_provider_inventory(providers: &[ProviderInfo]) {
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

fn log_bootstrap(bootstrap: &ResolvedBootstrapConfig) {
    let config_source = if bootstrap.config_loaded {
        bootstrap.config_path.display().to_string()
    } else {
        format!("defaults ({})", bootstrap.config_path.display())
    };

    host::info(
        "newviso.bootstrap",
        format!(
            "starting config={} base={} providers={}",
            config_source,
            bootstrap.base_dir.display(),
            bootstrap.provider_dir.display()
        ),
    );

    host::debug(
        "newviso.bootstrap",
        format!(
            "paths executable={} assets={} content={} cache={} codecs={}",
            bootstrap.executable_dir.display(),
            bootstrap.assets_dir.display(),
            bootstrap.content_dir.display(),
            bootstrap.cache_dir.display(),
            bootstrap.codecs_dir.display()
        ),
    );

    host::debug(
        "newviso.bootstrap",
        format!(
            "provider roles logging={} input={} assets={} ecs={} platform={} renderer={}",
            bootstrap.logging_provider,
            bootstrap.input_provider,
            bootstrap.assets_provider,
            bootstrap.ecs_provider,
            bootstrap.platform_provider,
            bootstrap.renderer_provider
        ),
    );

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
