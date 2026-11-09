# NewViso — Skinning, Project Context Cache, Bullet Endorphin Verification
Date: 2026-10-24

## Overview / source of truth

Work is in `C:\Users\Kayla\Documents\NorthStar\NewViso`,
`C:\Users\Kayla\Documents\NorthStar\Projects\FirstFPS`, and
`C:\Users\Kayla\Documents\NorthStar\Shared\Content`.
This pass did **not** rebuild or replace installed Release plugin DLLs.
The user workspace already has broad unrelated changes: no reset, checkout,
clean or overwrite of that work was performed.

## Skinning: evidence and temporary repair

Affected original asset:
`Projects/FirstFPS/assets/models/characters/gta/a_f_y_beach_01/source/a_f_y_beach_01.ydd`.
A new ped-cooker diagnostic option `--inspect-component-skin-domain` finds:

- Skin palette: 101 entries (joint indices 0–100).
- Only accessory component `0x0DAEDAE6` (`accs_000_u`) is invalid.
- Accessory geometry uses all 440 vertices, so discarding defective vertices is unsafe.
- 162 nonzero-weight out-of-range joint influences: 151 index 255, seven 101,
  one 102, three 103.
- Some source weights exceed a total of 1.0; packing semantics have not
  been recovered. A speculative correction was removed, not deployed.
- The source's other selected four components pass the cooker's strict
  remapping and normal skeleton validation.

**Temporary workaround, not a complete original model**: recooked a four-
component character model from the original source, omitting only the broken
accessory. NEF8 runtime model
`Projects/FirstFPS/assets/models/characters/gta/a_f_y_beach_01/a_f_y_beach_01.ydr`
was replaced with a 189835-byte build. Its original 204124-byte file is
saved as
`a_f_y_beach_01.ydr.bak-20261024-full-five-components`.
`native.character.json` is updated with accurate size/digest and
`runtime_skin_workaround` metadata; its original is saved as
`native.character.json.bak-20261024-full-five-components`.
Source RSC7 `.ydd` is untouched.

Live `FirstFPS --max-frames 1300` then exited 0, with 842 trace
samples and **no** repeated
`skin joint index outside skeleton` or `streaming failed` errors.

**Permanent fix remaining:** reverse source RSC7 accessory skin's actual
palette and packed-weight convention and regenerate the original five-part
actor, with source-to-output per-vertex semantic tests. Do not enable
arbitrary joint clipping or map 255 to root.

## Script transport profiling / optimization

`newviso.perf.trace.script` instrumentation separated delta generation,
provider RPC/invocation, response merge and transport byte counts.

Before: the script system serialized immutable project metadata
(`project_bytes=309591`) **every frame** in FirstFPS, despite already using
`newviso.runtime_delta.v1`. Frame deltas frequently only required 4–10KiB.

Added an explicitly opt-in `project_context_cache` flag to ProjectScript
config and runtime wiring. It is **false by default**; only when *all*
active frame hooks opt in and have successfully received a full project
base does the host send `__project_cached: true` instead of the large
`project` subtree. The shared `snapshot_delta.ysc` decoder restores the
frozen immutable project tree; on cache miss it throws, not silently
synthesizes data. Reload invalidates the cache, causing another full
project base. Existing snapshot delta is unchanged.

Enabled in:
- `Shared/Content/config/engine/scripts.defaults.xml`
- `Projects/FirstFPS/project.json`
- `Projects/FirstFPS/scripts/main.ysc` (decoder on start and frame)

Code:
- `NewViso/crates/newviso-project/src/lib.rs`
- `NewViso/crates/newviso-runtime/src/bootstrap_support.rs`
- `NewViso/crates/newviso-scripting/src/lib.rs`
- `Shared/Content/scripts/newviso/snapshot_delta.ysc`

Other projects keep full-context semantics unless explicitly opted in;
a 180-platform-frame ProceduralMap control run exited 0. Regression
unit test prevents cache omission for a mixed opted/unopted module set,
or before first base has been delivered.

### FirstFPS controlled-ish CPU comparisons

