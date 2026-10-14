# Ground vehicle contact and fragment handoff

The ground-wheel solver lives in `newviso-vehicle/src/runtime/tires.rs`.
Vehicle handling remains source-format neutral; Bullet receives point impulses
through `engine.physics`. The reference below is the local vehicle source tree
under `C:\Games\gta v source code\GTAV Source\src\dev_ng\game\Vehicles`.
This mapping records the implemented responsibilities and the remaining fidelity
boundaries rather than declaring complete parity for every vehicle subclass.

## Reference mapping

| Reference responsibility | NewViso behavior |
|---|---|
| `wheel.cpp`, traction ratio and force integration | Longitudinal and lateral slip form one normalized vector before evaluating the peak/falloff traction curve. Both forces share one friction budget. |
| Locked-wheel braking branch | A locked, sliding wheel opposes contact motion. Road speed does not continually reset its angular velocity. Brakes dissipate forward and reverse wheel motion. |
| Full-throttle launch traction | Low-speed grip reduction applies to driven wheels requesting traction, including reference material and hill compensation. |
| `CWheel::SetHandBrakeForce` | The configured handbraked axle stops receiving engine drive torque while handbrake force is applied. Multi-axle layouts distribute the authored axle force. |
| `CAutomobile::GetHandBrakeForce` and `GetHandBrakeGripMult` | Handbrake force decays from full force toward half force; temporary grip loss recovers over 1.5 seconds, including after release. |
| `CWheel::ProcessTyreTemp` | Rotational contact slip generates heat; sideways scrub is not a heat source. Ordinary rolling slip is bounded, and brake-held powered wheelspin uses the burnout branch. Airborne spin only cools; snow, ice and water do not heat the tyre. |
| Hot-tyre damage | Reaching 59 heat units punctures an eligible intact tyre only during an active, supported, brake-held burnout. Temperature alone never punctures a parked or ordinarily driven tyre. Wheel/rim/rubber, model burst protection and invincibility remain authoritative. |
| `vehicleDamage.cpp` and `door.cpp`, fragment release | Physical group ancestry and semantic/render ancestry jointly select an assembly. Already detached subtrees are excluded. Surviving windows and rims follow their physical parent even when their render parent differs. |

Contact integration uses bounded substeps and limits impulses to the closing
velocity of the wheel/contact pair. The wheel and supported mass are advanced
together. With no engine input this prevents friction from adding kinetic energy,
reversing low-speed lateral motion, or spinning a braked wheel backwards.
Suspension support planes are reprojected under the current wheel ray before
applying forces, so motion between physics queries does not offset the force and
particle contact positions.

## Damage and persistent parts

Detached geometry is reconstructed from immutable bind vertices and the captured
articulated/deformed pose. It remains valid even if the source part was already
hidden in the live render buffer. A bare-rim release preserves its last tyre
condition before the wheel changes to `missing`, excluding rubber that was
already worn away.

Generated fragment meshes are explicitly registered as resident geometry in
SceneWorld. They have no streamable asset path or cube render slot, so installing
vertices alone does not make them renderable. The debris snapshot's `rendered`
field now checks scene renderability as well as installed geometry, allowing
the native scenario to detect an invisible body instead of reporting success
from a CPU mesh record alone. Camera-frustum visibility remains separate.

Component IDs are deduplicated across render representations. An authored
assembly retains its component mass, weighted centre of gravity, diagonal
parallel-axis inertia and compound convex hulls. Its body inherits source point
velocity, rotation, hinge motion and captured wheel spin. The source loses the
matching collision components; repair restores them.

Detached components and physical solid particles have independent world
identities. They survive repair, repeated detachment and removal of their source
vehicle. There is no automatic lifetime or transient particle-budget eviction.
Explicit removal of a fragment's scene identity releases its body. This
persistence is within the current world, not a vehicle/debris save-game format.
See [vehicle-modular-damage.md](vehicle-modular-damage.md) for authoring commands,
damage policy, source metadata and the solid-particle path.

## Wheel telemetry and effects

