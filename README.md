# NewViso

NewViso is a lightweight engine host built from scratch around the existing Kaylas Systems runtime providers.

## Hard architecture rules

- NewViso does **not** depend on source crates or Cargo workspaces from NewEngineNorthStar, NewEngineRockstar, ModulesSrc, or Toolset-main.
- NewViso Cargo path dependencies may resolve only inside the NewViso workspace.
- Existing engine subsystems are reused as deployed dynamic providers from `runtime/providers`.
- Compatibility with those providers is implemented locally as a small ABI bridge; the old engine host is not linked into NewViso.
- There is one runtime provider model. NewViso does not maintain a second competing plugin ABI.
- Existing provider implementations remain authoritative for their domains: platform, rendering, assets, ECS, input, logging, physics, audio, UI, scripting, and profiling as compatible binaries become available.

## Current runtime providers

The current runtime package contains these existing release binaries:

- `chronicle-logging-0.8.3-release.dll`
- `compass-input-0.4.1-release.dll`
- `winit-platform-0.5.2-release.dll`
- `starVault-assetManager-3.5.1-release.dll`
- `constellation-ecs-0.1.4-release.dll`
- `vulkan-renderer-0.31.0-release.dll`
- `newengine-physics-jolt-adapter-0.2.0-release.dll`

## Verified integration

The NewViso host currently provides:

- provider signature discovery and bootstrap-phase ordering;
- local mirror of the stable provider ABI without depending on the old API crate;
- provider `descriptor` and `config_defaults` probing;
- host logging callbacks;
- service registry and provider-neutral aliases;
- event sink registration and event broadcast;
- host-owned `engine.platform` window snapshot service;
- existing provider `init/start/update/render/shutdown` lifecycle;
- existing winit platform runtime through `newengine_platform_runtime_run_v1`;
- existing Vulkan renderer initialization while the native HWND is alive.

Verified live path:

```text
NewViso.exe
  -> Chronicle Logging
  -> Compass Input
  -> StarVault AssetManager
  -> Constellation/Flecs ECS
  -> winit 0.5.2
       -> Win32 window
       -> host event bus
       -> engine.platform window snapshot
       -> VulkanRenderer 0.31.0
            -> Vulkan device/swapchain
            -> update/render
            -> shutdown before window destruction
```

A smoke run has completed 30 platform/render frames on a 1280x720 Win32 surface with the existing Vulkan provider.

## Known integration gaps

1. The deployed Jolt 0.2.0 DLL exports the legacy `export_plugin_root` ABI rather than the current `newengine_plugin_root_v1` ABI. NewViso detects it and does not reinterpret it unsafely. A dedicated legacy adapter or a compatible provider binary is required.
2. AssetManager starts successfully, but no prebuilt codec DLLs are currently present in `runtime/providers/codecs`.
3. AssetManager can use an `engine.jobs` service for its background pump. NewViso does not expose that host capability yet, so the provider currently reports the expected fallback warning.
4. AudioRuntime, AureliaUI, Profiler and TypeScript scripting sources are present in `ModulesSrc`, but compatible deployed release binaries have not yet been staged into NewViso runtime.

## Validation

Run:

```cmd
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
python scripts\verify_isolation.py
cargo run -p newviso
```

`scripts/verify_isolation.py` rejects Cargo path dependencies that escape the NewViso workspace or point at legacy project trees.

## Bootstrap configuration precedence

NewViso resolves startup configuration in this order, where every later layer overrides the previous one:

```text
built-in defaults
    < config.json next to newviso.exe
    < NEWVISO_* environment variables
    < command-line overrides
```

Relative paths from an adjacent `config.json` are resolved from the executable directory. When `paths.base` is overridden, provider-relative runtime paths are resolved from that base and NewViso switches the process working directory to it before provider initialization.

Example adjacent configuration:

```json
{
  "paths": {
    "base": ".",
    "providers": "runtime/providers",
    "assets": "assets",
    "content": "content",
    "cache": "cache",
    "codecs": "runtime/providers/codecs"
  },
  "runtime": {
    "platform_smoke_frames": 30,
    "skip_platform_smoke": false
  }
}
```

A single value can be overridden at launch with `--set key=value`:

```cmd
NewViso.exe --set paths.providers=D:\\Runtime\\Providers
NewViso.exe --set paths.cache=E:\\NewVisoCache
NewViso.exe --set runtime.platform_smoke_frames=120
NewViso.exe --set runtime.skip_platform_smoke=true
```

Multiple overrides may be supplied by repeating `--set`.

Convenience aliases are also supported:

```text
--base-dir <path>
--provider-dir <path>
--assets-dir <path>
--content-dir <path>
--cache-dir <path>
--codecs-dir <path>
--platform-smoke-frames <number>
--skip-platform-smoke
--run-platform-smoke
--config <path>
```

The supported generic keys are:

```text
paths.base
paths.providers
paths.assets
paths.content
paths.cache
paths.codecs
runtime.platform_smoke_frames
runtime.skip_platform_smoke
```
