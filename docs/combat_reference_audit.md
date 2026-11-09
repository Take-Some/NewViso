# Combat reference audit — NorthStar / NewViso
Date: 2026-10-17
Reference root: C:\Games\gta v source code\GTAV Source\src\dev_ng\game
Scope: reference-only inspection. Do not copy proprietary implementation into NewViso.

## Verified reference subsystems
| Reference | Observed subsystem | NewViso equivalent | Gap |
|---|---|---|---|
| task/Combat/TaskCombatMelee.h | Melee finite-state machine: pursue, orient, chase, retreat, wait/decide, perform action, swap weapon | newviso-agent/peds/melee.rs + combat.rs | No explicit melee tactical FSM or action graph |
| task/Combat/TaskCombatMelee.h | StrikeCapsule geometry (start, end, radius, material) | newviso-runtime/application_peds/melee.rs | Approximate line-of-sight, target cone and distance; no swept limb/weapon collision volume |
| task/Combat/TaskCombatMelee.h | Active/support/observer melee combatant queue types | newviso-agent/peds/combat.rs | No crowd attack-slot scheduler |
| Peds/Action/BrawlingStyle.psc | Target radius, attack pacing ranges, combo continuation, block/counter probabilities, taunts | MeleeProfile (timings, cost, guard, combo) | Tuning is per-actor but misses behavior-driven decisions and counter windows |
| Peds/HealthConfig.psc | Health, armor, endurance, hurt/injured/dying/fatigued thresholds | PedProfile health/armour, MeleeProfile stamina, generic reactions | State thresholds and severity-dependent reactions not wired |
| task/Weapons/Metadata.psc | Timing for bullet reactions, aim transitions, weapon swapping | firearm ballistic and pose pipeline | No complete target-aware gun task/cover/feedback parity |
| task/Combat/Subtasks/* | shoot-at-target, advance, charge, search, cover and chase subtasks | native combat visibility, search-last-seen, firearm attack | Tactical hierarchy substantially simpler |

## Runtime state verified
- newviso-agent melee: one timed attack impact, guard facing/stamina, combo-step and expiration.
- newviso-runtime melee: source/target validation, range/cone, obstruction, fixed body impact point, hit zone torso; applies impulse and event.
- newviso-runtime physics/damage: ballistic collision routed to ped vitals and directional hit reactions/ragdoll.
- newviso-runtime hit_zones: contact-height approximation, optional explicit hit_zone; **not** bone-accurate.
- All projects share engine implementation; WeaponLab is the opt-in test map.

## Implementation work packages (do not mark complete without tests)
1. **Attack data model:** distinct bare-fist/knife/blunt/heavy/long-weapon styles; weapon inventory binding; damage types, animation sets, collision reach, startup/active/recovery frames, stamina.
2. **Anim-to-physics contacts:** derive world-space hand or melee-weapon swept capsules each animation tick; nearest valid victim, material hit, obstruction priority; no center-to-center proxy.
3. **Skeletal hit receivers:** named bone transforms with body-part colliders and per-LOD hierarchy; map physics receiver to head/neck/torso/limbs; preserve impact points for ragdoll.
4. **Damage resolver:** single HitEvent with weapon kind, attacker, victim, hit bone/zone, material, impact normal/impulse and flags; armor coverage, directional block, critical damage policy; no duplicate health application.
5. **Reaction graph:** choose stagger, recoil, partial physical response, knockdown, get-up, active Endorphin and ragdoll based on impact/body region/velocity/health.
6. **Tactical melee AI:** active/support/observer slots, distance management, attack/counter/block/feint, react-to-hit, hearing, visibility, cooperative threat selection.
7. **Ranged AI:** seek cover, strafe, suppression, peeking, reload, ammunition inventory, friendly-fire obstruction, distance-dependent spread.
8. **Animation/weapon data:** inspect permitted local animation clips, weapon skeleton sockets and material data; avoid placeholder animations masquerading as parity.
9. **WeaponLab acceptance:** real-time playback capture plus scripted tests for head/limb contacts, blocked/rear hits, knife/blunt combos, allies in line of fire, cover, interrupt, lethal and nonlethal Endorphin.

## Quality gates
- cargo test -p newviso-agent --offline
- cargo test -p newviso-runtime --offline
- scripts/verify_melee.py (native fixture)
- scripts/verify_combat.py (native fixture)
- scripts/verify_boundaries.py
- WeaponLab visual inspection in first and third person.
- No claim of complete GTA V behavioral parity before all above and observed crosschecks.

## Weapon release reference inspection (2026-10-19)

The following source was read directly; the implementation remains independently authored.

| Source location | Verified behavior | Applied boundary |
|---|---|---|
| `Peds/Action/ActionFlags.psc:215-216`, `ActionManager.h:335-336` | Reaction data distinguishes self drop and opponent disarm. | Release is an explicit common action, separate from anatomical damage. |
| `task/Combat/TaskCombatMelee.cpp:5851-5859,7148-7199` | Reaction/action attributes drive weapon handling; object event tags select the release frame, with an immediate fallback for an inactive network. | Checked action-event entry point; no universal HP trigger. Original clip event import is still outstanding. |
| `task/Combat/TaskToHurtTransit.cpp:98-135` | Hurt transitions consume object Create/Release tags; long-weapon handling differs from pistol handling. | Equipment and support data determine what can be retained; no weapon-ID branches or conjured backup guns. |
| `task/Movement/TaskGetUp.cpp:2478-2519` | Physical/hurt recovery coordinates dropped long weapons and selection of available sidearms. | Physics reset owns its body lifetime; new equipment may not be processed against the retired ragdoll. Automatic sidearm selection/retrieval is not claimed implemented. |
| `weapons/inventory/PedWeaponManager.cpp:982-1147` | Dropping validates equipped objects, activates detached physics, removes inventory ownership and performs an obstruction probe. | Generation-checked ownership transfer, pose/momentum seeding and bounds-derived placement projection. |
| `pickups/PickupManager.cpp:932-1240` | Pickup construction uses current weapon matrix, starting velocities, weapon ammunition, components/tint and last-owner information. | Native pickup provenance, physical pose and generic instance payload, including ammunition. Component-specific live attachment transfer still requires the component system. |
| `weapons/WeaponDamage.cpp:4405 onward` | `CanDropWeaponWhenShot` returns false in the enabled preprocessor branch. | Do not misrepresent direct shot-out-of-hand behavior as an enabled reference mechanic. |
| `task/Combat/TaskDamageDeath.cpp:1466-1538` | Death release has explicit policy/equipment handling and removes NPC weapon inventory. | Death uses the same ownership handoff and cannot duplicate previously dropped equipment. |

The native adaptive model evaluates weapon wrench against available hand support and
integrates physical grip slip. Its coefficients are validated Shared assets; its formula
and numerical tuning are NewViso design, not extracted GTA constants.
The action-event hook does not constitute full animation tag decoding, tactical combat
parity, skeletal live collision or a complete Endorphin hand/muscle solver.

## Endorphin extraction (2026-10-19)

Read-only review of `TaskNM.cpp`, `TaskNMBalance.cpp`, `TaskNMShot.cpp` and
`TaskGetUp.cpp` confirmed the separation of physical behavior/control, balance
feedback, component impulses, moving-floor context and recovery to animation.
The current NewViso step implements an independent `newviso-endorphin` core and
a host adapter for rig construction, conserved rigid-body momentum, live passive
articulation and contact/energy/clearance-based recovery. Its parameters are
authored VFS data. It does not reproduce an extracted NaturalMotion controller.

See `endorphin.md` for the implemented ownership boundaries and verification.
Active muscles, protective stepping/bracing/reaching, asymmetric anatomical
joints, fully pose-matched recovery and upright skeletal colliders remain gaps.
The StrikeCapsule, cold-weapon and tactical Combat Director work packages above
are not completed by this extraction.
