# Pedestrian behavior port

Character physical reactions and ragdoll behavior belong to Endorfin.
The shared pedestrian system lives at `Shared/Content/scripts/newviso/world/peds.ysc`.
It uses the existing `newviso-agent`, `newviso-world`, `newviso-navigation`,
`newviso-steering` and `newviso-character` layers. Project scripts do not own
NPC perception, locomotion or hit detection.

## Source correspondence

Reference root: `C:\Games\gta v source code\GTAV Source\src\dev_ng\game`.

| Original responsibility | Implemented boundary | Scope |
| --- | --- | --- |
| `Peds/PedIntelligence`, event scanner | `newviso-agent/peds.rs` | Hearing, event priorities, reaction policy, bounded event memory |
| `task/System/TaskTreePed`, `TaskManager` | `newviso-agent/behavior.rs` | Primary/ambient/reaction arbitration, interruption, child sequences, cancellation |
| `task/Movement/TaskGoTo`, `TaskGotoPoint` | Coordinate world travel + resident navigation | Collision-safe motion to positions without synthetic graph nodes |
| `task/Default/TaskWander` | Wander task | Deterministic area selection and pauses; original road/population wandering is not reproduced |
| `task/Response/TaskFlee` | Flee task | Threat actor/position, safe distance, duration and resume behavior |
| `task/Movement/TaskMoveFollowEntityOffset` | Follow task | Distance maintenance and bounded replanning |
| `task/Scenario/TaskScenario` | `Ped.scenario` + world reservations | Exclusive point ownership, approach, heading, animation, release |
| `Peds/PedType.h` | Stable `PedType` identifiers 0–29 | Source classifications for civilians, gangs, services, animals and players; service AI is not implied |
| `Peds/Relationships`, `RelationshipEnums.h` | Ped relationship rules | Respect, like, ignore, dislike, wanted, hate, dead |
| `Peds/HealthConfig.psc` | Ped profile/vitals | The schema's default health of 200 and armour of 0; damage, invulnerability, death |
| Ped weapon actions | Combat task + native backend ballistics | Range, cadence, physical obstacle occlusion and dynamic capsule damage |
| `Peds/Ped.cpp::KillPedWithCar`, `task/Physics/TaskNM.cpp`, `Metadata.psc` | Native ped body + vehicle response | Weak bump, RAMMEDBYVEHICLE, RUNOVERBYVEHICLE, directional fall, delayed safe recovery |
| Vehicle wheel/body contact | Native wheel telemetry + suspension reaction | Physical downward load and sustained crushing damage on a pedestrian under a wheel |
| `CTaskFallOver`, `CTaskFallAndGetUp`, `CTaskGetUp` | Native reaction state machine + Shared clip catalog | Directional knockdown, held floor pose, grounded recovery, interrupted get-up |
| `TaskNMFallDown` airborne/landing transitions | Native gravity and ground contacts + skeletal clips | Fall from height, soft landing or hard landing followed by recovery |
| `TaskNMShot`, `EventDamage` | Actual vitals change + additive pose layer | Original directional wounded clips without replacing the underlying animation |
| `CTaskDyingDead::Start_OnUpdate`, `TaskRageRagdoll` | Generic articulated body rig + optional provider joints | Death transfers the current pose and motion to native bodies; corpses retain contact and projectile queries |

These implementations adapt the behavior to NewViso's Rust/TypeScript boundaries.
They are not a complete binary-compatible implementation of every original task.

## Tasks and lifetime

Supported descriptors: `idle`, `wait`, `travel_to_node`, `go_to_position`,
`follow_actor`, `flee`, `wander`, `look_at`, `play_animation`, `sequence`, `combat`.
Sequences accept 1–64 children and at most eight nesting levels. Reaction tasks
cancel the previous travel owner; interrupted movement resumes from its current
physical position. Replacing or clearing an active task also cancels its motion.
Completed and failed tasks are visible in the agent snapshot and emit
`agent.task.lifecycle` events.

Coordinates are stored directly in world travel records and are checkpointed.
Physical actors use resident navmesh, steering, grounding and collision sweeps;
authored static cube floors generate exact transformed navigation triangles,
with tile caching and retirement rather than path invalidation every frame.
Distant actors retain world existence and use the cheaper world travel layer.
Health/armour/death are recorded as world facts and restored on registration.
Shutdown releases presentation/controllers while retaining authored world actors.
Scenario reservations carry their agent/task owner in world payloads. The Shared
frame lifecycle reconciles them with native task/death state, so leases also end
for calls originating in a separate project script module. Completion, failure,
preemption and explicit task cancellation release ownership; scenario instance
identifiers keep queued cleanup from removing a later reservation.

