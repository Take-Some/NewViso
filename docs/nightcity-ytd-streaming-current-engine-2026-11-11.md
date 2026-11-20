# KaylaNightCity YTD streaming — current NewViso implementation

Date: 2026-11-11. Scope: NewViso only; no Legacy NewEngine SDK or source migration.

## Implementation

The runtime uses the existing contract chain:

- `newviso-resource-runtime` owns source resolution, residency, policy and worker scheduling;
- `newviso-semantic-assets` requests `assets.textures.entry_runtime_v1` from the current `engine.assets` service and converts NTRT v2 into `TextureResource`;
- active `engine.assets.starvault` provider owns physical YTD/NEF8 decoding.

Changes in the **current NewViso** workspace:

1. `crates/newviso-resource-runtime/src/streaming/loading.rs`: source-neutral per-container concurrency admission limit (two in-flight selectors per logical source), applied during priority-ranked candidate selection. This reserves worker capacity for independent textures, models and road collision assets instead of allowing one dictionary to fill eight worker slots.
2. `crates/newviso-resource-runtime/src/lib.rs`: raw byte cache is now LRU on hits, keeping frequently reused compressed dictionaries hot. BLAKE3 hashing happens outside the common cache mutex, avoiding long global lock hold while checking a ~191 MB compressed container. Oversize assets remain loadable but cannot remain permanently cached above the 256 MiB cache-byte ceiling.
3. `crates/newviso-resource-runtime/src/streaming.rs` and `src/lib.rs`: unit tests for fair scheduling, eventual entry residency, LRU source eviction, correct memory bound and oversize admission.

The generic asset address and `@entry` semantics, texture wire format, collision readiness and all map startup gates are unchanged. No DLL was replaced or Legacy source restored.

## Verification

- `cargo test -p newviso-resource-runtime --lib --offline -j 2`: 11 passed, 0 failed.
- `cargo test -p newviso-semantic-assets --lib --offline -j 2`: 23 passed, 0 failed.
- `cargo check -p newviso-runtime --offline -j 2`: passed.
- `rustfmt --check` on the three changed files: passed.
- `git diff --check` on changed files: passed.
- Current NewViso Release build: completed in 51.54 s.
- Actual bounded KaylaNightCity profile JSON:
  `cache/profiles/live-KaylaNightCity-20261111-133842.json`.

Run: 960 renderer frames, 27.47 s, process exit 0, autostart acknowledged,
**0 gameplay frames**, map not ready, world frame 0, physics tick 0,
70 resident assets, 217 queued, 8 loading, 0 failed.
Benchmark exit: **2** (invalid gameplay benchmark); GPU timestamps absent.

Eight startup snapshots (frames 120..960) showed **max 2 entries per source**
and **5 independently active logical sources** each time, with road collision
`.ybn` work observed in 5 snapshots. This directly verifies source-lane
admission and absence of total worker-slot capture by one YTD dictionary.

## Limitations and next benchmark gate

This fixes source-load fairness and repeated **compressed source-byte**
refetches due to FIFO eviction. It does **not prove** elimination of repeated
NEF8 decompression **inside the installed provider DLL**. That provider
still owns semantic decode and cannot be replaced by Legacy NewEngine code.
The old codec's separate 768 MiB source patch has not been deployed.

Do not claim FPS speedup, memory reduction, Vulkan p95/p99, complete map
readiness, vehicle traversal or proven collision correctness until gameplay
actually runs. Do not lower the collision/map readiness criteria to hide
the bottleneck.

Next: profile provider-level YTD decompress/cache hits using the supported
current `engine.assets` contract, resolve provider build against current
SDK, build/test/deploy only a compatible provider, then repeat the same
cold-start and steady-gameplay workload with CPU/GPU timestamps.
