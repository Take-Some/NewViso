# Upright stair locomotion

The dynamic player and native physical NPCs use the same provider-neutral
`newviso-character::StairController`. It runs once per physics substep after
input/locomotion control, before the provider integrates bodies. Bullet remains
responsible for horizontal contact response and external impulses.

The finite-traction player regression was caused by omitting stair resolution
when switching from direct velocity commands. Native resting-contact slop also
exposed an overlap-query defect: an upward query could get a downward normal
from the floor. Primitive overlap normals now use the nearest geometric escape
face. Capsule sweeps reject separating contacts before choosing the nearest
blocking sphere, so support overlap cannot hide a real low ceiling.

The original FirstFPS triangle stair also exposed a posture defect: the rounded
capsule/nosing contact normal was interpreted as the support plane. An upright
NPC could therefore be classified as tipped and enter ragdoll while descending.
Stair assistance now measures the reachable tread plane separately from the
contact envelope used for capsule clearance. Controlled stair support supplies
that plane to posture and protection, and disappears on actual loss of support.
Zero-radius mesh queries return the triangle plane normal against ray travel,
independently of triangle winding, rather than a rounded edge normal.

## Physical traversal

- Find the nearest riser using the actual capsule dimensions and movement intent.
- Probe its first reachable tread with a narrow downward sphere, validate slope,
  height and the complete capsule's overhead/across clearance.
- Transfer support with a bounded vertical lift, on the fixed-step clock.
  Upward contact motion within the configured step-speed budget remains part
  of this transfer; a stronger vertical launch releases control. Shallow
  contact slop is resolved with the same bounded rate and headroom sweep.
  Revalidate the tread throughout the transfer and expire stalled plans.
- Follow a reachable lower tread with bounded descent; a new airborne character
  cannot acquire this snap. A real jump, fast fall or lost support releases it.
- Keep separate native stair support in body snapshots so Shared and FirstFPS
  do not emit a jump/landing pair for each support transfer.
- Explicit pose/velocity handoff, body removal and ragdoll release cancel control.
- Controlled capsule contacts have zero native tangential friction; the existing
  finite-traction drive uses the saved authored coefficient. Releasing control
  restores the passive capsule material. This avoids competing contact and
  virtual-foot traction without giving unlimited drive force.
- Initial body materialization is silent; later genuine falls and landings keep
  their normal events. A temporary native contact omission inside the configured
  combined query and provider contact skins around the last supported pose cannot generate another fall/landing cycle.
  Leaving this plane beyond the skin or a genuine jump keeps the air lifecycle.

Player configuration is `engine_player.ground_drive.stairs`. Native NPCs use
their physical `CharacterConfig`. Radius and half-height always come from the
actual native capsule for physical stair queries.

| Parameter | Meaning |
| --- | --- |
| step_height | Largest reachable rise, metres |
| step_speed | Maximum assisted ascent, metres/second |
| ground_snap_distance | Reachable descending support gap, metres |
| ground_snap_speed | Maximum assisted descent, metres/second |
| max_slope_degrees | Walkable tread/slope limit |
| skin | Query/contact clearance, metres |

Project controllers in FirstFPS, Island, ProceduralMap, VehicleTest and
NorthYankton submit normal locomotion through the engine finite-traction drive.
Jump requests are explicit actions, never inferred from a difference between
requested and observed vertical speed. Island water control retains ownership
of its vertical component while preserving native horizontal momentum before
submitting horizontal drive.

## Navigation

The persistent edge index matches projected shared edges, then validates their
height difference against `NavBuildConfig.max_step_height`. Each path request
also supplies the actor's own step reach. A vertical step between adjacent
walkable treads therefore connects without a manually authored link.
Coplanar internal diagonals and coincident stacked floors cannot become step
shortcuts. Tile removal retires the affected connections. Existing door access,
capsule clearance and route checks remain in the path search.