Ped locomotion faces actual horizontal motion, including collision sliding and
sharp direction changes. While walking/running, smoothed steering headings and
combat/look intents cannot rotate the body away from its movement. Stopped peds
retain heading and may turn for look/scenario actions. Reduced-tier presentation
uses world velocity after physical authority is released.

The Shared facade corrects converted GTA model forward (`-Z`) with a 180-degree
model yaw offset. `modelYawOffsetDegrees` overrides that asset convention for a
different model. `headingDegrees` initializes world facing independently and
does not bias every later walking direction.

## Commands and events

Commands: `ped.upsert`, `ped.remove`, `ped.damage`, `ped.relationship.set`,
`agent.tasks.clear`, and the existing `agent.task.set` interface.
`ped.upsert` requires an existing world actor and an agent bound to that actor.
Use `reset: true` to create fresh vitals; ordinary profile changes retain damage.

Snapshots expose `runtime.peds.peds`, each with `profile`, `health`, `armour`,
`dead` and `last_damage_source`. Events are `ped.damage`, `ped.death`,
`ped.reaction`, `ped.bullet.hit`, `ped.vehicle.hit`, `ped.vehicle.recovered`,
`ped.weapon.fire`, `ped.hit.reaction`, `ped.ragdoll.started`, `agent.task.lifecycle`.
Shots produce spatial `gunshot` stimuli. Event caches are bounded to 128 entries.

## Physical contacts and vehicle hits

A materialized full-tier ped owns a native dynamic capsule with 70 kg mass,
physical contacts and query participation, with the continuous-collision flag requested.
Bullet currently does not advertise native CCD support.
Its scene model uses that same entity; presentation itself is not an extra solid.
Native ballistics, including the ordinary Shared RMB/LMB weapon flow and NPC
shots, hit this capsule through the physics backend. Walls occlude damage.
Navigation supplies horizontal steering intent; native physics owns position,
gravity and contact impulses. Sweep depenetration never becomes walking intent.
The engine retires the body when the actor leaves the full tier or is removed.

The original distinguishes weak vehicle collision events, rammed-by-vehicle
and run-over-by-vehicle damage. This adaptation keeps those response states:
weak bumps pause motion briefly, strong impacts release the upright capsule,
and underbody/sideswipe or fallen-body hits select runover. Impact damage follows
actual contact impulse/closing speed, with a 30-health floor for an activated
strong hit. Direction controls physical tipping; rammed bodies can lift over the
hood, while runover carries them with 90% of vehicle velocity. These capsule
response formulas adapt the source policy to the available backend; original
per-archetype character-response activation forces are not reproduced in this
Endorfin adaptation.

Walking and combat fire are suspended while the body falls and gets up. Recovery
waits at least two seconds after the knockdown clip, requires grounded low-speed
motion, and is deferred while a vehicle occupies the space needed to stand.
For actors with reaction clips, the native capsule follows the recovery clip's
root orientation and rises gradually; normal movement resumes after the clip
finishes. Actors without a reaction catalog retain the capsule-only fallback.
Dead peds do not recover; `ped.upsert` with `reset: true` explicitly restores
health and body control.
Repeated resting floor or chassis contacts do not repeatedly apply impact damage.
Vehicle activation requires motion towards the ped along the impact direction.
Walking into a parked vehicle is a weak collision, not a vehicle ram.
Actual wheel contacts additionally push down on the ped with suspension load and
apply time-scaled crushing damage even when the vehicle is stationary.

`runtime.peds.physics.bodies` exposes body control, recovery seconds and the
current `vehicle_response` (kind, source, direction and wheel load), reaction
phase/clip/generation, actual skeletal playback time and selected joint positions. Physics
snapshots retain numeric entity IDs and supply exact `entity_key`, `source_key`,
`target_key`, `a_key` and `b_key` strings because scene IDs exceed JavaScript's
safe integer range. Scripts should use symbolic scene IDs for body commands and
these string keys for comparing contacts.

## Original skeletal reactions

`Shared/Content/scripts/newviso/world/ped_reactions.ysc` supplies a typed catalog
for the converted `s_f_y_hooker_01` skeleton. `Ped.spawn` chooses this catalog
for that model; `Definition.reactions` can supply compatible clips for other
models. The engine receives clip references and durations, without model names
or original-game asset paths in the Rust state machine.

