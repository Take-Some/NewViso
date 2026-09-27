# Runtime configuration and script ownership

The host exposes neutral capabilities. Project JSON owns launch configuration;
project scripts own behavior and can change supported policies without rebuilding.
Values in `src/assets/*_defaults.json` are compatibility defaults, not forced values.
Project data and script commands override them. ABI layouts, shader uniform array
sizes, numerical identities and validation invariants remain compiled contracts.

## Configuration sources

1. `config.json`, environment and CLI select bootstrap paths and providers.
2. The project's `files.runtime` asset is decoded through AssetManager/VFS.
3. Packaged runtime defaults are merged with the project runtime document.
4. `startup_commands` execute in order before project `on_start`.
5. Commands returned by `on_start` execute before scene GPU allocations.
6. `runtime.configure` changes supported policies at subsequent command boundaries.

`schema: "newviso.project.runtime.v1"` remains required. Unknown fields, malformed
values, zero queue capacity, invalid camera limits and overflowing byte budgets
are errors. Omitted fields retain compatibility defaults on load. Live patches
merge into CURRENT settings, preserving earlier patches. Object values merge
recursively; arrays and scalar values replace; null is a literal value, not a delete.

## Supported settings

| Section | Fields | Change timing |
| --- | --- | --- |
| `window` | title, width, height | Launch |
| `camera` | rotate_sensitivity, zoom_sensitivity, min_distance, max_distance, rotate_button, min_pitch_degrees, max_pitch_degrees | Launch or live |
| `streaming` | max_resident_mb, max_loads_per_tick, max_source_mb_per_tick, eviction_grace_frames, failed_retry_frames, dependency_priority_scale | Launch or live |
| `scripting` | event_queue_capacity | Launch or live |
| `scheduling` | max_physics_frame_seconds, scene_focus_observer, native_navigation_enabled | Launch or live |
| `variables` | Project-owned JSON object | Launch or live |
| `world_persistence` | Existing save/load/autosave policy | Launch |
| `startup_commands` | Ordered capability command array | Launch only |

Native navigation settings apply to the native orbit fallback. An active script
controller continues to own its own input mapping and camera movement. Disabling
`scene_focus_observer` removes only the implicit Scene-focus observer; explicitly
registered world observers continue to work. Physics still enforces its configured
fixed-step and maximum-step policies after applying the configurable input delta cap.

Each live patch is fully decoded and validated before its fields are applied.
A successful patch updates `runtime.settings`, `project.runtime` and emits
`runtime.settings.changed`. This atomic validation applies to one settings patch;
a whole heterogeneous command buffer is ordered, not transactional. Provider or
host infrastructure failures may still abort execution.

Shrinking an event queue retains the newest events. Discarded events are counted
in the existing dropped-event notification with the current capacity. Changing
streaming dependency priority updates already discovered dependencies without
clearing loaded assets. Zero load/byte budgets preserve the existing unlimited
semantics. Live settings are not automatically written back to project files.

## Typed script API

Shared VFS asset: `scripts/newviso/runtime.ysc`.

```typescript
import {
  RuntimeConfiguration,
  RuntimeSettingsPayload,
} from "./newviso/runtime.ysc";

interface GameVariables {
  quality: string;
  movementSpeed: number;
}

export function on_start(payload: RuntimeSettingsPayload<GameVariables>) {
  const runtime = new RuntimeConfiguration<GameVariables>(payload);
  const speed = runtime.settings.variables.movementSpeed;
  // Project behavior uses speed; the engine does not define movement rules.
  return {
    commands: [
      runtime.configure({
        streaming: { max_loads_per_tick: 16 },
        scheduling: { scene_focus_observer: false },
        variables: { quality: "high", movementSpeed: speed },
      }),
      runtime.configureRenderer({ shadow_resolution: 4096 }),
    ],
  };
}
```

The snapshot is from the beginning of a lifecycle call. Returned commands take
effect after that call returns; read the next payload to observe the result.
Unsigned JSON integers represented by TypeScript numbers should stay within
`Number.MAX_SAFE_INTEGER` for exact values.

Raw live command:

```json
{
  "op": "runtime.configure",
  "settings": {
    "camera": { "rotate_button": 2, "max_distance": 300 },
    "scripting": { "event_queue_capacity": 8192 },
    "scheduling": { "max_physics_frame_seconds": 0.1 }
  }
}
```

## GPU allocation policy

`scene.render.configure` accepts a partial `settings` object with:
`runtime_cube_capacity`, `transient_sphere_capacity`, `overlay_quad_capacity`,
`lens_flare_capacity`, `flare_element_capacity`, `shadow_resolution`.
Set it in `startup_commands` or return it from `on_start`. Commands during normal
frames are rejected because the GPU allocations already exist. This is explicit
startup configuration, not live GPU-buffer reallocation. Current effective values
are exposed under `runtime.scene.render_settings`.

Capacity arithmetic is checked before allocation. The backend remains responsible
for device-specific resource size limits. `runtime_cube_capacity` is additional
capacity beyond cubes present at renderer initialization. Fixed shader light and
sky-visual array counts are shader contracts, not allocation policy fields.

Example in `config/runtime.json`:

```json
{
  "schema": "newviso.project.runtime.v1",
  "variables": { "quality": "high", "movementSpeed": 4.5 },
  "startup_commands": [
    {
      "op": "scene.render.configure",
      "settings": { "shadow_resolution": 2048, "runtime_cube_capacity": 8192 }
    }
  ]
}
```

Existing commands still control physics-world parameters, world clocks, simulation
LOD, actors, routes, population, scene entities, lights, atmosphere and weather.
They may also be used in startup_commands without adding Rust gameplay logic.
The sky animation clock now wraps at the configured timecycle duration instead
of an unconditional 86400 seconds.

## Scope and remaining work

This change removes selected forced runtime policies and provides a validated
configuration path. It is NOT a proof that the entire engine has no hardcoded
policy. Native shader code still contains visual formulas and fixed arrays;
scene descriptor defaults, host/client resource limits and other subsystem
fallbacks require a separate inventory before claiming full coverage. Provider
DLL internals are outside this host repository. Extending a live configurable
field requires wiring it to its actual consumer, not merely adding JSON fields.

## Verification (2026-09-25)

- Workspace check passed: cargo check --workspace -j 1 --offline.
- Workspace tests passed: 96 passed, 0 failed (including seven new regression tests).
- Final application build passed: cargo build -p newviso --bin newviso -j 1 --offline.
- Live TypeScript/VFS + Vulkan smoke passed: three platform frames, exit code 0.
  Script assertions verified live settings, project variables, queue capacity,
  startup renderer policy and compatibility of scene.orbit.configure snapshots.
  GPU allocation used the script-selected 512 x 512 shadow map.
- Isolation and direct-console-output checks passed; git diff --check passed.
- Existing architecture boundary check failures remain: missing noise/world rules
  and existing physics-client/world dependencies absent from the runtime allowlist.
  No new crate dependencies were introduced by this change.
- Existing code-health findings decreased from 53 to 52: five oversized modules
  and 47 duplicate-window findings remain. This is not a clean code-health gate.
- The initial First3D-derived smoke was blocked by the deployed UI codec rejecting
  ui/runtime.neui.xml. The isolated verification fixture omitted UI and passed;
  this does not certify the original UI project or FirstFPS end to end.