Navigation still requires authored surfaces to share edges. This is not a
voxelized Recast mesh builder that repairs arbitrary disconnected geometry.

## Skeletal placement

An independent leg IK channel shares the existing two-segment, length-preserving
CPU/GPU solver. It does not replace weapon or protective arm channels.
NPC leg bindings come from their Endorphin rig's sole parts and parent chains.
The configured player uses `engine_player.foot_placement_profile`, with
`hip`, `knee`, `foot`, `toe` and `sole_height` per leg. Shared supplies player bindings in `physics/endorphin/feet_abby.json` and
`physics/endorphin/feet_humanoid.json`.

Foot targets now use the complete neutral sole footprint rather than just
ankle/toe height probes. Static collision meshes build connected coplanar support
regions alongside the collision BVH, during background preparation. Interior
triangulation edges are removed; actual outer edges, concavity and holes remain.
Refitted/deforming meshes invalidate this metadata and use BVH-bounded local
regions, avoiding a full heightfield support rebuild on the gameplay thread.
Primitive box faces use the same region/footprint contract. A mesh collider's
enclosing AABB is never a substitute support surface.

NPC width and heel extent come from the rig's existing sole dimensions. Player
profiles accept `sole_half_width` and `heel_length` in metres in addition to
`sole_height`; Shared Abby and humanoid profiles provide these explicitly.
Legacy profiles without horizontal dimensions use their authored sole-height
radius as a conservative envelope. Neutral geometry is cached per entity,
skeleton/profile identity and scale, so bone names and forward axes are not
hardcoded in placement. The authored ankle-to-toe span determines sole length. Neutral up is derived
from both hip/ankle chains, so an imported neutral pose need not be Y-up.

For each foot, project the sole orientation onto the candidate plane and apply
query-skin clearance to the sole envelope. Retain the authored horizontal placement
when the whole sole fits. Otherwise choose the nearest feasible translation,
bounded by the actual capsule radius and leg reach. There is no attraction to
the tread centre, and no shared target that collapses natural stance width.
The fit rejects a sole that spans a riser, gap or hole. Real downward collision
queries validate the candidate corners against occluding geometry before
publishing the target. An undersized or unreachable surface is not reported as
a valid complete-foot placement.

Capsules can anticipate a higher riser before an animated ankle can reach that
height. A bilateral reach calculation therefore selects a bounded translation
of the complete animated hierarchy. Its limit and speed come from the shared
stair configuration and fixed physics clock. This is a coherent posture layer
evaluated before all limb IK, on both CPU and GPU palettes; it does not change
bone lengths. Physical/ragdoll poses ignore the animated posture offset. Support
plans that cannot satisfy the same posture interval are not published together.

NPC lower limbs are evaluated immediately after native actor presentation,
before the script snapshot is published. The script-owned player is evaluated
after its controller commands. Each actor is solved once per frame, so a new
NPC torso transform cannot be paired with the previous pose's foot plan.
Entering an external physical pose retires animated lower-limb ownership.

The independent leg solver preserves bone lengths on CPU and GPU, aligns the
foot orientation to the support plane and retains the authored relative swing
lift and knee pole. Diagnostic foot snapshots include the support identity,
target and actual sole corners, original ankle and planted/swing state. This
remains terrain correction of authored locomotion, not a replacement for the
experimental articulated Endorphin standing/balance controller.

## Verification

`python scripts/verify_stair_locomotion.py --controller shared --tag current`
launches a disposable project with eight individual 18 cm risers and 32 cm
treads. The real player input and two native NPC tasks must reach the upper
landing and return down. `--controller project` uses the actual FirstFPS adapter.
`--case wall` and `--case ceiling` verify that assistance cannot climb a tall
wall or penetrate insufficient headroom. Native receipts, body traces and
window captures are saved under `.newviso/cache/stair-locomotion-work/<tag>`.

Earlier traversal acceptance on 2026-10-28:

