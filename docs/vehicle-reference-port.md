# Vehicle reference port

Status: common vehicle state, damage integration and script events are implemented
and verified. Complete behavioral parity with the reference vehicle stack is not
yet achieved. This document tracks the completed slice and the remaining owners.

## Scope and ownership

Reference source: the user's local `GTAV Source/src/dev_ng/game` tree.
The audit selected 304 C++/header files and 467,916 lines across Vehicles,
vehicleAi, vehicle-related event declarations and their network dependencies.
These numbers describe the selected source inventory, not a completion percentage.

Generate the inventory again:

```powershell
python scripts/audit_vehicle_reference.py --source-root "C:\Games\gta v source code"
```

The report is `.newviso/verification/vehicle-reference/source-inventory.json`.
Every file initially has `review_status: inventory_only`; reviewing one behavior
does not mark an entire reference file as ported.

NewViso owns source-neutral vehicle state in `crates/newviso-vehicle`.
The application bridge in `crates/newviso-runtime/src/application_vehicles`
connects that state to scene transforms, physics, fragment geometry, audio,
particles and host events. Reusable script command factories and subscriptions
live in `Shared/Content/scripts/newviso/vehicles.ysc`. Project scripts supply
actors, controls and gameplay policy.

## Implemented and exercised

| Reference responsibility | NewViso owner | Current result |
| --- | --- | --- |
| Vehicle creation/removal notifications | vehicle runtime + events bridge | Creation is an edge; removal remains deliverable after the binding disappears. |
| Occupants and configured driver seats | runtime cabin/access + Shared access scripts | Canonical seat names, entry/exit/shuffle events, unique actor occupancy across vehicles, explicit player-driver context. |
| Alarm processing and triggering | vehicle runtime systems + cabin/audio | Automobile alarm runs once for 10–20 seconds, expires/disarms, drives flashing lights and alternating horn. |
| Engine temperature update | vehicle systems | Ambient cooling, damaged radiator factor, running-engine heating, airflow and fan hysteresis at 107/80.1 degrees. |
| Oil/fuel leakage and fire progression | damage + runtime powertrain | Shared frame processing runs for road vehicles, boats, planes, helicopters and submarines. |
| Infinite fuel | damage + ignition/runtime | Zero tank volume permits engine operation and never produces fuel exhaustion. |
| Player petrol-fire burn rate | damage frame context | Actual driver classification selects the reference player burn rate. |
| Damage proofs | vehicle damage policy | Independent bullet/collision/explosion/fire/melee/water flags and invincibility gate damage before health or presentation mutation. |
| Weapon and physics damage routing | script commands + platform + damage presentation | Vehicle targets enter one vehicle transaction; generic destructible health does not receive a second transaction. |
| Rollover engine shock | damage | An upside-down body impact above the reference shock threshold can damage the engine away from its normal region. |
| Authoritative health and tank setters | runtime systems | Body/engine/tank/oil changes update flags, driveability and transition events; the old tank setter uses the same path. |
| Explosion | vehicle systems/powertrain + presentation | One explosion transition per wreck, synchronized core health, fragments, debris, light/tyre damage, audio and particles. |
| Complete repair | lifecycle + core repair | Restores internal damage state, fluids, tyres, fragment visibility, dents and collision hulls; clears failed-start state and preserves existing world debris. |
| Model specifications | vehicle specification + native Shared XML catalog | 298 base model profiles; damage/control capabilities, electric/unpowered motors, fuel trailers and retained source masks. |
| Weapon damage and class gates | damage + reference handling | Weapon multiplier, aircraft fire factor, optional authored engine/tank regions and independent damage scales. |
| Burning engines and aircraft misfires | powertrain + class solver | Running player motor retains output during fire; player/AI driveability is separate; plane/heli health misfires affect motor output. |
| Event delivery | events bridge + Shared VehicleEvents | All 64 kinds publish through `engine.vehicle.<kind>`, with cause, part, actor, seat and spatial metadata; damage/profile snapshots and symmetric state end events. |

See [vehicle-specifications.md](vehicle-specifications.md) for exact capability
mapping, native catalog generation, defaults and script replacement semantics.

Previously implemented suspension, transmission, tyre slip, fragment articulation,
door dynamics, lights, audio and visual effects remain in their owners.
Their existence is not evidence of exact reference parity. The common changes
above were added to those working systems rather than replacing their state.

## Authored fragment physics

The 2026-10-04 fragment slice preserves physical group hierarchy, component IDs,
mass/inertia/centre of gravity, group break strength and damage tuning from PC YFT.
newviso-model and the semantic importer retain source-neutral metadata;
application_vehicles/fragment_body.rs forms independent assemblies and
fragment_damage.rs propagates committed group damage. Chassis hull IDs allow exact
component removal/restoration. Bullet's optional CollisionPairFiltering prevents
the initial source overlap from injecting an artificial impact.

