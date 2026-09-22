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
        eprintln!(
            "[newviso/runtime] renderer '{}' initialized on live window",
            renderer.id()
        );

        self.renderer = Some(renderer);
        self.scene.initialize_renderer()?;
        self.ready = true;

        eprintln!(
            "[newviso/runtime] 3D scene '{}' initialized",
            self.scene.title()
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
    print_bootstrap(&bootstrap);

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

    let mut state = EngineState::default();
    state.set_phase(RuntimePhase::Boot);

    let providers = probe_directory(&bootstrap.provider_dir)
        .map_err(|error| format!("provider discovery failed: {error}"))?;
    print_provider_inventory(&providers);

    println!("provider lifecycle ABI probes:");
    for provider in &providers {
        if provider.root_symbol == RootSymbol::Legacy {
            println!(
                "  LEGACY {} {} root={} (deferred to legacy adapter)",
                provider.id,
                provider.version,
                provider.root_symbol.label()
            );
            continue;
        }

        let report = probe_lifecycle(&provider.path)
            .map_err(|error| format!("provider ABI probe failed for {}: {error}", provider.id))?;
        println!(
            "  PASS {} {} capabilities={} defaults={} bytes={} format={}",
            report.descriptor_id,
            report.descriptor_version,
            report.capability_count,
            report.default_content_type,
            report.default_bytes,
            report.default_format_version
        );
    }

    host::reset();
    state.set_phase(RuntimePhase::Engine);

    let bootstrap_provider_ids = [
        bootstrap.logging_provider.as_str(),
        bootstrap.input_provider.as_str(),
        bootstrap.assets_provider.as_str(),
        bootstrap.ecs_provider.as_str(),
    ];

    let mut bootstrap_providers = Vec::new();
    println!("provider init/start:");
    for id in bootstrap_provider_ids {
        let info = require_provider(&providers, id)?;
        let mut running = RunningProvider::load_current(&info.path)?;
        running.initialize()?;
        println!("  PASS {} <- {}", running.id(), running.path().display());
        bootstrap_providers.push(running);
    }

    println!(
        "host registry: services={:?} event_sinks={}",
        host::registered_service_ids(),
        host::event_sink_count()
    );

    let (scene, scene_report) = Scene3dRuntime::load_first_scene()
        .map_err(|error| format!("3D scene load failed: {error}"))?;
    println!(
        "3D scene: '{}' entities={} camera='{}' mesh='{}'",
        scene_report.title,
        scene_report.entity_count,
        scene_report.camera_name,
        scene_report.mesh_name
    );

    let platform_report = if bootstrap.skip_platform_smoke {
        println!("platform runtime: skipped by bootstrap configuration");
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
                frame_limit: Some(bootstrap.platform_smoke_frames),
            },
            app,
        )?;

        println!(
            "runtime: frames={} events={} window_ready={} backend={:?} surface={}x{} app_error={:?}",
            report.frames,
            report.emitted_events,
            report.window_ready,
            report.backend,
            report.width,
            report.height,
            report.app_error
        );

        if !report.window_ready {
            return Err("platform runtime exited before window_ready".to_owned());
        }
        if let Some(error) = &report.app_error {
            return Err(format!("runtime application failed: {error}"));
        }
        Some(report)
    };

    state.set_phase(RuntimePhase::Shutdown);
    for provider in bootstrap_providers.iter_mut().rev() {
        provider.shutdown();
    }

    Ok(RuntimeReport {
        provider_count: providers.len(),
        scene: scene_report,
        platform: platform_report,
    })
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

fn print_provider_inventory(providers: &[ProviderInfo]) {
    println!("existing providers discovered: {}", providers.len());
    for provider in providers {
        println!(
            "  {} {} [{}:{}] root={} descriptor_v2={} <- {}",
            provider.id,
            provider.version,
            provider.kind.label(),
            provider.bootstrap_phase.label(),
            provider.root_symbol.label(),
            provider.has_descriptor_v2,
            provider.path.display()
        );
    }
}

fn print_bootstrap(bootstrap: &ResolvedBootstrapConfig) {
    if bootstrap.config_loaded {
        println!("bootstrap config: {}", bootstrap.config_path.display());
    } else {
        println!(
            "bootstrap config: <defaults> ({} not found)",
            bootstrap.config_path.display()
        );
    }

    println!(
        "executable directory: {}",
        bootstrap.executable_dir.display()
    );
    println!("base directory: {}", bootstrap.base_dir.display());
    println!("provider directory: {}", bootstrap.provider_dir.display());
    println!("assets directory: {}", bootstrap.assets_dir.display());
    println!("content directory: {}", bootstrap.content_dir.display());
    println!("cache directory: {}", bootstrap.cache_dir.display());
    println!("codecs directory: {}", bootstrap.codecs_dir.display());
    println!("provider roles:");
    println!("  logging  = {}", bootstrap.logging_provider);
    println!("  input    = {}", bootstrap.input_provider);
    println!("  assets   = {}", bootstrap.assets_provider);
    println!("  ecs      = {}", bootstrap.ecs_provider);
    println!("  platform = {}", bootstrap.platform_provider);
    println!("  renderer = {}", bootstrap.renderer_provider);

    if !bootstrap.cli_overrides.is_empty() {
        println!("CLI overrides:");
        for item in &bootstrap.cli_overrides {
            println!("  {item}");
        }
    }
}
