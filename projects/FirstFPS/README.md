# First FPS — Blockout Yard

The first playable NewViso FPS blockout: a 32 × 32 metre yard with tiled floor, perimeter walls, cover, columns, a walk-through gate, and project-owned dynamic world lighting.

**All game behavior is project-owned TypeScript.** NewViso has no built-in FPS controller and no hardcoded WASD, sprint, jump, shooting, player collision, projectile, crosshair, sun, or day-cycle behavior.

## Script entrypoint

The project manifest is the authority:

```json
"scripts": {
  "enabled": true,
  "provider": "engine.scripting.typescript",
  "entrypoint": "scripts/main.ysc",
  "lifecycle": {
    "start": "on_start",
    "frame": "on_frame",
    "shutdown": "on_shutdown"
  },
  "permissions": []
}
```

There is no separate `config/scripts.json`. `scripts/main.ysc` is the only project entrypoint. It imports child `.ysc` files with relative paths, and the TypeScript provider resolves those children through AssetManager/VFS.

Current graph:

```text
scripts/main.ysc
├─ ./fps/game.ysc
│  ├─ ./config.ysc
│  ├─ ./math.ysc
│  ├─ ./state.ysc
│  ├─ ./world.ysc
│  ├─ ./projectiles.ysc
│  └─ ./presentation.ysc
└─ ./world/lighting.ysc              (project)
   ├─ ./config.ysc                    (project)
   └─ ../newviso/world/celestial.ysc  (Shared Assets math only)
```

Bare/package imports and paths escaping the project VFS root are rejected.

## Launch

From the repository root:

```powershell
cargo run -p newviso --bin newviso -j1 -- --project projects/FirstFPS
```

Provider DLLs must be installed in `runtime/providers`.

## Controls

| Input | Action |
|---|---|
| Left click inside the window | Capture the cursor and start/resume playing |
| W / A / S / D | Walk forward / left / backward / right |
| Mouse | Look around |
| Left or right Shift | Sprint |
| Space | Jump |
| Left click while captured | Fire a projectile |
| Esc | Release the cursor and pause movement |
| R | Reset player position/view and clear projectiles |
| Alt+Tab / focus loss | Release the cursor and pause |
| Window close / Alt+F4 | Exit |

The physical input mapping and tuning constants live under `scripts/fps/`. `scripts/fps/config.ysc` also configures the generic physics world (fixed rate, gravity, contact skin and scene-collider material); the engine only exposes raw input and generic capabilities.

## Celestial lighting

The day/night policy is project-owned. `scripts/world/config.ysc` defines FirstFPS cycle timing, celestial IDs, elevations, shadow policy and visual parameters; `scripts/world/lighting.ysc` converts that project data into generic scene commands. Shared Assets exposes only reusable celestial math through `assets/scripts/newviso/world/celestial.ysc`.

- Sun: 0.53° angular disc, altitude-driven intensity, red/orange horizon → golden hour → warm daylight color gradient, directional shadows while above the horizon.
- Moon: 0.52° textured billboard using Shared Assets `textures/skydome.ytd@moon_new`, blue horizon → cold neutral high-altitude color gradient, weak directional moonlight and night shadows when the sun is below the horizon.
- Sky dome: direction-based atmospheric shading independent of imported mesh UVs, eliminating stretched cloud bands and UV pole artifacts.
- Clouds: configurable world-anchored cloud layer with coverage/density/softness/scale/wind/detail controls, animated in engine time and naturally tinted by day/night/twilight lighting.
- Sky shader: day/night atmospheric gradient, twilight band, daytime star suppression, night stars, cloud occlusion of celestial bodies and flare, solar limb glow, lunar surface modulation, and screen-space optical sun flare/ghosts.
- Celestial behavior remains project-owned. NewViso only understands generic light, transform, sky-visual and shader/render capabilities.

## Project layout

- `scripts/main.ysc` — project script composition root and lifecycle exports.
- `scripts/fps/game.ysc` — FPS frame orchestration.
- `scripts/fps/config.ysc` — key/button mapping and gameplay tuning.
- `scripts/fps/state.ysc` — mutable gameplay state.
- `scripts/fps/world.ysc` — project-side character/world collision queries.
- `scripts/fps/projectiles.ysc` — projectile body spawn/despawn commands; rigid-body simulation is owned by `engine.physics`.
- `scripts/fps/presentation.ysc` — camera, projectile visualization, and crosshair commands.
- `scripts/fps/math.ysc` — shared game-script math helpers.
- `scripts/world/config.ysc` — FirstFPS celestial/day-night policy.
- `scripts/world/lighting.ysc` — project-owned light/sky command generation.
- `assets/scripts/newviso/world/celestial.ysc` — reusable celestial math only.
- `scenes/yard.scene.json` — scene/camera/geometry/collider data.
- `config/runtime.json` — generic window/runtime configuration.
- `environments/yard.environment.json` — environment/background data.

Child scripts emit only generic engine commands such as camera, cursor, physics-world/body, visibility-channel, sky-clock, transient rendering, entity transform, and light commands. NewViso does not know what a player, sprint, weapon, projectile, crosshair, sun, or day cycle is.

## Architectural invariant

The manifest names the root script. The scripting provider owns relative module resolution. NewViso must not parse TypeScript imports, enumerate child files as gameplay modules, or contain genre-specific state machines.

Scene shaders remain packaged SPIR-V under `crates/newviso-scene/src/assets/`.

## Autonomous world

The root script also runs `world/living_simulation.ysc`, configured by
`world/living_config.ysc`. Logical factory and shop actors produce, deliver and
consume goods on world time even without player input. Stock, shortages and their
causal history are visible in `runtime.living_world`.

The same project now defines a separate `demo.traveler` route spanning 4 km through
generic navigation nodes/edges. The actor starts beside the initial yard, travels on
world time with no player dependency, drops from physical/proxy Scene presentation to
abstract background simulation as it leaves relevant observers, and can rematerialize
at its current logical position when an observer reaches it again. The demo presentation
is a runtime cube; route meaning, speed, presentation policy and coordinates remain
project-owned.

See [Living World integration](../../docs/LIVING_WORLD.md) for the API, checks and limits.
