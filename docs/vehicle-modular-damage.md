# Vehicle component damage

Vehicle damage is tracked independently from the Bullet chassis body. Component
state is applied to the wheel solver, fragment presentation, runtime lights, and
detached physics bodies. Ordinary resting contacts remain below the damage threshold.

## Tyres

`vehicle.tire.set` addresses one wheel by its definition name or index:

```json
{"op":"vehicle.tire.set","id":"test.rancher","wheel_index":0,"condition":"punctured"}
```

| Condition | Radius multiplier | Grip multiplier | Additional rolling resistance |
|---|---:|---:|---:|
| intact | 1.00 | 1.00 | 0.000 |
| punctured | 0.82 | 0.48 | 0.075 |
| rim | 0.65 | 0.22 | 0.120 |
| missing | 0.00 | 0.00 | 0.000 |

Resistance is a load-proportional force at that wheel's contact patch, capped by
the impulse needed to stop its forward velocity. A missing wheel produces no
suspension query or tyre force. Damage follows wheel names when definitions are
reordered. Telemetry exports `tire_condition`, `effective_radius`, and
`tire_grip_multiplier` per wheel. `tire_temperature` exposes reference heat units
(0..59); rotational slip heats eligible tyres. Thermal punctures require an
active, supported, brake-held burnout. Ordinary acceleration, braking and
lateral skids do not create punctures or permanent wheel friction damage.
Temperature alone cannot puncture a tyre. Airborne spin only cools; repair
resets heat. Flat rubber wear
smoothly moves effective radius/grip toward the bare-rim values.

## Detachable parts

```json
{"op":"vehicle.part.set","id":"test.rancher","part":"bonnet","detach":true}
{"op":"vehicle.part.set","id":"test.rancher","part":"boot","damage":1.0}
{"op":"vehicle.part.set","id":"test.rancher","part":"door_dside_f","detach":true}
```

Fully damaged doors, bonnet, boot, wheels, body panels, extras, spoiler and roof
become independent dynamic bodies when their meshes are separately bound.
Authored physical groups and semantic/render parent trees jointly supply the
assembly: a wheel includes its rim; a door includes surviving physical children
whose render parent may differ. Already detached subtrees and broken glass are
excluded. A bare-rim release excludes worn-away rubber. Root chassis
groups remain attached during automatic impact damage.

The cooker retains source child masses, damaged masses, inertia, centre of gravity,
component collision hulls, group parents, strength/latch strength, transmission,
joint tuning and damage-health/scales. Physical component IDs are distinct from
render part IDs: duplicate render representations do not double a component's mass.
Distinct physical children of one source group remain distinct.

Detachment freezes the current articulated and deformed mesh pose, including an
open door or bonnet, without changing shared materials/textures. Bind geometry
recovers parts already hidden on the source. Generated meshes are registered
as resident scene geometry so the independent bodies are actually renderable.
Its independent
compound body uses the assembly's weighted centre of gravity and parallel-axis
inertia. It inherits vehicle point velocity, vehicle angular velocity, hinge motion
and wheel spin captured before the missing-wheel transition. The physics API stores
diagonal inertia; off-diagonal tensor terms remain an approximation.

Authoring supplies aligned convex_hull_components with the chassis hulls.
Detachment removes only matching component IDs. Repair restores original hulls
and IDs. Older assets without physical metadata retain role masses and the
geometry-region fallback; an enclosing chassis hull cannot acquire exact holes
from that fallback.

A detached body temporarily ignores only its source body while their world bounds
overlap. Ground, projectiles and other vehicles still interact with it. After
separation the ordinary source-body collision returns. This optional provider
feature is CollisionPairFiltering; older providers continue through capability
negotiation without that feature.

Committed damage follows the physical group tree: child transmission-up reaches
its parent and receiving-child transmission-down reaches siblings/descendants.
Proofs gate the transaction before group health or impulse changes. Negative
strength and root groups prevent automatic fragment release; explicit
vehicle.part.set with detach:true remains an authoring/gameplay force command.
The existing wheel capability and door latch/hinge paths retain their gates.
Exact original constraint/fracture budgets remain separate work.

## Shared authoring and verification

Source models and import recipes are retained under
Shared/Source/models/vehicles/prologue. Run
Scripts/Assets/cook_shared_vehicle_fragments.py after building the VehicleTest
native cooker. It deploys Rancher XL, Police Old 1 and Police Old 2 with 83 groups
and 91 source physical components to Shared content and existing project copies.
The owned game resources supply geometry/tuning; the local source tree at
C:\Games\gta v source code supplies behavior/layout reference.

