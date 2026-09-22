# NewViso

NewViso is a lightweight modular 3D engine host written in Rust. The host stays small; rendering, platform, input, assets, ECS and other subsystems are supplied by replaceable runtime providers.

The current build opens a real 3D scene with a perspective camera and Vulkan rendering. Hold the left mouse button to orbit around the scene and use the mouse wheel to zoom.

## Build

```powershell
cargo build -p newviso
```

Run from the repository root so the default runtime and asset paths resolve correctly:

```powershell
cargo run -p newviso
```

A normal run has no frame limit. NewViso keeps running until the window is closed or the runtime explicitly requests shutdown.

For automated or development runs, a frame limit can be requested explicitly:

```powershell
cargo run -p newviso -- --max-frames 120
```

## Configuration

NewViso looks for `config.json` next to the executable. If it is not present, built-in defaults are used.

Start from `config.example.json`:

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
  "providers": {
    "logging": "engine.logging.chronicle",
    "input": "engine.input.compass",
    "assets": "engine.assets.starvault",
    "ecs": "engine.ecs.constellation",
    "platform": "engine.platform.winit",
    "renderer": "engine.render.vulkan"
  },
  "runtime": {
    "max_frames": null,
    "skip_platform": false
  }
}
```

Configuration precedence is:

```text
built-in defaults < config.json < environment variables < command line
```

Any supported value can be overridden for one launch:

```powershell
NewViso.exe --set paths.providers=D:\Runtime\Providers
NewViso.exe --set providers.renderer=engine.render.vulkan
NewViso.exe --set runtime.max_frames=120
```

Common path overrides also have short arguments such as `--provider-dir`, `--assets-dir`, `--cache-dir` and `--config`.

## Runtime layout

Provider binaries are loaded from `runtime/providers` by default. They are not committed to this repository.

NewViso routes subsystem communication through host services rather than linking old engine source crates into the workspace. Logging also goes through the configured logging provider; engine code does not write directly to stdout or stderr.

For the crate boundaries and dependency rules, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Development checks

```powershell
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
python scripts/verify_boundaries.py
python scripts/verify_isolation.py
python scripts/verify_no_console_output.py
```