`Scripts/Assets/cook_shared_ped_reactions.py` imports 21 original clips from the
installed game's `x64c.rpf`: `dam_ad`, `dam_ko`, `get_up@standard` and `move_fall`. It uses
the existing RSC7 character cooker and the Shared skeleton, writes four runtime
YCD dictionaries under `Shared/Content/animations/characters/gta/reactions`,
and retains source assets and a hash/duration manifest under `Shared/Source`.
Run it from the NorthStar root with the workspace Python executable:

```powershell
C:\msys64\mingw64\bin\python.exe Scripts/Assets/cook_shared_ped_reactions.py
```

The active catalog uses four directional `dam_ko` clips, matched standard
get-up clips, `fall_med`, `low_land_stand` and `land_fall`. The directional KO
clips provide the original authored fallback when procedural ragdoll is absent.
Impact direction is relative to the actor's world facing. Pairing follows the
authored final pose: `ko_back` ends face down and uses `get_up@standard@front`;
front/side knockdowns end on the back and use `get_up@standard@back`.

Reaction phases are `airborne`, `fall`, `down`, `get_up` and `land`. One-shot
knockdowns hold their final skeletal pose while lying. Native downward velocity
and ground contacts select soft or hard landing. Physical tipping also starts a
directional knockdown. Repeated wheel contacts do not restart the clip; a new
impact or death during get-up starts a new generation and seeks toward a fallen
pose according to the current torso tilt. A configured profile `death_clip`
is used by the animated fallback; an active physical ragdoll owns the skeleton.

Reaction clips disable mover translation and lock the root's horizontal
translation to the skeleton bind pose. Vertical root motion and skeletal
rotations remain animated; native physics owns world travel. The model is
anchored to the capsule's ground height without applying the capsule rotation a
second time to an already rotating skeletal clip. For a configured rig, authored
contact radii and the current blended skeletal pose also correct model height;
interrupted clip blends cannot leave hands or feet below the support floor.

## Wounded reactions and physical death

Surviving damage that actually changes health or armour starts one of the original
`dam_ad` front/back/left/right clips, relative to world facing. These dictionaries
contain additive transforms: the pose layer composes their local rotations with
the existing animation and preserves the base bone translations. Each reaction
lasts about one third of a second. Invulnerability without lost vitals does not
start a wounded layer. Native ballistics and `ped.damage` use the same transition.

`Shared/Content/scripts/newviso/world/ped_ragdolls.ysc` supplies the compatible rig
for `s_f_y_hooker_01`; `Definition.ragdoll` accepts a compatible rig for other
models. Runtime code receives generic joint names, shape radii, mass weights and
parent relations. At death, the engine builds 15 dynamic bodies and 14 native
point constraints from the current blended/additive skeletal pose. Total mass is
70 kg. Each body inherits linear and angular motion; projectile impulse is
applied once to the part nearest the hit. A small directional angular seed also
lets stationary zero-impulse deaths topple. Self-collision within this rig is
filtered while contacts with the world, player and vehicles remain active.

The provider solves socket positions with four native substeps and additional
iterations while joints are active. This rig does not break joints under wheel
loads. Inertia-aware implicit, equal/opposite torques add compliant angular cone
stops and damping; these are not the
original anatomical hinge limits or every original procedural reaction behavior. The skinning worker
uses physical bone transforms, including derived helper joints and captured
local poses of unspecified descendants. Animation and AI do not drive an active
corpse. Later bullets hit and push the actual limb collider without changing its
dead vitals. Reset, removal and streaming retirement remove child bodies and
constraints, and release physical ownership of the skeleton.

The optional `JointConstraints` feature is detected through provider diagnostics.
It does not enter legacy capability negotiation. If a provider lacks joints or
no compatible skeleton/rig is available, the engine retains the existing
capsule/animated fallback and retries handoff when the skeleton is installed.
`runtime.peds.physics.bodies` adds `hit_reaction`, `additive_playback` and
`ragdoll` snapshots with exact part keys, actual poses and socket separation.

## Shared usage

```typescript
import { Ped } from "./newviso/world/peds.ysc";
const ped = new Ped("worker.1");
commands.push(ped.goTo([12, 0, 8]));
commands.push(ped.follow("player.character", 2));
commands.push(ped.patrol([[12, 0, 8], [18, 0, 8]], 1.4, 1));
commands.push(ped.playAnimation("animations/work.ycd@idle", 10));
commands.push(...Ped.relationship("guards", "intruders", "hate", true));
```