See [vehicle-modular-damage.md](vehicle-modular-damage.md) for the shared cook
pipeline, three migrated models and the Release scenario. These mechanics retain
the existing parametric deformation, wheel proofs and latch/hinge systems.
Imported latch/stiffness/soft-angle values are preserved for subsequent constraint
work; they do not yet replace every procedural hinge formula.

## Script contract

Commands use an authored scene `id` or a resolved numeric `entity`.
Prefer authored IDs in gameplay. Payloads retain the legacy numeric entity fields,
and also include exact decimal `entity_key`, `actor_entity_key` and
`other_entity_key`. `sequence` is an exact decimal string because a vehicle u64
sequence exceeds JavaScript's integer precision.

Events are observations after committed state changes:
`phase: after`, `source: engine.vehicle`, `cancelable: false`.
Set proofs or gameplay policy before requesting damage. A health setter is
authoritative and is allowed to restore/change state independently of proofs.

When a vehicle transform is available, damage `position` and `normal` are
converted to world space. Original coordinates are retained as
`local_position` and `local_normal`. Normals use inverse scale, rotation and
normalization. Without a transform the payload explicitly reports
`position_space: vehicle_local`.

```typescript
import { Events } from "./newviso/events.ysc";
import { Vehicles, VehicleEvents } from "./newviso/vehicles.ysc";

let exploded = 0;
let subscription = 0;

export function on_start(): any {
  subscription = VehicleEvents.on("vehicle_exploded", event => {
    exploded++;
    // event.payload.entity_key and sequence remain exact.
  });
  return { commands: [
    Vehicles.alarm("test.rancher", true),
    Vehicles.proofs("test.rancher", { bullet_proof: true }),
  ] };
}

export function on_event(envelope: any): any {
  Events.dispatchIncoming(envelope);
  return { commands: Events.drainCommands() };
}

export function on_frame(): any {
  return { commands: [] };
}

export function on_shutdown(): any {
  VehicleEvents.off(subscription);
  return { commands: [] };
}
```

The facade also exposes `specification`, `health`, `damage`, `thermal`, `environment`,
`engine`, `gear`, `lights`, `seat`, `leave`, `repair`, `explode`
and `triggerAlarm`. `seat(..., actor_is_player)` explicitly supplies player
classification; core vehicle code does not hardcode an actor ID.

Command operations added in this slice:

- `vehicle.specification.set`
- `vehicle.health.set`
- `vehicle.damage_policy.set`
- `vehicle.damage.apply`
- `vehicle.alarm.set`
- `vehicle.thermal.set`
- `vehicle.environment.set`
- `vehicle.explode`

Existing fuel, cabin, occupant, light, tyre, part and lifecycle operations remain
available. Replaying an unchanged seat assignment does not emit another entry.
A cross-vehicle transfer emits departure on the old vehicle and entry on the new
one, and clears the old driver context and reservations.

## Event topics

Every name below is prefixed with `engine.vehicle.`.

| Group | Kinds |
| --- | --- |
| Lifecycle/occupancy | vehicle_created, vehicle_removed, specification_changed, occupant_entered, occupant_left, seat_changed |
| Alarm/controls/temperature | alarm_started, alarm_stopped, lock_changed, lights_changed, cooling_fan_started, cooling_fan_stopped |
| Engine/transmission | engine_started, engine_start_failed, engine_stopped, gear_shifted |
| Brakes/traction | brake_released, handbrake_applied, handbrake_released, skid_started, skid_stopped, wheel_spin_started, wheel_spin_stopped |
| Wheels/suspension | tyre_punctured, tyre_burst, wheel_detached, suspension_impact, jump_landed |
| Impacts/components | collision_impact, collision_scrape, deformation, glass_broken, light_smashed, door_opened, door_closed, door_locked_attempt, door_latch_loosened, door_broken_off, bonnet_broken_off, boot_broken_off, part_loose, part_broken_off |
| Sound/control | horn_started, horn_stopped, siren_started, siren_stopped |
| Damage/powertrain | damage_applied, engine_damaged, oil_leak_started, oil_leak_stopped, engine_fire_started, engine_fire_stopped, engine_misfire_started, engine_misfire_stopped, petrol_leak_started, petrol_leak_stopped, petrol_fire_started, petrol_fire_stopped, fuel_exhausted, vehicle_exploded, driveability_changed, vehicle_disabled, vehicle_restored, vehicle_repaired |

Adding an event topic connects existing emitters to scripts. It does not
automatically reproduce NPC, crime, mission, network or task-system reactions.

## Geometry and integration limits