The existing wheel snapshot adds the following fields without changing the
physics-provider ABI:

| Field | Meaning |
|---|---|
| `longitudinal_force`, `lateral_force` | Applied contact force components in the steered tyre basis, in newtons. |
| `friction_utilization` | Fraction of this contact's peak force capacity, including load, handling and grip modifiers. |
| `wheel_locked` | Supported wheel held at zero angular velocity by braking. |
| `sliding_speed_mps` | Combined longitudinal rubber/road mismatch and sideways scrub speed. |
| `tire_temperature` | Reference heat units from 0 to 59; not degrees Celsius. |

Existing `longitudinal_slip`, `lateral_slip_angle`, `slip_intensity`, surface and
tyre-condition fields remain available. Wheel-spin events require propulsive
slip in the selected gear direction: braking-locked driven wheels do not emit
burnout-start events merely because throttle is pressed. Stationary burnouts
can still produce tyre audio and particles because effect eligibility uses
actual contact scrub. Tyre particles originate at the contact position.

## Tyre integrity

Normal acceleration, wheelspin without a brake, braking and lateral skids do
not create permanent wheel damage. Reference `m_fFrictionDamage` describes
an injured/skewed wheel; it is not an accumulator for ordinary tyre slip.
Actual damage transactions remain responsible for that state.

The thermal input uses rotational slip rather than combined sideways scrub.
Outside burnout it is limited to [-1, 1]. With reference heat gain 1.5 and
minimum cooling 0.12, ordinary driving has an equilibrium of at most 12.5
heat units, below the 59-unit burst threshold. Persisted heat at the threshold
also cannot trigger a puncture without an eligible current contact.

A thermal puncture requires a driven wheel receiving engine torque, a service
brake input, vehicle speed below 2 m/s, positive support load, and actual
propulsive rotational slip above 2. A locked or airborne wheel cannot qualify.
The heat-damage transition also checks the surface, model protections and
runtime invincibility. The temperature field remains backward compatible;
the new burnout state is internal and recalculated for each physics step.

The maintained `scripts/verify_tyre_integrity.py` scenario drives the original
Rancher XL handling on a separate flat Bullet surface through forward/reverse
launches, coasting, braking, a handbrake turn and parking. It performs no repair
between manoeuvres, and finally checks that an explicit bullet hit still
punctures only the targeted wheel. The previous Release reproduced a false
rear-tyre puncture at 44.605 s with throttle 1, brake 0 and handbrake 0.

The corrected Release passed 96 vehicle and 100 runtime unit tests (196 total).
Its native integrity scenario completed all 61 seconds and produced 229
samples without runtime errors. All four tyres remained intact through the
first 59 seconds; the explicit final wheel hit punctured only wheel 0.

| Integrity measurement | Previous Release | Corrected Release |
|---|---:|---:|
| False puncture during ordinary driving | Rear tyres at 44.605 s | None |
| Peak sampled heat before the deliberate hit | 58.882 units | 3.269 units |
| Peak vehicle speed | 33.995 m/s | 34.051 m/s |
| Explicit hit affects only its target | No: rear tyres were already damaged | Yes |

The separate Release/Bullet contact scenario also passed all 144 samples:
braking, dry/ice traction, handbrake yaw, normal tyre integrity, and intentional
burnout heat damage. Thermal puncture remained present in the brake-held
burnout stage. Both scenarios used the same final executable. Source hashes,
test results, before/after samples and the installed executable backup are
retained under .newviso/verification/tyre-integrity-20261006.


## Verification

From `NewViso`, build and test an isolated executable before updating the working
Release installation:

