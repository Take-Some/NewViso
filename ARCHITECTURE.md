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
    |                  |
    |                  +---------------> newviso-host
    |
    +----> newviso-world
    |
    +----> newviso-agent
    |
    +----> newviso-navigation
    |
    +----> newviso-character -------> newviso-collision
    |
    +----> newviso-steering
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
- newviso-semantic-assets: source-format-neutral materializer for stable AssetManager semantic payloads such as `model.runtime_v1` and `collision.runtime_v1`. It converts validated semantic wires into `ModelResource` / `CollisionMeshResource`; it must not parse source containers or branch on source extensions/magic.
- newviso-scripting: project script lifecycle bridge. It forwards engine-neutral frame snapshots to `engine.scripting` and returns opaque generic command buffers; it contains no gameplay rules.
- newviso-scene: scene/ECS extraction, world bounds, native editor/orbit navigation fallback, scene math, transient render primitives, and homogeneous clip-space render preparation. It contains no genre/player/controller logic and does not know the raw render service wire format.
- newviso-world: engine-neutral autonomous world-simulation backend. It owns world clock/fixed stepping, background actors, multi-observer simulation LOD, recurring processes, scheduled events, persistent world facts, population metadata, zones, scenario points, relationships, and temporary stimuli. It contains no player-specific policy, NPC archetypes, factions, traffic rules, economy rules, or game-specific event meanings.
- newviso-agent: engine-neutral autonomous-agent intelligence layer above world existence. It owns bounded perception memory, blackboards, priority task lanes, task lifecycle/arbitration, and generic agent intentions. It consumes typed world snapshots and emits capability-level commands; it contains no pedestrian archetypes, factions, combat rules, traffic rules, weapons, or project-specific event meanings.
- newviso-navigation: engine-neutral local navigation service for materialized actors. It builds streamed walkable polygon tiles from generic collision triangles, owns polygon corridors, budgeted path requests, portal/string-pull smoothing, off-mesh connectivity, and dynamic navigation obstacles. It does not replace the LivingWorld coarse route graph.
- newviso-character: engine-neutral collision-safe character motion. It owns capsule-style continuous sweeps, grounding, slope/step handling, depenetration skin, moving supports and physical motion state. It has no player or NPC policy.
- newviso-steering: engine-neutral local velocity selection and avoidance. It resolves desired path velocity against moving neighbors and dynamic obstacle horizons; it does not choose goals or gameplay behavior.
- newviso-runtime: composition root. Chooses providers by configured role and wires platform, renderer, scene, living world, autonomous agents, local navigation, physical character motion, steering, host, and lifecycle together.
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
12. Live runtime composition is project-driven. Engine crates may define generic validation bounds and neutral fallbacks, but may not contain demo/game policy, concrete project references, gameplay event names, artistic presets, physics-world tuning, or fixed gameplay visibility categories.
13. Shared scripts may provide reusable libraries and capability adapters; concrete game/world behavior belongs under the project script graph.
14. Shared engine scripts under `../Shared/Content/scripts/newviso` expose typed OOP APIs. Public behavior belongs on classes/instances with explicit interfaces/types; exported function bags and `any`-typed public contracts are not allowed.

## Script-owned gameplay

NewViso does not contain an FPS controller or any other built-in game rules. `newviso-runtime` samples raw provider-neutral input once per frame and, for projects with scripts, sends an engine-neutral snapshot to `engine.scripting`: input state, surface/platform state, camera state, scene world metrics, and solid world bounds.

Project scripts return a generic command buffer. The runtime routes capability-level commands such as `events.emit`, `console.write`, `audio.cue.preload`, `audio.cue.play`, `audio.listener.set`, `platform.cursor.set`, `scene.camera.set`, `scene.entity.visibility.set`, `scene.sky.time_scale.set`, `scene.timecycle.state.set`, `scene.weather.state.set`, `scene.sky.atmosphere.set`, `scene.sky_clouds.set`, `scene.atmospheric_cloud_layer.target.set`, `scene.atmospheric_cloud_layer.target.clear`, `scene.environment.set`, `world.clock.configure`, `world.clock.set`, `world.simulation.configure`, `world.observer.upsert`, `world.actor.upsert`, `world.process.upsert`, `world.event.schedule`, `world.fact.set`, `world.navigation.node.upsert`, `world.navigation.edge.upsert`, `world.travel.start`, `world.actor.presentation.bind`, `world.population.channel.upsert`, `world.population.streaming.configure`, `world.zone.upsert`, `world.model_set.upsert`, `world.scenario_point.upsert`, `world.relationship.set`, `world.stimulus.emit`, `agent.upsert`, `agent.task.set`, `agent.task.clear`, `agent.blackboard.set`, `character.world_actor.bind`, `character.world_actor.unbind`, `navigation.configure`, `navigation.obstacle.upsert`, `navigation.off_mesh_link.upsert`, `physics.world.configure`, `physics.body.upsert`, `physics.body.destroy`, `scene.transient_spheres.set`, and `scene.overlay_quads.set`. These are engine capabilities, not gameplay actions: the host does not know what a player, sprint, jump, weapon, projectile, crosshair, sun, moon, or day cycle is.

