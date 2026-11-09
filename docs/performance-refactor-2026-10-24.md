# NewViso performance and modularization pass
Date: 2026-10-24

## Scope and change policy

This is an incremental, test-gated refactor on the current **NewViso**
sources, not the obsolete NewEngine tree. The working tree contained many
pre-existing uncommitted changes and they were not reset or replaced.
No runtime plugin DLL was installed during this pass.

## Baseline and structural inventory

- 476 Rust source files under `NewViso/crates`.
- 47 Rust files above 24 KiB.
- Largest monoliths in the inventory before this pass:
  - `newviso-runtime/src/application_vehicles/audio_fx.rs`: 86,991 bytes / 1,965 lines.
  - `newviso-vehicle/src/damage.rs`: 71,257 bytes.
  - `newviso-rsc7-import/src/lib.rs`: 66,235 bytes.
  - `newviso-vehicle/src/runtime.rs`: approximately 60.7 KiB.
  - `newviso-runtime/src/application_vehicles/damage_presentation.rs`: 61,464 bytes.

Only the relevant runtime audio/gameplay monoliths were changed in
this first pass; the remaining large files are candidates, not completed
refactors.

## Modularization

### Vehicle audio

`crates/newviso-runtime/src/application_vehicles/audio_fx.rs` was
divided at the existing function boundaries, preserving the engine's
`EngineApplication` methods and existing public behavior.

- `audio_fx.rs`: vehicle sound configuration model, diagnostics and
  provider voice helpers.
- `audio_fx/events.rs`: vehicle reference events.
- `audio_fx/runtime.rs`: per-vehicle synchronization.
- `audio_fx/configure.rs`: script-settable vehicle audio configuration.

The original ~1,965-line source becomes a main file of approximately
432 lines plus three cohesive child modules. No ABI, XVAG format, or
script-operation renaming was introduced.

### Script audio

`crates/newviso-runtime/src/application_script_commands/audio.rs`
was divided from ~605 lines into a core dispatcher/service adapter
(~245 lines) and:
- `audio/loops.rs` for named looping voices.
- `audio/characters.rs` for character motion sound sampling and playback.
- `audio/physics.rs` for material dispatch and contact-audio routing.

All existing `audio.clip.*`, `audio.endorphin.*`, `audio.physics.*`,
legacy compatibility, listener, route and loop commands keep their existing
behavior. These are internal Rust modules, not new runtime providers.

## Hot-path optimizations

### Endorphin acoustic lookup

`application_endorphin/audio.rs` formerly used
`BTreeMap<(String, ImpactMaterial), Vec<String>>` and allocated a new
String for each candidate (up to six candidates per collision).
It now indexes by the normalized region first:

```rust
BTreeMap<String, BTreeMap<ImpactMaterial, Vec<String>>>
```

This permits borrowed `&str` lookups, exact-part/region/body fallback
and no composite-string allocations per contact. Known lowercase
rig identifiers are held as borrowed `Cow<str>` values, and the ephemeral
limb-to-entity contact index is a `HashMap`. It no longer over-reserves
memory for all *inactive* pedestrians when only a few ragdolls are active.

Additional unit tests cover case folding, region fallback, invalid
regions, XVAG sample-bank admission and removal.

### Bullet contact-audio projection

`application_physics/frame_events.rs` now performs:
1. Contact begin/persist filtering.
2. Hash coalescing by canonical body pair + surface, keeping the
   highest impulse.
3. Deterministic key ordering.
4. Relative velocity / effective mass / material projection **once per
   selected contact** rather than for every solver-manifold sample.

No damage threshold was introduced. The coalesced event contract,
normal-speed semantics and stable output order remain unchanged.
A deterministic repeated-contact test covers pair/surface ordering and
strongest-impulse selection.

## Reproducible performance measurement

An ignored, opt-in Rust microbenchmark is available at
`crates/newviso-runtime/src/application_physics/frame_events/benchmarks.rs`.

Run:

```text
cd C:\Users\Kayla\Documents\NorthStar\NewViso
cargo test -p newviso-runtime --lib compare_audio_contact_coalescing_legacy -- --ignored --nocapture
```

It runs a preserved in-test reference implementation and the optimized
code with identical synthesized physics frames:

- 24,500 synthetic physics contact samples.
- 700 unique body-pair/surface groups.
- 20 repeated full iterations after warmup.
- Same per-contact projected impulse/velocity/mass verified by assertions.
- **Measured** on this development machine: original 27.508 ms,
  refactored 12.430 ms, **2.21× faster** for this test.

This is a single microbenchmark, subject to hardware noise, allocator,
compiler and frame-population differences. It is **not** a claim of
2.21× engine FPS or of any GPU frame-time improvement.

## Verification

- `cargo check -p newviso-runtime`: pass.
- `cargo test -p newviso-runtime --lib`:
  **155 passed, 1 ignored** (profiling-only benchmark).
- `cargo test -p newviso-vehicle --lib`: **96 passed**.
- `cargo test -p newviso-audio-api`: **12 passed**, doc tests pass.
- `git diff --check` on touched tracked code: pass.
- Targeted `rustfmt` completed.
- No end-to-end game render or device-audio FPS trace recorded.

## Next profiling-directed priorities

1. Instrument full-frame CPU scheduling, physics, render preparation,
   animation skinning and plugin service calls with release build traces.
2. Refactor the remaining vehicle damage state/transaction monoliths
   using data ownership and independent tests.
3. Audit per-NPC `Endorphin` active-ragdoll costs, especially world
   collision queries, articulated solver substeps and lod admission.
4. Analyze `newviso-rsc7-import` and streaming decoder stage
   allocations, cache keys and parse repetition.
5. Profile GPU draw/indirect submission, scene visibility/occlusion,
   streaming uploads, mesh skinning and materials at actual map scale.
6. Only change scheduling parallelism and lock/allocator strategy after
   collecting sample-based contention/latency evidence.

Do not interpret a successful build as proof of full-game performance.
