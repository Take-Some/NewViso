# NewViso full-frame performance profiling — 2026-10-24

## Scope and modifications

This pass extends actual runtime instrumentation and splits the vehicle damage
monolith. It works **only with NewViso**, not the obsolete NewEngine source
tree, and preserves existing uncommitted repository changes.

### Instrumentation

Opt in with:

```powershell
cd C:\Users\Kayla\Documents\NorthStar\NewViso
$env:NEWVISO_PERF_TRACE = "1"
$env:NEWVISO_PERF_TRACE_INTERVAL = "1"
.\target\debug\newviso.exe --project ..\Projects\FirstFPS --max-frames 600 2>&1 |
  Tee-Object cache\profiles\firstfps.log
python scripts\analyze_frame_trace.py cache\profiles\firstfps.log --json cache\profiles\firstfps.json
```

The launch command is a **reproduction command**, not a successful run.
The current FirstFPS startup is blocked by the missing YPT codec described
below; repair that blocker before claiming game FPS numbers.
`NEWVISO_PERF_TRACE_INTERVAL=30` is the default if omitted; use 1 when
measuring p95/p99 frame tails. The trace uses chronicle host logging.
For reliable comparisons, capture warm steady-state intervals separately
from startup/shader compilation, and compare equivalent scenes/cameras.
Remove environment flags after profiling.

Frame trace source:
`crates/newviso-runtime/src/application_platform/performance.rs`

Physics trace source:
`crates/newviso-runtime/src/application_physics/performance.rs`

Scene trace source:
`crates/newviso-scene/src/first_scene/renderer_frame.rs`

Analysis:
`scripts/analyze_frame_trace.py`. Supports `--self-test` and JSON reports.

### New metric groups

- Main frame: `dt_ms`, `total_ms`, `pre_physics_ms` (climate/input/UI), `physics_ms`, `cold_ms`, `scripts_ms`,
  `streaming_ms`, `interest_ms`, `pump_ms`, `publish_ms`,
  `scene_render_wall_ms`, `render_provider_wall_ms`.
- Bullet fixed-step timing (enabled only in trace mode):
  `endorphin_ms` (sum of control callbacks), `physics_build_ms`,
  `bullet_service_ms` (host/provider round-trip, including provider),
  `physics_post_ms`, fixed steps and *summed per-step* submitted
  bodies/joints/returned event counts.
- NPCs: number of active articulated Endorphin ragdolls and simulated
  world bodies.
- Asset streaming: queued/loading/resident counts and bytes, failures,
  whether memory budget was exceeded, model/collision preparation queue
  depth. The scene-preparation queue lock is only sampled on traced frames,
  not in the ordinary gameplay loop.
- Renderer internal stage trace: visibility, geometry/material GPU
  synchronization, geometry build, residency, frame-slot acquire, skin
  upload, primitive upload, total uploads, graph command recording and
  graph CPU time, visibility/resident counts, instance and Hi-Z counts.

**Measurement caveats:** these are wall-clock CPU/thread timings and
host-service call timings. They are **not GPU execution timestamps**.
`render_provider_wall_ms` or `acquire_wall_ms` can include frame
pacing/back-pressure. The FPS estimate derived from input `dt_ms` is
a frame-cadence proxy, not a verified presented-frame counter. Genuine GPU
time needs Vulkan timestamp queries. Probing every frame itself has a
cost, including Chronicle log output.

### Vehicle damage modularization

`crates/newviso-runtime/src/application_vehicles/damage_presentation.rs`
was a 1,373-line monolith. It is now a small dispatch/helpers/tests module
plus these cohesive child modules (method boundaries preserved):

- `damage_presentation/transaction.rs` — collision/damage transaction.
- `damage_presentation/detach.rs` — physical detached vehicle parts.
- `damage_presentation/fire.rs` — fire progression and presentation.
- `damage_presentation/explosion.rs` — explosion presentation/impulses.

Child methods remain callable from the existing engine and do not expose
new public service ABI. The damage transaction is still a substantial
single function; decomposition of its inner state machine needs explicit
invariant tests, not an unsafe mechanical split.

### Code validation

- `cargo build -p newviso -j 2`: passed (debug).
- `cargo check -p newviso-scene -p newviso-runtime`: passed.
- `cargo test -p newviso-runtime --lib`: **157 passed, 1 ignored**.
- Scene/render tests: **171 passed, 3 ignored**.
- Vehicle subsystem tests: **96 passed**.
- Together with runtime: **424 passed, 4 ignored**.
- `python scripts/analyze_frame_trace.py --self-test`: passed.
- `git diff --check` on touched tracked files: passed.
- Targeted Rust formatting completed.

The **architecture boundary checker remains failing** on direct `.xvag`
mentions in 5 source files inherited from earlier XVAG-first work:
`application_character_audio.rs`, `application_endorphin/audio.rs`,
`application_vehicles/audio_fx.rs`,
`application_script_commands/audio/characters.rs`, and
`newviso-audio-api/src/lib.rs`. These were not added by this
profiling pass; resolving the semantic-vs-container boundary is a
separate task.

### Attempts to measure real runtime FPS

Built the latest debug application and attempted bounded live runs:

1. `FirstFPS --max-frames 120`: **exit code 2**. Startup failed
   loading `content/particles/gtav/core.ypt` because no asset codec
   accepted `particle_dictionary.runtime_v1`. No stable frame trace.
2. `ProceduralMap --max-frames 120`: **exit code 2**. Startup/
   gameplay script invocation failed in
   `scripts/newviso/default_game.ysc::on_frame` because
   `enemies` was undefined. No stable frame trace.
3. No-project launch: **exit code 2**, as the current runtime is
   explicitly project-driven.

Failed-run output, **not performance samples**, can be found in:
`NewViso/cache/profiles/default-scene-2026-10-24.log` and
`NewViso/cache/profiles/procedural-map-2026-10-24.log`.

**No global game FPS/p95 frame-time or realized GPU speed-up is
claimed.** Fix startup blockers, retest Release build with actual
gameplay workload, capture a 600–3,000-frame series, and compare
identical camera paths before/after engine changes.

### Remaining optimization priorities

1. Repair YPT codec availability and gameplay script bootstrap
   to obtain real live traces for representative projects.
2. Profile Bullet backend internals / host IPC separately from
   Endorphin controller time and physics packet construction.
3. Add GPU Vulkan timestamp queries and pipeline statistics
   where supported, along with render-thread/GPU correlation IDs.
4. Trace streaming task queue latency and bytes decoded/uploaded.
5. Use collected p95/p99 stage samples to choose the next real
   optimization; avoid speculatively changing physics fidelity or
   animation update cadence without evidence.
