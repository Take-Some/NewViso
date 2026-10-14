# WeaponLab: modularity and measured frame cost

Date: 2026-10-14. Runtime: NewViso release, Vulkan/Bullet, the existing NorthStar workstation. Window: 1440 × 900. Fixed daytime scene with nine targets and sixteen physical fragments per target.

## Ownership and module boundaries

| Entry file | Before, lines | After, lines | Extracted responsibilities |
|---|---:|---:|---|
| first_scene/animation_skinning.rs | 2452 | 490 | clip sampling, affine transforms, joint attachments, frame scheduling, persistent workers, tests |
| first_scene/asset_models.rs | 1969 | 473 | material interpretation, vertex decoding, triangles, bounds, fragment geometry and state, tests |
| first_scene/scene_state.rs | 1300 | 67 | weather, camera settings, snapshots, particles, transient overlays, physics geometry |
| gameplay/weapons.ysc | 485 | 185 | definitions, ammo/reload transfer, ballistics, effects, presentation, shared types and math |

Existing public Scene3dRuntime methods and imports from gameplay/weapons.ysc are retained. The split changes ownership of implementation; it does not introduce a second weapon implementation.

## Runtime changes

- WeaponLab owns its HUD through engine_player.hud_enabled=false. The shared player retains movement, inventory, weapon presentation and UI bindings. Other projects retain the default HUD when this optional setting is absent.
- HudPresenter retains the current scene overlay until its displayed content or dimensions change. The cache includes ammunition, reload progress, aim/view state, hit feedback, focus, notices and round statistics.
- WeaponLab lifecycle modules opt into the existing immutable runtime delta transport.
- Dense array changes collapse into a single array replacement only when at least half of its elements changed and the actual serialized replacement is no larger than the individual operations. Sparse descriptor edits retain their narrow paths.
- Serialized size comparison uses a counting writer instead of allocating temporary byte buffers.

## Measurements

The same workload was run before and after: three-second warmup, 150 idle frames, destruction of every target, 200 frames with 144 fragments, target restoration and 150 restored frames. Native per-frame profiling used NEWVISO_PERF_LOG_EVERY_FRAME=1 in both runs. Results describe this workload on this workstation, rather than a universal FPS estimate.

| Phase | Median before, ms | Median after, ms | Reduction | P95 before, ms | P95 after, ms |
|---|---:|---:|---:|---:|---:|
| Idle yard | 14.33 | 5.98 | 58.3% | 14.58 | 7.86 |
| 144 physical fragments | 42.61 | 32.08 | 24.7% | 57.98 | 39.86 |
| Restored yard | 14.35 | 5.94 | 58.6% | 14.64 | 10.09 |

Script invocation median: idle 9.46 → 1.88 ms; fragments 22.56 → 15.62 ms. Frame cost includes native simulation, script invocation and command handling, streaming and rendering. Physics step count follows real frame time, so these figures measure the complete application workload.

Raw results and logs: Projects/WeaponLab/.newviso/cache/refactor-before.json and refactor-after-final.json.

## Validation

- Release scene library: 165 tests passed, three existing tests ignored.
- Release scripting library: 12 tests passed, including dense arrays, sparse descriptors, signed zero, null/removal/type changes and retry revision handling.
- Release application build completed.
- WeaponLab functional scenario checks the three weapons, cooldown, ammo consumption, reload/audio/animation commands, focus gating, switch cancellation, all nine targets and 144 bodies, fragment motion/rotation, restoration, ammo refill, attached hit-mark motion/rotation/cleanup, retained HUD and HUD updates on focus loss.

## Commands

From the NorthStar root:

```bat
Scripts\Run\RunWeaponLab.cmd
python Projects\WeaponLab\tools\verify_weapon_lab.py
python Projects\WeaponLab\tools\verify_weapon_lab.py --status
python Projects\WeaponLab\tools\profile_weapon_lab.py refactor-run NewViso\target\release\newviso.exe
```