- Release built; workspace `cargo check --offline --workspace -j1` passed.
- Seven selected engine packages: **479 passed, 0 failed, 5 ignored benchmarks**.
- Shared script checks: **324 motion/adapters, 71 physical interactions, 24 doors**.
- **12 native scenarios passed**, all with executable SHA-256
  `99fcce6da00a722dce009f22db03e733dc0f94129448cf5a26ee7d220791dc8b`.

| Native coverage | Runs |
| --- | ---: |
| Shared, FirstFPS, Island and ProceduralMap individual-tread courses | 4 |
| Original FirstFPS first flight, landing and descent | 1 |
| High wall and insufficient headroom | 2 |
| Shared and actual FirstFPS walk/run/jump/landing, with two NPCs | 2 |
| Textured lock/latch/access/physical door exercise | 1 |
| Actual VehicleTest controller, protective hands and authored door push | 1 |
| Native Endorphin protective reach and handoff | 1 |

Each stair course uses the real input controller and two distinct native NPC
skeletons with real `Ped.goTo` tasks. Acceptance rejects all jump, fall and
landing events during ordinary stair traversal; footsteps remain active.
Skeletal checks verify segment lengths and leg-target residuals. The flat
FirstFPS run reports exactly three lifts and three landings (player plus two
NPCs), and validates the compiled first-person coverage contract and loaded
player material textures.

The original course copies `v_9_steps.ydr`, `v_faceoffice.ybn` and the original
texture dictionary/transform from FirstFPS. It exercises the approximately
22 cm risers and rounded triangle nosings that exposed the false tipped state.
Mid-flight and upper-landing captures were inspected. Generic courses use eight
18 cm risers and 32 cm treads. The Island fixture retains its authored water
props/render capacities and seeds the real adapter at the verification course;
production map spawns are unchanged.

The authoritative aggregate is
`.newviso/cache/stair-locomotion-work/acceptance.json`, with source hashes,
individual receipt hashes, provider identity and the scoped whitespace check.
Build/native worker evidence is `final-35.json`; individual stair receipts are
under `<case>-35/acceptance.json`. These checks do not certify unrelated global
code-health/documentation debt or completion of articulated Endorphin balance.

## Complete-foot tread validation (2026-10-28)

The current extension passed Release validation with executable SHA-256
`7e31fa93bf8b298abdaad9de7ddc8fed6f8a1069b1a0d02271e7e4aca9c12956`.

- **389 unit tests passed**, 6 opt-in benchmarks ignored in the ordinary suite.
- The selected support-surface benchmark was also run explicitly and passed.
- Workspace `cargo check --offline --workspace -j1` and scoped formatting/whitespace checks passed.
- Shared source checks passed: **324 motion/adapters** and **3606 camera checks**.
- **Six fresh native scenarios passed** on the same executable:

| Coverage | Evidence |
| --- | --- |
| Shared individual-tread course | Player and two NPC skeletons: all 8 treads, complete soles within bounds, valid off-centre placements unchanged |
| Actual FirstFPS input on individual treads | Same complete-foot, reach, bone-length and air-event checks |
| Original FirstFPS triangle stair | Imported original geometry/textures; ascent/descent and multiple support heights for all three actors |
| FirstFPS first-person motion/materials | Loaded player textures; exactly 3 real lifts and 3 landings for player plus two NPCs |
| Shared third-person camera | 1747 native poses, including 630 collision-shortened booms; heading, vertical look and manual turn checks |
| Endorphin protection/handoff | Player/NPC reaches, moving surface, native muscles, passive death and preserved bone lengths |

Both individual-tread courses independently validate target and CPU-solved sole
corners against the known riser bounds. Every actor covers every tread. They also
check that an already fitting foot remains at its authored horizontal position;
clear landings provide this check for angled/wide soles with little spare tread
clearance. Ordinary stair traversal generated no jump/fall/landing events.
Middle-of-flight and landing captures are fresh; Shared and original FirstFPS
middle captures were visually inspected.

