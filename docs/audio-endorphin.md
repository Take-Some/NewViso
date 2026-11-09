# NorthStar — Endorphin acoustic event integration
Date: 2026-10-24

## Scope

Endorphin body sounds are now driven by **real Bullet contacts**, not death
animations, authored pain signals, or health/damage transactions. The
NewViso host owns semantic event selection and physical provenance;
`engine.audio` remains responsible for XVAG decoding, spatial playback
and voice management. No legacy `NewEngine` SDK is required by this
change, and no change to the native audio DLL is implied.

### Code map

- `crates/newviso-runtime/src/application_endorphin/audio.rs` — Endorphin
  anatomy/ragdoll body-to-source mapping, XVAG sound banks and contact
  admission/priority/cooldowns.
- `crates/newviso-runtime/src/application_platform/frame.rs` — existing
  post-Bullet contact drain and generic sound routing. This now invokes
  Endorphin sound resolution first, then generic Physics Audio for
  remaining contacts.
- `crates/newviso-runtime/src/application_script_commands/audio.rs` —
  `audio.endorphin.bind` and `audio.endorphin.unbind`.
- `crates/newviso-runtime/src/application_script_commands.rs` — commands
  registered as engine operations (not ad-hoc JS calls to an audio device).
- `crates/newviso-runtime/src/lib.rs` — per-scene Endorphin audio catalog.

### Authored clip binding

Assign a single XVAG clip:

```json
{"op":"audio.endorphin.bind","region":"arm","material":"generic",
 "clip":"audio/characters/impacts/arm_01.xvag"}
```

Assign an XVAG variation bank (up to 16, repeated paths removed):

```json
{"op":"audio.endorphin.bind","region":"leg","material":"generic",
 "clips":["audio/characters/impacts/leg_01.xvag",
          "audio/characters/impacts/leg_02.xvag"]}
```

Disable a binding:

```json
{"op":"audio.endorphin.unbind","region":"arm","material":"generic"}
```

The above `audio/characters/impacts/*.xvag` paths are **examples**;
they have not been authored or verified to exist. Existing XVAG files
include `audio/footsteps/stone/land_01.xvag` and other footsteps,
weapon impacts, and bullet impacts. Do not reuse bullet impact
recordings for soft-body falls without sound-design approval.

For testing on stone-only surfaces a project *may* explicitly bind
`leg` / `generic` to a stone landing recording. This is not a
general-purpose production material assignment.

**Binding selection order:** exact limb + exact surface; exact limb +
generic; anatomical region + exact surface; region + generic; body +
exact surface; body + generic. Typical authored regions in
`Shared/Content/physics/endorphin/humanoid.json` are `arm`, `leg`,
`head`, `torso`, and `abdomen`. Individual limb names come from
the actual Endorphin rig, not from hard-coded height tests. Region
strings are short ASCII identifiers.

### Physics pipeline

1. Bullet finishes simulation, reports contact impulses, points,
   entity IDs and relative normal velocities.
2. The existing physics-audio extractor coalesces fixed-step contact
   peaks; no damage predicate is involved.
3. Endorphin maps active ragdoll body IDs to the actual rig part and
   anatomical region. Self-collisions within one rig are not voiced.
4. Authored XVAG bank resolution selects an asset; the event competes
   on normal-impulse / listener-distance importance.
5. Dedicated Endorphin `PhysicsAudioRouter` admits real impact episodes,
   rejecting resting support and duplicate solver contacts.
6. Audio Client calls `engine.audio/play_clip_json_v1` with the XVAG URI,
   3-D source position, actor scope, gain, pitch, and deterministic seed.
7. Generic physics-audio skips already-claimed Endorphin contact IDs so
   one impulse cannot produce two duplicated voices.

### Performance admission

- At most **2 accepted sounds per actor per frame**.
- At most **24 accepted Endorphin sounds per frame**.
- **32 overall** emitted Endorphin + generic impact sounds per frame
  under the current generic router (separate from existing steps,
  weapons, music and vehicle loops).
- Endorphin clip candidates beyond **90 world units** from the current
  focus point are dropped from this route.
- The contact episode cache is bounded by the audio API.
- No extra allocations or locking were introduced into the device callback.
  Semantic indexing and catalog lookups run on the simulation thread.

These are **gameplay event budgets**, not a global native mixer voice cap.

### Current limitations / next engineering work

- No authored Endorphin-specific body/cloth XVAG pack is installed yet;
  the new route is intentionally dormant until scripts bind real assets.
- Bullet contact sound materials still arrive as `generic` in the
  general extractor; wire authored surface material identifiers through
  the Physics API and asset metadata before relying on a metal/stone
  sound taxonomy.
- Active physical ragdoll contacts are covered. Procedural vocal effort,
  injury breathing, clothing drag, sliding/friction loops, muscular
  exertion, and recovery transitions require separate continuous event
  streams and sound assets.
- Native `engine.audio.native` output (HRTF, real voice virtualization,
  streamed XVAG spatialization and Audio Graph) needs independent
  device/latency validation. No native DLL has been rebuilt in this pass.
- Reconcile global voice budgets and listener position with an explicit
  audio-graph/voice-priority service rather than relying only on
  post-physics per-frame admission.

### Verification

`cargo check -p newviso-runtime` passed.
`cargo test -p newviso-runtime --lib` passed: 153 tests.
`cargo test -p newviso-endorphin -p newviso-audio-api
-p newviso-audio-client` passed: Endorphin 41 tests,
Audio API 12 tests, Audio Client 0 tests, plus doc tests.

No game-on-device listening claim is made.
