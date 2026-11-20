# NewViso Performance Benchmark V2 — 2026-11-11

## Scope

A reproducible, **gameplay-only** CPU/GPU baseline for `KaylaNightCity`.
Do not report FPS from a menu, platform loop, loading screen, or GPU
submission wall time. HLOD and advanced render-graph changes are **not**
implemented by this benchmark instrumentation.

## Opt-in runtime mode

The profiling launcher supplies both environment gates:

- `NEWVISO_PERF_TRACE=1`
- `NEWVISO_BENCHMARK_AUTOSTART=1`

Only when **both** are enabled does the application request the project's
normal Play/map-residency path automatically. It also skips the intro movie
and ignores persistent save slots, so test sessions neither restore nor
overwrite user gameplay saves. Regular launches are unaffected.

The launcher also sets `NEWVISO_PERF_TRACE_INTERVAL=1`. With
`--gpu-timestamps` it enables Vulkan timestamp diagnostics and polls the
renderer inspector every tenth traced host frame. This does not guarantee
a GPU sample exists; lack of queries must be reported explicitly.

## Build and run

From `C:\Users\Kayla\Documents\NorthStar\NewViso`:

```powershell
cargo check -p newviso-runtime --offline -j 2
cargo test -p newviso-runtime --lib --offline -j 2
python scripts/test_profile_tooling.py
python scripts/analyze_frame_trace.py --self-test
cargo build -p newviso --release --offline -j 2

python scripts/profile_live_runtime.py `
    --project ..\Projects\KaylaNightCity `
    --frames 6000 --timeout 180 --warmup 180 `
    --min-gameplay-samples 500 --gpu-timestamps
```

A correct runtime test produces a JSON report and a separate stderr log
under `NewViso/cache/profiles`. Reports include `benchmark_valid`,
`benchmark_issues`, `benchmark_autostart_confirmed`, warmup coverage,
CPU stage distributions and optional completed GPU query distributions.
Exit code **2** denotes an invalid benchmark even if the executable
itself exited with code 0. Never reinterpret it as an FPS result.

## Timestamp/frame semantics

- `frame` = host gameplay frame identifier in the CPU trace.
- `render_frame` = scene-renderer counter, independent from host frame.
- `app_frame` = host frame when inspector data was *observed*.
- `gpu_frame` = frame of the completed Vulkan timestamp query. Queries
  may lag; never equate `app_frame` and `gpu_frame`.
- `scene_render_wall_ms` and `render_provider_wall_ms` are **CPU** wall
  clock timings that may include GPU backpressure; neither is GPU time.
- `gpu_completed_queries.metrics` uses unique GPU-frame samples, not the
  number of inspector polls.

For meaningful p95/p99, use a warm, resident scene and many continuous
gameplay frames. Report query sample counts with every GPU percentile.
An empty GPU result is "not measured", **not** zero GPU time.

## Regression checklist

1. Cold start: record elapsed time from launch to map residency ready and
   to the first gameplay CPU trace.
2. World streaming: repeat a deterministic route on Highway 101 and into
   the city; compare loaded sector counts, queued tasks and RAM/VRAM.
3. Collision: verify bus wheels and pedestrian capsule on real surfaces
   while moving, turning, fast-travelling and returning.
4. Frame pacing: compare median/p95/p99 of `total_ms` and Vulkan GPU
   phases over identical cameras and identical release binaries.
5. Run with and without renderer parallel-recording enabled; check output
   fidelity, Vulkan validation and CPU/GPU time independently.
6. Repeat a safe unload/shutdown run and confirm project save files were
   neither restored nor modified by the benchmark.

**Limitations:** A bounded autostart run tests the project startup and
whatever gameplay runs without user input. It does **not** automatically
exercise a three-kilometre bus route or fast travel; those require a
separate deterministic input/scenario harness. A build, a clean test
suite, or 900 renderer-only frames do not establish 60/120 FPS.