The maximum planned/solved planted-sole discrepancy in these three native stair
runs was approximately **0.011 mm**. This measures evaluated IK corner positions,
not a measurement of final rendered shoe vertices or articulated physics joint
error. Bone lengths remained invariant within the numerical checks.

The geometry-only microbenchmark used a 20,000-triangle plane in the optimized
test profile: index construction **23.91 ms** and **10,000 footprint queries in
11.13 ms**. This is not a complete NPC/frame CPU budget.

The authoritative aggregate is
`.newviso/cache/tread-foot-placement/acceptance.json`; worker evidence is
`final-8.json` in the same directory. It binds source hashes, six native receipt
hashes, six stair captures, executable/provider hashes and scoped checks.
The earlier 12-scenario traversal result above is historical evidence for its
own executable. Full articulated Endorphin balance remains experimental;
this extension corrects authored locomotion using real support geometry.

## Generic scene entities (2026-10-28)

The shared character capability also works without a player, Ped, world actor,
AI registration or project script adapter. Add `character` to a skinned scene
entity, or provide the same object to `scene.dynamic_entity.upsert`:

```json
{
  "character": {
    "enabled": true,
    "foot_placement_profile": "physics/endorphin/feet_humanoid.json",
    "body_id": "custom.physics.body",
    "phase": "native",
    "sync_position": true
  }
}
```

Use an anatomy profile matching the actual skeleton; the solver never guesses
joint names from a project or an NPC identifier. A profile contains independent
hip/knee/foot/toe descendant chains and sole dimensions. The provider-independent
`FootLeg` contract accepts 1–32 supporting chains; weapon arm IK still has its
separate two-arm limit. The skeleton validator rejects missing joints, crossed
ancestry and overlapping branches. Names and geometry are data, not humanoid
left/right branches in the placement algorithm.

Instead of `foot_placement_profile`, `endorphin_model` can reference an Endorphin
anatomy JSON; supporting chains and sole dimensions are derived from its sole
parts. Exactly one anatomy source is required when the capability is enabled.

| Field | Meaning |
| --- | --- |
| `body_id` | Scene name of a separate physics body; omitted means the entity itself |
| `body_entity` | Alternative native body key; cannot coexist with `body_id` |
| `phase` | `native` after native presentation, or `script` after script commands |
| `sync_position` | Follow the body in world space; default true |
| `root_offset_y` | Optional explicit vertical root offset in metres from the body centre |

The default root offset is derived from neutral skeletal sole height and actual
collider dimensions. It is not a model-name/root-lift lookup. Root following
continues in the air while support IK is retired. Raw capsule-centre poses do
not overwrite an anatomy-owned animated root/heading before snapshots.
A full physical pose takes precedence and cancels upright drive without
replacing collision momentum.

`scene.character.configure` uses an entity `id` and a `character` object to
replace the capability; `character: null` removes it. Omission on an upsert
preserves the existing component. Removal/disable clears stale support IK and
posture; phase changes and body rebindings use the same source lifecycle.
The scene inspector exposes the component and authoring undo/redo validates it
transactionally. Runtime snapshots expose string `entity_key`/`body_key` values
under `scene.characters`.

Locomotion intent still uses the engine `physics.character.drive` API and the
real upright capsule with finite traction, slope/step/headroom limits. Foot
sensing also derives dimensions from dynamic box, sphere, cylinder and convex
body geometry; it does not give those shapes automatic capsule stair traversal.
Cylinder local bounds use their actual half-height rather than capsule height.
A non-legged prop uses its native collision shape against each tread and riser;
declaring a character component is unnecessary for physical prop collisions.

This is reusable terrain correction and upright capsule locomotion. It does
not synthesize an unknown skeleton's anatomy, provide arbitrary multi-legged
gaits or complete the experimental articulated balance controller.