High-detail render fragments can exceed the physics API's 4096-point hull limit.
Debris hull construction removes duplicate/nonfinite vertices. If the hull
remains larger than the limit, it retains support vertices in 124 directions,
including all axis extrema. This preserves the visual mesh and keeps the
physics contract valid, but approximates complex collision shapes.

Deformation remains NewViso's articulated/parametric dent system. Reference GPU
damage textures, exact fragment break budgets, full inertia tensors and exact
constraint solving are separate work. Authored component hulls and masses are
now preserved for migrated assets; legacy assets keep the geometry fallback.

## Verification

Run from the engine root:

```powershell
cargo test --offline -p newviso-vehicle -p newviso-runtime --lib
cargo build --offline --release -p newviso -p newviso-bugtrap-ui -j 2
```

Run the integration scenarios from `Projects/VehicleTest`:

```powershell
python tools/verify_reference_logic.py
python tools/verify_specification.py
python tools/verify_modular_damage.py
python tools/verify_vehicle_access.py
python tools/verify_cabin_fx.py
```

The new scenario exercises actual host-event delivery through Shared scripts,
bullet proofs on the common scene-damage route, internal health, leak/fire
transitions, finite-fuel shutdown, explosion idempotence, full repair,
configured driver context, canonical seat names, repeated attachment,
seat changes, transfers, exits and unbound-vehicle removal.

On 2026-10-02, all five scenarios passed and exited with code 0, including all
31 specification checks. The release build and scoped Rust formatting passed.
The default executable was locked by an existing process, so this pass built
and tested `.newviso/build/vehicle-specification/release/newviso.exe` with
`NEWVISO_TEST_ENGINE_EXE` set to its absolute path. The active old binary was
not replaced. Build commands for this isolated output use
`--target-dir .newviso/build/vehicle-specification`.

Unit tests currently cover 63 vehicle and 58 runtime cases. Verification reports
and build logs for this pass are under
`.newviso/verification/vehicle-specification`; earlier reference evidence is
under `.newviso/verification/vehicle-reference`;
project summaries are under `Projects/VehicleTest/maps/generated`.

The specification scenario verifies native XML model matching, source flags,
idempotent profile changes, protected tyre/control behavior, committed damage
payloads, leak end events, repair preservation, burning player-engine output,
electric operation, tank explosion and restoration. Unit tests additionally
cover forced-explosion cleanup and a terminal tank after explosion proof is
removed. The reference leak fixture explicitly enables leakage for its
unoccupied test car; imported model defaults remain `player_only`.

The cabin regression now sends the shared engine key before checking running
engine lights/audio/exhaust. Its previous fixture never requested initial
ignition, while vehicles already spawn with ignition off.

Repository-wide isolation passes. Boundary and code-health checks still report
existing repository debt: stale package rules, concrete asset markers,
oversized modules and duplicate blocks. Their full results are retained.
These checks are not presented as passing based on the successful vehicle tests.

## Remaining reference parity work

| Area | Concrete remaining behavior |
| --- | --- |
| Model flags and metadata | Source-authored engine/tank region extraction, special armour/weapon branches, contact explosions, explosion tuning and recent-player/network leakage context. Current named gates and fuel trailers are implemented. |
| Transmission/engine | Exact burning-engine torque and steering behavior, all gearing/clutch/KERS branches and remaining class damage multipliers. Aircraft health misfires are implemented. |
| Motorcycles/BMX | Distinct bicycle semantics, lean/wheelie/stoppie state, rider balance/ejection and specialised wheel contacts. |
| Aircraft | Reference aero/control state, landing gear, propellers/rotors, tail damage, stalls, aircraft-specific break-down and crash handling. |
| Boats/submarines/amphibious vehicles | Exact buoyancy/wave/anchor behavior, water entry, flood/sink transitions, depth implosion and amphibious mode transitions. |
| Trains/trailers/draft vehicles | Rail paths, carriage coupling, doors, derailment, tow articulation and specialised powertrain ownership. |
| Gadgets | Tow arms, cranes, winches, retractable devices, parachutes, weapons and special abilities. |
| Occupant/gameplay events | Explosion injuries, collision/ejection damage, carjacking/stealing, shocking/crime events and NPC/task responses. |
| Vehicle AI/population | Driver tasks, traffic policy, generators, population jobs, navigation and mission-specific vehicle state. |
| Networking/persistence | Authority/clone rules, replicated damage, ownership migration, replay/save and network event ordering. |
| Rendering/physics parity | GPU damage textures, exact collision deformation, full inertia tensors and complete authored fragment constraint solving. Component masses/hulls and group metadata are preserved for migrated models. |

Each remaining area needs its own reference-to-owner mapping and behavioral
scenario. The inventory and this checklist are the continuation point; the
entire vehicle subsystem must not be labelled fully ported until these areas
are implemented and verified.
