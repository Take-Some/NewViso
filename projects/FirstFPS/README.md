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
└─ ./newviso/world_lighting.ysc      (Shared Assets)
   └─ ./world/celestial.ysc          (Shared Assets)
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

The physical input mapping and tuning constants live under `scripts/fps/`; the engine only exposes raw input and generic capabilities.

## Celestial lighting

`assets/scripts/newviso/world_lighting.ysc` is the shared lifecycle adapter. `assets/scripts/newviso/world/celestial.ysc` owns the reusable accelerated 240-second day/night cycle. FirstFPS reaches both through the Shared Assets VFS fallback.

- Sun: 0.53° angular disc, altitude-driven intensity, red/orange horizon → golden hour → warm daylight color gradient, directional shadows while above the horizon.
- Moon: 0.52° textured billboard using Shared Assets `textures/fps/skydome.ytd@moon_new`, blue horizon → cold neutral high-altitude color gradient, weak directional moonlight and night shadows when the sun is below the horizon.
- Sky shader: day/night atmospheric gradient, twilight band, daytime star suppression, night stars, cloud response, solar limb glow, lunar surface modulation, and screen-space optical sun flare/ghosts.
- Celestial behavior remains project-owned. NewViso only understands generic light, transform, sky-visual and shader/render capabilities.

## Project layout

- `scripts/main.ysc` — project script composition root and lifecycle exports.
- `scripts/fps/game.ysc` — FPS frame orchestration.
- `scripts/fps/config.ysc` — key/button mapping and gameplay tuning.
- `scripts/fps/state.ysc` — mutable gameplay state.
- `scripts/fps/world.ysc` — project-side character/world collision queries.
- `scripts/fps/projectiles.ysc` — projectile simulation.
- `scripts/fps/presentation.ysc` — camera, projectile visualization, and crosshair commands.
- `scripts/fps/math.ysc` — shared game-script math helpers.
- `assets/scripts/newviso/world_lighting.ysc` — shared celestial lifecycle adapter.
- `assets/scripts/newviso/world/celestial.ysc` — shared sun/moon/day-night behavior.
- `scenes/yard.scene.json` — scene/camera/geometry/collider data.
- `config/runtime.json` — generic window/runtime configuration.
- `environments/yard.environment.json` — environment/background data.

Child scripts emit only generic engine commands such as camera, cursor, transient rendering, entity transform, and light commands. NewViso does not know what a player, sprint, weapon, projectile, crosshair, sun, or day cycle is.

## Architectural invariant

The manifest names the root script. The scripting provider owns relative module resolution. NewViso must not parse TypeScript imports, enumerate child files as gameplay modules, or contain genre-specific state machines.

Scene shaders remain packaged SPIR-V under `crates/newviso-scene/src/assets/`.