```powershell
cargo test --offline --target-dir .newviso/build/vehicle-specification -p newviso-vehicle -p newviso-runtime -p newviso-scene --lib
cargo build --offline --release --target-dir .newviso/build/vehicle-specification -p newviso -p newviso-bugtrap-ui
$env:NEWVISO_TEST_ENGINE_EXE = (Resolve-Path .newviso/build/vehicle-specification/release/newviso.exe).Path
python scripts/verify_vehicle_contact.py
python scripts/verify_tyre_integrity.py
python ../Projects/VehicleTest/tools/verify_vehicle_fragments.py
python ../Projects/VehicleTest/tools/verify_persistent_debris.py
python ../Projects/VehicleTest/tools/verify_modular_damage.py
python ../Projects/VehicleTest/tools/verify_solid_particle_debris.py
```

The road verifier maintains `VehicleTest/scripts/verify_tire_contact.ysc` and
`tire-contact.project.json`, refusing to overwrite differing fixtures. It uses
the original Rancher XL mass properties and collision hulls with the real
Release/Bullet provider. Braking and handbrake stages retain the original
handling. The paired launch stages use identical drive force `0.36` and disable
the full-throttle low-speed grip reduction, isolating the traction ceiling from
intentional reference launch compensation. The standing burnout changes only
brake bias. The verifier compares dry road versus ice, measures wheel locking
and handbrake yaw without an artificial yaw impulse, and checks contact
scrub/heat during a standing burnout. Reports
default to `.newviso/verification/vehicle-contact`; set
`NEWVISO_TEST_REPORT_DIR` to retain a separate run.

The fragment verifier independently audits three cooked models against their
91 raw YFT physical components. Its settled-part impulse assertion measures
peak speed, peak spin and displacement across the response, avoiding a delayed
snapshot after ground friction has already settled the body.

The preceding 2026-10-06 fragment/contact run passed 89 vehicle, 100 runtime and 91 scene unit tests
(280 passed; one pre-existing scene test ignored). The Release/Bullet road
scenario emitted 144 samples and exited normally with no runtime errors:

| Measurement | Result |
|---|---:|
| Speed after braking from 18 m/s for 2.5 s, dry road | 0.029 m/s |
| Same braking, ice | 4.782 m/s |
| Controlled traction launch after 4 s, dry road | 22.472 m/s |
| Same controlled launch, ice | 19.235 m/s |
| Peak handbrake yaw rate | 1.999 rad/s |
| Peak standing-burnout scrub speed | 87.421 m/s |
| Thermal puncture observed | Yes |

Fragment verification passed the raw 91-component audit and all seven initial
assembly releases. Its settled bonnet reacted to an off-centre impulse with
4.249 m/s peak speed, 6.290 rad/s peak spin and 0.896 m displacement. Native
contacts with the ground, another vehicle and a separate moving dynamic box
were all observed, with renderable fragment geometry retained after source
removal. The separate
persistence scenario retained 19 fragments through repair, source removal and
endurance checks. Reports for this run are retained under
`.newviso/verification/vehicle-contact-20261006`,
`.newviso/verification/vehicle-fragments` and
`.newviso/verification/vehicle-persistent-debris`.

The installed Release additionally passed component damage with 31 complete
diagnostic samples (puncture, independent lamps, bonnet/boot/door release and
repair), and kept all 97 solid metal, rubber and glass particles through the
26-second solid-debris scenario. Their bodies remain dynamic after transient
cleanup and react to a point impulse with translation, rotation and lift.
That impulse uses a scene key and a small lever appropriate to a thin shard;
peak response is measured before ground contacts settle it again. Fire and
explosion effects still expire normally.

The working executable is `target/release/newviso.exe`. Its SHA-256 and the
previous executable backups are recorded in this run's
`installed-release.json`. Visibility captures show the detached panels and
wheels on the ground; the preserved earlier captures reproduce the invisible
mesh registration failure.

## Fidelity boundaries

Bullet contact/friction integration and NewViso's parametric denting are their
own algorithms. Exact original deformation textures and fracture/hinge solver,
off-diagonal inertia tensors, parent-body mass subtraction, per-model ABS
branches, and full boat/aircraft/train/trailer subclasses are not reproduced by
this wheel-contact change. The existing native CCD capability limitation also
remains documented in `vehicle-modular-damage.md`. Reference-derived behavior
must be evaluated separately from bit-identical solver results.