`python scripts/verify_entity_stairs.py --tag current` creates a disposable
native project with player/Ped controllers disabled. Two authored characters
(including a separate named body) and one runtime-created character must ascend
and descend all eight treads. Acceptance checks complete target and solved soles,
natural off-centre placement, bone lengths, component removal/restore/disable,
phase switch, numeric body rebinding and an unpinned airborne pose whose render
root follows the new body. Separate native box/sphere/cylinder drops verify
individual tread contact heights; a fast sphere meets a riser without receiving
character stair assistance. Receipts and scoped window captures are stored in
`.newviso/cache/entity-stairs-work/<tag>`.


### Historical generic entity acceptance (before natural-motion extension)

The final Release executable SHA-256 is
`d04901980122b62c6da0890fc83df92840b170ff0df6c4d709efc12bd18179a7`.
**489 selected unit tests passed**, with 6 opt-in benchmarks ignored;
workspace offline check, scoped Rust formatting and whitespace checks passed.
Shared source checks passed **324 motion/adapters** and **3606 camera checks**.

**Nine fresh native scenarios passed on this executable**: generic entities and
rigid props; Shared stairs; actual FirstFPS stairs; original FirstFPS triangle
stairs; tall-wall rejection; insufficient-headroom rejection; FirstFPS
first-person movement/materials with two NPCs; Shared third-person camera; and
Endorphin protective reach/physical handoff. The generic proof covers all eight
treads for each of three entities with no legacy Ped/player controller active.
Separate tests exercise CPU/GPU palette results with one, four and six foreign-
named supporting chains. These do not certify synthesized multi-legged gaits.

The generic body rebind/airborne case follows a new numeric body source, retires
ground IK during flight and reacquires support after landing. The script-phase
root sampled before the next script update can lag the current fixed-step body
by one presentation interval; its measured maximum vertical difference in this
case was about 2.46 cm. This is distinct from the solved sole residual, and
should not be reported as physical-joint or rendered shoe-vertex accuracy.

Fresh generic and original FirstFPS mid-flight window captures were inspected.
The authoritative aggregate is
`.newviso/cache/entity-stairs-work/acceptance.json`; build evidence is
`release2.json`, native/source-check evidence is `native-final.json`.
The aggregate binds source, executable, provider, receipt and capture hashes.
Full articulated Endorphin balance remains experimental.

## Natural authored gait extension (2026-10-28)

Current terrain retargeting preserves authored cadence, independent swing
height and heel/toe roll. It blends landing preparation, holds a loaded support
frame, anticipates clearance and transports the source knee bend plane with
the new leg direction. It never schedules a mandatory flat landing on each
tread.

Both configured source groups now resolve their final foot targets after
script commands and the current animation tick. Attachments refresh after
posture correction and before the late camera update. Published foot snapshots
are immutable captures of that evaluation, with their own sample timestamp;
the native/script source setting still selects ownership and lifecycle.

Actual `contact_point` and `contact_gap` come from the existing native clearance
queries and describe real geometry. They are distinct from a projected landing
goal, source load readiness and a fully flat planted foot. The original stair
verification uses measured contact with at most 5 mm clearance, including
rolling heel/toe support.

See [natural-locomotion.md](natural-locomotion.md) for the shared JSON profile,
contracts and current acceptance. Release
`b81fc2e8024d650f8352890172a4991a20357a5c371f6fb13cd0d35b2bb009f9`
passed 506 selected unit tests and nine fresh native scenarios, including
FirstFPS, the original triangle geometry, camera, materials and physical
handoff. The detailed aggregate is
`.newviso/cache/natural-stairs-work/acceptance.json`.

Earlier per-tread coverage and residual figures above remain historical evidence
for their own executables. Current acceptance measures natural swing/contact
trajectories and actual contacts; full articulated balance remains experimental.
