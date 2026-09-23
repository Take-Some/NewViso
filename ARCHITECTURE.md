# NewViso Architecture

NewViso is a small composition host around replaceable runtime providers. The source tree is split by replacement boundary, not by feature count.

## Dependency direction

~~~text
apps/newviso
    |
    v
newviso-runtime ---------------------> newviso-config
    |                                  newviso-core
    |
    +----> newviso-platform ----------> newviso-host -------> newviso-compat-abi
    |
    +----> newviso-provider-runtime --> newviso-host
    |                  |
    |                  +--------------> newviso-compat-abi
    |
    +----> newviso-scene -------------> newviso-render-client ---> newviso-host
                       |
                       +---------------> newviso-host
~~~

Dependencies flow downward only. scripts/verify_boundaries.py enforces the allowed direct NewViso dependencies.

## Crate responsibilities

- newviso-compat-abi: binary compatibility types and symbols for deployed external provider DLLs. No engine policy and no provider loading.
- newviso-config: bootstrap defaults, adjacent config.json, environment overrides, CLI overrides, and provider-role selection.
- newviso-core: engine-neutral runtime state primitives.
- newviso-host: service registry, aliases, event bus, host callbacks, and host-owned platform snapshot service.
- newviso-provider-runtime: provider discovery, ABI probing, DLL ownership, and provider lifecycle.
- newviso-project: project manifest schema, validation, and project-local path resolution. It has no runtime/backend dependencies.
- newviso-platform: platform runtime bridge and event loop. It knows only the generic PlatformApplication callback interface; it does not know about Vulkan, scenes, ECS, or rendering.
- newviso-render-client: typed client facade over the provider-neutral engine.render service protocol.
- newviso-resource-runtime: engine-wide, domain-neutral residency runtime. It owns canonical asset identity and generic streaming/residency policy (owner claims, priorities, dependency closure supplied by the asset service, budgets, churn grace, retry, eviction, and VFS-generation invalidation). It contains no file-format parser, magic table, extension dispatch, or codec registry.
- newviso-scripting: project script lifecycle bridge. It forwards engine-neutral frame snapshots to `engine.scripting` and returns opaque generic command buffers; it contains no gameplay rules.
- newviso-scene: scene/ECS extraction, world bounds, native editor/orbit navigation fallback, scene math, transient render primitives, and homogeneous clip-space render preparation. It contains no genre/player/controller logic and does not know the raw render service wire format.
- newviso-runtime: composition root. Chooses providers by configured role and wires platform, renderer, scene, host, and lifecycle together.
- apps/newviso: executable entry only.

## Provider replacement

Provider role IDs come from configuration and may be overridden without rebuilding NewViso.

~~~cmd
NewViso.exe --set providers.renderer=engine.render.other
NewViso.exe --platform-provider engine.platform.other
~~~

A replacement provider must satisfy the same runtime service/ABI contract expected for that role.

## Rules

1. No NewViso crate may depend on source crates from older engine trees.
2. ABI compatibility is isolated in newviso-compat-abi.
3. The platform layer never owns a renderer or scene implementation.
4. Scene code never speaks the raw renderer JSON wire protocol.
5. The executable is not a composition root; newviso-runtime is.
6. Backend/provider selection is configuration, not source code.
7. New internal dependencies require an explicit architecture-rule update and must preserve acyclic dependency direction.
8. Engine runtime code logs through the configured logging provider; direct stdout/stderr macros are prohibited.
9. Project assets are mounted through the asset service VFS; scene/runtime code does not read project asset files directly. Engine Shared Assets are mounted as a lower-priority fallback and project mounts override them by logical path.
10. Built-in JSON data/configuration belongs to the owning crate under `src/assets/` and is embedded at compile time (for example with `include_str!`). Static JSON configuration must not be hidden inside Rust source; runtime protocol payloads are exempt.
11. Streaming is an engine resource policy. Consumers submit generic AssetAddress interests with owner + priority. AssetManager and its dynamically loaded codec DLLs own all file/container recognition and decoding; NewViso core must not contain format crates, extension tables, magic tables, or codec dispatch. Residency, retry, churn protection, and eviction stay in newviso-resource-runtime.

## Script-owned gameplay

NewViso does not contain an FPS controller or any other built-in game rules. `newviso-runtime` samples raw provider-neutral input once per frame and, for projects with scripts, sends an engine-neutral snapshot to `engine.scripting`: input state, surface/platform state, camera state, scene world metrics, and solid world bounds.

Project scripts return a generic command buffer. The runtime currently routes commands such as `platform.cursor.set`, `scene.camera.set`, `scene.transient_spheres.set`, and `scene.overlay_quads.set`. These are engine capabilities, not gameplay actions: the host does not know what a player, sprint, jump, weapon, projectile, or crosshair is.

`projects/FirstFPS/project.json` declares one script entrypoint, `scripts/main.ysc`. That root module composes project-owned child modules through relative `.ysc` imports. The TypeScript scripting provider—not NewViso—resolves the import graph through `engine.assets`/VFS. The FPS child graph owns physical input mapping, cursor capture policy, movement, look, sprint/FOV/head-bob, character collision, gravity/jump, reset, firing/projectile simulation, and crosshair generation; `world_lighting.ysc` owns the demo day/light behavior. The scene file contains only scene/camera/geometry/collider data. Projects without gameplay scripts may still use native orbit navigation as an engine/editor camera fallback.

Scene vertices preserve homogeneous clip-space W through a dedicated vec4 vertex shader, enabling Vulkan near-plane and behind-camera clipping for arbitrary script-driven cameras.

Scene shaders are packaged SPIR-V under `crates/newviso-scene/src/assets/`. GLSL sources are beside them. Rebuild them with `python scripts/compile_scene_shaders.py` (Vulkan SDK); a normal build and game launch need neither a shader compiler nor `engine.jobs`.


## Asset streaming

The runtime owns one persistent AssetStreamer<AssetClientSource> above the VFS-backed asset source and below all domain consumers.

~~~text
scene / scripting / UI / gameplay / bootstrap consumers
                    |
                    | AssetAddress + owner + priority/pin
                    v
          newviso-resource-runtime
       AssetStreamer / ResourceManager
                    |
                    | opaque address / semantic request
                    v
             engine.assets
        AssetManager / VFS / type registry
                    |
                    v
       dynamically loaded codec DLLs
                    |
                    v
          canonical semantic payload
~~~

The streamer never branches on a filename extension, file magic, container type, scene entity class, renderer resource class, or gameplay class. Concrete formats exist only inside AssetManager codec DLLs. Engine consumers may know stable semantic contracts such as model, material, texture, UI surface, script module, or audio clip, but never the source format that produced them.

Streaming policy is project runtime data (streaming in config/runtime.json): source-container residency budget, per-pump load count/byte budgets, unrequested grace frames, retry delay, and inherited dependency priority. Zero byte/load caps mean unlimited where documented.