Script-visible events follow the same boundary. `newviso-events` defines the engine-neutral `newviso.event.v1` envelope, `newviso-host` owns sequencing and fan-out, `newviso-scripting` forwards subscribed host events to project `on_event`, and the generic `events.emit` script command publishes project-originated events back through the same host bus. The engine does not define gameplay topic names such as walk, jump, footstep, weapon fire, or interaction; projects define those semantics. Shared script libraries may provide reusable subscription, wildcard-routing, priority, once/off, and cancelable-before helpers without moving gameplay policy into the host.

`console.write` and the `audio.*` commands are likewise engine-level capabilities. `Shared/Content/scripts/newviso/console.ysc` and `audio.ysc` are the reusable script-facing adapters; the runtime validates and routes their commands to `newviso-host` logging and the replaceable `engine.audio` gateway. The active scene camera is the default spatial audio listener, while an explicit `audio.listener.set` command can override that default. Reusable policy such as mapping `character.footstep`/`character.jump`/`character.land` to a shared SoundCue bank belongs in Shared script content, not in an individual project or the engine host.

Collision-contact surface identity follows the same format-neutral boundary. Imported collision resources may carry an opaque per-triangle numeric material/surface id. A replaceable physics provider supplies contact entities and world-space contact points; for resident streamed meshes the runtime resolves that point against the authored collision BVH and enriches the contact with optional `surface_id`, `surface_entity`, and `surface_triangle` fields. Core does not interpret those numbers as concrete, metal, wood, or any source-format material name. Source-format compatibility tables and gameplay/audio categorization belong in Shared content or project policy.

NewViso is project-driven. A live runtime launch requires `--project <project-root>` and loads the startup scene, environment, runtime policy, capabilities, providers, and script entrypoint from that project. There is no embedded demo scene fallback. Project scene assets own initial camera and primitive material data; missing required camera fields or primitive material colors are rejected rather than replaced with demo values.

`../Projects/FirstFPS/project.json` declares one script entrypoint, `scripts/main.ysc`. That root module composes project-owned child modules through relative `.ysc` imports. The TypeScript scripting provider—not NewViso—resolves the import graph through `engine.assets`/VFS. The FPS scripts own physical input mapping, cursor capture policy, movement, look, sprint/FOV/head-bob, character collision, jump/reset/fire decisions, projectile body creation/destruction, physics-world tuning, visualization, and crosshair generation. `scripts/world/*` owns FirstFPS day/night, celestial light, shadow and visual policy. The external `engine.physics` provider owns rigid-body integration and contacts. Shared script assets contain reusable APIs/math only, not a game-specific world preset.

The living-world layer follows the same boundary. `newviso-world` advances on its own fixed world clock and does not require a player entity to exist. Background actors, recurring world processes, scheduled events, facts, zones, relationships, navigation routes, active travel, and stimuli continue to advance without player input. Observers—including the current scene focus when present—only raise simulation fidelity through full/reduced/background update tiers; they do not create the world, stop the world, or determine whether distant actors exist. Multiple persistent observers can be registered for settlements, cameras, simulations, or other project-defined interests.

Logical existence and Scene representation are deliberately separate. A world actor always owns its authoritative logical position/state while enabled. The generic route graph can move that actor in full, reduced, or background simulation. Representation tiers map to `physical`, `proxy`, and `abstract`; a project may bind any subset of those tiers to a Scene presentation. When a bound actor leaves the configured materialized tiers, the Scene entity becomes dormant instead of deleting the world actor. When it returns, the same presentation is reactivated at the actor's current authoritative world position. Materialization therefore cannot rewind an actor to the point where it was last visible. Runtime cube presentation exists as a format-neutral diagnostic/demo visual; asset-backed presentation uses the normal Scene/AssetAddress streaming path and must not introduce file-format knowledge into `newviso-world`.

The engine stores and validates neutral population channels, population-streaming budgets, spatial zones, ambient model sets, scenario points, scenario reservations, navigation nodes/edges, relationship rules, and temporary world stimuli. It does not define pedestrians, traffic, factions, crimes, ambient activities, reactions, economy rules, road semantics, transport modes, or spawn schedules. Shared scripts provide reusable typed orchestration helpers; project scripts decide which groups, models, scenarios, routes, densities, schedules, travel modes, reactions, state transitions, and relationships exist.

Generic autonomous decision execution is owned by `newviso-agent`, not `newviso-world`. A persistent world actor may have zero or one project-bound agent. The agent layer receives typed actor/stimulus snapshots, maintains bounded perception memory and blackboard state, arbitrates generic task lanes, and emits capability-level intentions such as a world travel request.