`engine_peds.actors` in project variables contains model, locomotion dictionary,
feet position, optional root lift, group, wander radius and profile. The default
engine lifecycle installs the facade for every project. VehicleTest registers
three female peds using its existing converted model and animation dictionary.

## Explicit remaining limits

- No complete original ped model/component/variation catalog is imported.
- Live fall/get-up reactions use the native capsule and authored skeletal clips.
  Death uses articulated native bodies for a compatible rig and joint provider.
  Original bone damage multipliers, dismemberment, complete procedural physical
  reactions and the point-cloud
  get-up pose matcher are not implemented. Other models require compatible
  reaction clips and ragdoll definitions.
- Combat has functional attack intent and ballistics; original cover, melee,
  weapon selection/reload and full weapon grip/IK presentation are not reproduced.
- Original wanted/police/arrest, witnesses, ambulance/fire crews, animals,
  population zones, traffic-road wandering and all scenario action graphs remain.
- Ped vehicle entry/exit/driving tasks are not connected to the vehicle access
  state machine by this port.
- Scenario points and animation dictionaries must already exist. Original
  scenarios and their props/audio are not imported automatically.

## Verification

Unit coverage includes stable source ped types, restored vitals, coordinate goals, follow distance, coincident threats,
sequence preemption, replacement cancellation, armour/death, repeated/self
gunshots, combat fire cadence, retained scenario heading, descriptor validation, primitive floor navigation/cache retirement,
mirrored/rotated primitive geometry and coordinate checkpoint handoff.
`Projects/VehicleTest/scripts/verify_peds.ysc` exercises the actual script/native
boundary, physical navigation, gunshot reaction, capsule hits, obstacle occlusion,
lethal damage, vitals reset, armed combat, NPC ballistics, timed scenario sequences
and reservation release/reacquisition/cancellation. It also checks the actual
rendered model's forward vector against physical velocity on moving frames,
including flee and combat. It emits `PED_RUNTIME_VERIFY_PASS` on success.

`Projects/VehicleTest/scripts/verify_ped_physics.ysc` exercises actual native
capsules, the Shared RMB/LMB pistol path, torso damage, wall occlusion and misses,
vehicle/player contacts and physical pushing, surviving-impact recovery, death,
vitals reset and body cleanup. It emits `PED_PHYSICS_VERIFY_PASS`.
`verify_ped_runover.ysc` verifies a moving chassis ram, physical fall, a stationary
wheel contacting a fallen capsule, suspension load, crushing damage and safe
get-up after the car leaves and non-damaging contact with a parked car, followed by a lethal bullet during
a weak bump. It emits `PED_RUNOVER_VERIFY_PASS`.
`verify_ped_animations.ysc` verifies streamed original dictionaries, actual
skeletal limb motion, directional selection, non-looping held floor pose,
animated recovery with a rising collision capsule, fall from height, hard
landing, external tipping, a second physical vehicle impact during get-up, and
death interrupting recovery. It emits `PED_ANIMATION_VERIFY_PASS`.
`verify_ped_damage_ragdoll.ysc` checks four directional additive hits, preserved
bone lengths, surviving control, invulnerability, current-pose death handoff,
15 real bodies/14 native sockets, physical skeletal transforms, corpse/car
contacts, projectile impulses after death, reset and removal cleanup, and
scripted damage/death. It emits `PED_DAMAGE_RAGDOLL_VERIFY_PASS`.
Run `Scripts/Run/VerifyPedPhysics.cmd` for all four checks using release and the same
Shared Content/providers as the playable project.

Run `Scripts/Run/VerifyPeds.cmd` from the NorthStar workspace. It uses the same
Shared Content/providers as the normal engine launcher, builds release, and loads
`Projects/VehicleTest/verify-peds.project.json`. The normal project manifest and
its `scripts/main.ysc` entrypoint remain the ordinary playable scene.

Validated native suites:

```powershell
cargo test -p newviso-physics-api -p newviso-physics-client -p newviso-agent -p newviso-world -p newviso-runtime -p newviso-scene --lib --offline -j1
```

The engine suites cover contract validation, additive bone preservation and
coordinate handoff. The Bullet suite additionally checks a real constrained
body under gravity, small-limb angular stability and native constraint/body cleanup.

Validated on the installed Bullet release: 243 engine tests and 17 native provider
tests passed; one manual debris benchmark remains ignored. All six Release scene
checks passed: damage/ragdoll, original animations, runover, physical pedestrian
contacts, NPC runtime and weapon input. The original animation check verifies
actual blended joint clearance and death interrupting get-up.