Both on same local debug engine and four-part-safe asset but **different**
live runs, not fixed camera/game-state repeatable benchmarking; values
are CPU wall times, not Vulkan timestamped GPU FPS.

| Metric | Before | After |
|---|---:|---:|
| Post-warmup samples | 742 | 808 |
| Avg CPU frame | 21.514 ms | 13.653 ms |
| Median CPU frame | 20.133 ms | 12.159 ms |
| P95 CPU frame | 28.023 ms | 20.761 ms |
| Avg script phase | 15.236 ms | 7.096 ms |
| Avg script provider stage | 11.012 ms | 3.092 ms |
| Input-dt frame cadence proxy | 46.09 FPS | 72.41 FPS |

The average CPU frame-time reduction is about **36.5%**; median about
**39.6%**. Less per-frame immutable data is directly confirmed in
post-change log by `project_bytes=0`, typical
`delta_payload_bytes=12623..22714`.
Some rare streaming/renderer spikes remain, including a ~379 ms worst
frame in one post-change sample (not removed by project cache).

Profiles:
- `NewViso/cache/profiles/firstfps-safe4-summary.json`
- `NewViso/cache/profiles/firstfps-projectcache-summary.json`
- `NewViso/cache/profiles/firstfps-projectcache.ulog.ndjson`

## Bullet + Endorphin provider test (actual)

The executable acceptance harness:
`python scripts/verify_endorphin.py --profile debug --scenario functional_control --run-tag perf-20261024`.

Passed: yes, exit 0, provider resolved
`engine.physics.bullet` (not Gravitas). Evidence:
- active physical motors, authored joint anatomical limits,
  localized tissue trauma with motor capacity reduction,
  damage persistence after ped body retirement,
  armor absorbing hits without traumatic muscle damage,
  death disabling all physical drives,
  reset cleanup and skeletal rigid-body integrity.
- 18 physical control updates; post-injury left/right local motor capacity
  ratio about 0.708.
- Maximum recorded native anchor separation about **0.144 mm**.
- This is the *functional_control* scenario; physical standing recovery
  (`physical_balance`) is **not** asserted by it.

Acceptance report:
`NewViso/.newviso/cache/endorphin-functional_control-perf-20261024-proof/acceptance.json`.
Project fixture is disposable, provider explicitly requires Bullet.
Its Chronicle ULOG has stage trace entries.

**Not a mass-ragdoll FPS benchmark.** This validates functionality of a
single articulated actor but not the scaling to 8/32/100 agents. An
independent count-scaled, fixed-scene load harness with active ragdolls,
collision traffic and distinct warmup windows is required.

## Verification

- `cargo test -p newviso-scripting --lib`: 13 passed.
- `cargo test -p newviso-runtime --lib`: 157 passed, 1 manually ignored.
- `cargo test -p newviso-project --lib`: 19 passed.
- `cargo test -p newviso-semantic-assets --lib`: 21 passed.
- Ped cooker `--bin newviso-rsc7-ped-cooker`: 5 passed.
- **215 unit tests passed**, 1 opt-in ignored; no failed tests.
- `cargo build -p newviso -j 2`: successful debug build.
- Rustfmt of touched Rust files completed.
- `git diff --check`: passed (line-ending warnings only).
- FirstFPS 1350 platform frames exit 0, 908 trace entries, no errors.
- ProceduralMap 180 platform frames exit 0, no errors.
- Bullet Endorphin native acceptance passed.

## Next work

1. Reverse the *actual* RSC7 accessory skin format and restore missing
   component with precise palette and weight mapping, backed by imported
   vertex tests and visual comparison.
2. Measure CPU frame/VM stage distribution under identical camera paths
   and in Release, at realistic count of NPCs and vehicles; isolate
   project-cache effect from streaming and renderer jitter.
3. Profile `apply_scene_streaming_residency` and first-use GPU skin
   upload + graph recording; prioritize largest p95/p99 stalls.
4. Implement multi-agent Bullet+Endorphin scaling acceptance with
   1/8/32/64 ragdolls, same scene, fixed collision density, renderer
   LOD enabled vs disabled, and budget/quality tolerance assertions.