Materialized full-tier actors may additionally bind the generic physical-character stack. `newviso-navigation` refines the current coarse LivingWorld route segment against streamed walkable collision polygons; `newviso-steering` resolves local avoidance; `newviso-character` applies the resulting velocity through continuous collision queries. While physical motion is active, LivingWorld retains the travel intent but delegates authoritative position updates to the character controller. When the actor dematerializes or drops to a lower simulation tier, that authority is released and coarse route travel resumes from the accumulated real position without rewinding. IK, ragdoll, combat, cover, vehicle use, and game-specific behavior remain separate capabilities or project policy. See `docs/npc-runtime.md`.

Scene visibility is based on arbitrary project-defined named channels rather than a fixed engine enum such as Player/Gameplay/VFX. Projects without gameplay scripts may still use native orbit navigation as an engine/editor camera fallback.

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

The concrete RSC7 YDR/YBN provider path, semantic wire layouts, deployment layout and fixture tests are documented in `docs/rsc7-asset-pipeline.md`.


## Runtime policy configuration

See `docs/runtime-configuration.md` for the configuration schema and lifecycle.
Engine policy defaults are Shared Assets under `Shared/Content/config/engine/`:
`runtime.defaults.xml`, `environment.defaults.xml`, `render.defaults.xml`, and
`vfs_mounts.xml`. Project assets are deep-merged over that base. Live
`runtime.configure` commands validate before applying camera, streaming, scripting,
scheduling and project-variable patches. Shared renderer capacity policy is installed
before project startup commands, which may override it before GPU allocation. ABI
and shader layouts remain compiled contracts rather than configurable defaults.

## Sky and cloud runtime

The sky dome is an infinite-depth render capability with a world-anchored horizon and
camera-relative horizontal origin. Procedural cloud motion owns persistent runtime
phase state rather than deriving every layer from a single wrapped time value. The
large/base cloud phase follows normalized global wind direction, while small,
overall-detail and edge-detail channels have independent continuous time-cycle phases.
Project/weather data supplies the speeds and shaping parameters; the renderer owns
sampling and phase integration.

Authored atmospheric cloud geometry is a second, separate capability. Its layer
runtime owns camera-relative transforms, angular animation, three independent
COMBINE/SCULPT UV channels, wind modulation, altitude fades, weather weights and
temporary script overrides. Per-layer transition duration percentages and delay,
piecewise transition-midpoint remapping, transition alpha shaping and a generic
cost budget control runtime admission. GPU layer resources are paged in/out with
that admission state rather than remaining permanently allocated.

Atmospheric geometry uses a dedicated depth-tested, depth-write-disabled alpha
pipeline and is never hard-coded into the procedural dome shader. For soft
intersection with opaque world geometry, Scene builds a low-resolution R32F linear
camera-depth proxy from the compact opaque/cutout indirect submission stream; the cloud
shader samples it in screen space and fades over each layer's configured
soft-intersection distance. This is a provider-neutral semantic equivalent of the
reference soft-particle use of scene depth, not a copied source shader.

True volumetric clouds are a third, independent capability and do not use either the
sky dome or atmospheric carrier geometry for their visible shape. Scene intersects
camera rays with a project-authored altitude slab and integrates an XYZ-dependent
density field using Beer-Lambert extinction. A short secondary march toward the
atmosphere-driving sun supplies self-shadowing; Henyey-Greenstein phase, ambient,
powder and a bounded multi-scattering approximation provide lighting. The pass uses
low-resolution Rgba16Float render targets, linear opaque-scene depth termination and
ping-pong temporal history before full-resolution composition. The present backend
has no Texture3D contract, so generic volume density is synthesized from weather/base
and detail Texture2D resources projected across XZ/XY/ZY plus a vertical height
profile. This is still spatial density, not a 2D cloud carrier.

See docs/sky-cloud-pipeline.md.

## Navigation and renderer implementation ownership

Navigation keeps the public runtime and descriptions separate from incremental
tile ingestion, edge/off-mesh connectivity, path requests and geometry. A derived
polygon broad phase owns local endpoint candidates; exact triangle distance and
obstacle policy remain in search. Oversized faces and large query windows retain
complete coverage. No new format, provider ABI, or project policy is introduced.

Renderer resource creation, shader/pipeline construction, geometry uploads,
material residency, draw-list construction, graph declaration, pass recording and
teardown have separate modules under `first_scene/`. Frame orchestration owns an
acquisition scope: after acquisition every fallible exit aborts the frame until
successful finalization. Pipeline invariants and material/vertex binding state
have one shared implementation.

See [the 2026-10-04 refactor report](docs/architecture-refactor-2026-10-04.md)
for module ownership, reproducible endpoint benchmarks, verification, and the
remaining repository/deployed-provider findings.