Projects/VehicleTest/tools/verify_vehicle_fragments.py independently checks the
cooked masses, inertias, centres of gravity and group tuning against raw YFT data.
Its Release scene checks all four wheel/rim assemblies, doors, bonnet, boot,
collider removal/repair, spin handoff, persistent debris, proof handling, group
damage, ground/other-car contacts, impulses and explicit cleanup. Reports are
under NewViso/.newviso/verification/vehicle-fragments.

## Lights and windows

Each light part has independent damage:

```json
{"op":"vehicle.part.set","id":"test.rancher","part":"headlight_l","damage":1.0}
{"op":"vehicle.part.set","id":"test.rancher","part":"headlight_r","damage":1.0}
```

A fully damaged light stops creating its runtime light and hides its separately
bound lamp mesh. Other lights remain operational. Glass and breakable mesh
groups also have independent damage. Collision damage chooses nearby component
geometry, falling back to semantic pivots when mesh bounds are unavailable.

## Repair

```json
{"op":"vehicle.upsert","id":"test.rancher","create_body":false,"repair":true}
```

Repair restores tyre states, component visibility and damage, body/engine
health, dents, and saved collision hulls. Existing detached fragments retain
their independent scene identities, render meshes and physics bodies. Use `recreate_body:true` with authored body parameters to reset vehicle
position and momentum as well.

## VehicleTest controls

Controls apply to the occupied or nearest vehicle:

| Key | Action |
|---|---|
| T | Puncture the next intact tyre |
| B | Detach bonnet |
| H | Detach boot |
| J | Detach the next door |
| N | Break the next intact headlight |
| M | Break all light parts |
| R | Repair and reset vehicles |

`Projects/VehicleTest/modular-damage.project.json` runs the real Bullet scenario
through `scripts/verify_modular_damage.ysc`, logging individual tyre states,
one and two headlight failures, detached body IDs, and preservation of debris on repair.

## Persistent solid particles

Original metal debris, rubber fragments and glass shards are separated from
transient smoke, fire, sparks, dust and glow. Each solid particle retains its
authored geometry, texture, visible colour and size. It receives an independent
dynamic Bullet body with material density, a convex collision hull, gravity,
friction and restitution. The continuous-collision flag is requested; Bullet
currently does not advertise native CCD support. Its initial linear
and angular motion comes from the original emitter.

Solid debris does not fade or disappear at the source particle lifetime and
does not consume the transient particle budget. scene.particles.clear clears
transient effects while keeping physical solids. Explicit scene.entity.remove
of the fragment's scene_key also releases its collider. Debris survives vehicle
repair, repeated detachment and removal of its source vehicle within the current
world. Fire and smoke continue to use their original finite particle lifetimes.

runtime.scene.particles.physical_debris exposes solid particle identities,
emitter/effect names, mass, size, age and source lifetime separately from transient
effect counts. runtime.vehicle_debris reports detached component geometry.

Projects/VehicleTest/solid-particle-debris.project.json exercises original metal,
rubber, windscreen and side-window effects plus transient wreck fire and vehicle
explosion. persistent-debris.project.json covers detachable component geometry,
repair, source removal and explicit cleanup.

On 2026-10-03, 74 runtime, 84 scene and 75 vehicle unit tests passed. Native release
verification retained all 97 solid particles for 26 seconds, recorded surface
contacts for every particle, and verified linear/rotational response to an
off-centre impulse. Original fire and explosion effects expired before cleanup;
all solid bodies and render records survived transient cleanup. The component
scenario also passed with 18 fragments. Both processes exited with code 0.

## Verification result

On 2026-10-02, 63 vehicle and 58 runtime tests pass. The release component
scenario passes its assertions and exits with code 0, together with the
specification, common reference logic, access and cabin FX regressions. Older
verification recorded an access violation during host.registry.release; that
failure did not reproduce in this run. Current build/test evidence and the
separate binary path are documented in vehicle-reference-port.md.

## Reference logic and script events

See [vehicle-reference-port.md](vehicle-reference-port.md) for the current
reference mapping, 64 event topics, common damage routing, complete core repair,
script API, verification results and remaining parity work.

Model capabilities now gate automatic tyre bursts and wheel detachment, without
preventing suspension health loss. Repair preserves authored handling, fluid
capacities and model specifications. See
[vehicle-specifications.md](vehicle-specifications.md) for the exact contract.

See [vehicle-contact-physics.md](vehicle-contact-physics.md) for the current
combined contact solver, source mapping, truthful debris renderability check,
and the 2026-10-06 unit/native verification results.
