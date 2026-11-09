# Physical swing doors

Shared `scripts/newviso/world/doors.ysc` starts and retires authored doors through
`engine_game.ysc`. This is available to projects using the shared engine lifecycle,
including projects which own their player controller. Empty default door lists
create no bodies. VehicleTest's active `config/district-runtime.json` contains a
sample near the player spawn; its older track configuration contains the same sample.

A door uses a dynamic box collision proxy with computed inertia, a separate
textured model, a static hinge anchor, and optional static frame pieces. The visual leaf is not a scene-static
collider: its position and rotation follow native body updates. Script code emits
startup/teardown and interaction intents. Native controls and contacts own leaf motion;
opening never teleports the leaf or animates its transform.
Player capsules, NPC capsules and other dynamic objects can push the leaf through
ordinary native contacts. The shared swing profile now supplies a yielding roller latch and a bounded closer.
Use a handle latch for doors that require deliberate operation.

## Authored configuration

The engine defaults provide `variables.engine_doors.enabled` and
`profiles.swing`. A project adds instances:

```json
{
  "engine_doors": {
    "doors": [{
      "id": "room.entry",
      "profile": "swing",
      "hinge_position": [0, 1.06, -5],
      "size": [1, 2.1, 0.06],
      "rotation_degrees": [0, 0, 0],
      "hinge_side": -1,
      "joint_ids": [900082001, 900082002],
      "frame": true
    }]
  }
}
```

`hinge_position` is the world-space centre of the hinge edge. Size is width,
height and thickness in metres. Rotation is the closed leaf orientation.
`hinge_side` selects the local negative or positive X edge. Each instance needs
two unique joint IDs. The shared profile supplies a centred office-panel model with its original
materials and texture dictionary. `model_dimensions` defines its reference panel
size; Shared scales it to the authored collision box. `model_half_extents` preserves
render bounds for handles extending beyond the panel. A custom instance `model`
can provide these fields too; without dimensions it keeps its existing unit scale.
No automatic arbitrary-asset fitting or terrain placement is performed.

Profiles supply mass, friction, restitution, linear/angular damping, positional
force capacity, angular stop stiffness/damping/torque capacity, stop angles, hinge
separation, frame dimensions/clearance, colours and streaming distances. Angles
are authored in degrees and converted at the shared boundary to radians.
The sample's envelope is -100 to +100 degrees. Stops are compliant and effort
limited; this is not an infinitely rigid lock.


## Existing scene models

`variables.engine_doors.scene_doors` attaches the same shared behavior to an
existing scene mesh. The engine command `physics.hinge.bind_scene` owns the native
body, anchor and joints; it does not spawn a replacement visual. Example:

```json
{
  "engine_doors": {
    "scene_doors": [{
      "id": "office.entry.left",
      "profile": "swing",
      "anchor_entity": 900086000,
      "joint_ids": [900086001, 900086002],
      "local_pivot": [0, 0, 0],
      "frame_rotation": [0, 0, 0, 1],
      "collision_inset": [0.04, 0, 0],
      "collision_bounds": [[0, -1.35, -0.03], [1.5, 1.42, 0.03]]
    }]
  }
}
```

Bounds and pivot are in original model space; inset is in scaled physical metres.
Omit `collision_bounds` to use resident model bounds. Full render bounds include
handles and decoration: use a fitted panel proxy when their union would overlap a
frame. The runtime derives COM, box inertia, scaled extent and visual-to-body
offset from these bounds. A model pivot on the bottom of a hinge stays on that
same hinge line; the two sockets are centred along it at the physical COM height.
Original materials, model identity, hierarchy, transform scale and entity ID
remain attached to the scene object.

Binding waits for geometry residency and reserves its IDs while pending. A repeat
of the same definition is idempotent; conflicting IDs or changed definitions are
rejected. A body or independently streamed collision resource already owned by
another system cannot silently become a door. Activation removes the original
scene-static proxy before collecting that frame's static colliders. Active bodies
are not recreated on every render residency update.

`physics.hinge.unbind_scene` removes only the owned assembly and restores the
original transform, mobility, solidity and physics process claim. Shared shutdown
uses this command for existing meshes and keeps them in the scene. The binding
also retires if its source entity or an owned body disappears. Binding status,
source identity, fitted geometry and pivot separation are exposed in
`runtime.physics.scene_hinges.bindings`.

FirstFPS configures 13 original Lifeinvader door instances as data. Its
`tools/bind_lifeinvader_doors.py` reproduces these bindings from scene identities,
retained door-extension pivots, cooked mesh bounds and map collision. The cooker
fits broad panels, gives the hinge edge clearance and samples nearby imported
floors and lintels through the configured swing envelope. This handles the
entrance's overlapping street/interior floors and 1.7 mm lintel intersection
without moving the visible model. These are
box proxies, not exact handle collision or automatic arbitrary-object recognition.

The runtime and Shared layer contain no FirstFPS or GTA asset-name branches.
FirstFPS explicitly selects the installed Bullet provider because the older
Gravitas provider lacks articulation. Other projects use the same commands and
profiles with any provider advertising the required neutral joint capabilities.

## Provider boundary and lifecycle

`newviso-physics-api::PhysicsHinge` expands a revolute joint into two separated
positional sockets along its local Y axis. Their separation constrains the other
rotational degrees of freedom. One socket supplies twist limits and an optional
bounded motor through the existing neutral joint protocol. The other socket has
no duplicate motor. Frame rotations allow a different world hinge axis.

Runtime commands are `physics.hinge.upsert` and `physics.hinge.remove`.
The former accepts symbolic `parent_id`/`child_id` or numeric `parent`/`child`.
Invalid frames, anchors, IDs, spans, limits, missing bodies and conflicting joint
ownership are rejected before either socket changes. Providers must advertise
the existing articulation capabilities. The anchor uses an ordinary static body
with query/contact reporting disabled: the installed Bullet does not advertise
trigger bodies. Linked bodies use the existing joint collision filtering.

Shutdown removes both sockets and all leaf/anchor/frame bodies and visuals.
Removing a body also follows the runtime's existing joint retirement path.
No new Bullet ABI or provider DLL was required.

## Acceptance and remaining work

`python scripts/verify_scene_doors.py --capture` checks all 13 original bindings,
rest, a measured native player/leaf contact, clearance relative to the actor's
radius, passage through the original opening, and unbind/rebind identity. Use
`--door v_ilev_fb_door02 --side -1` for the mirrored leaf from inside. The harness
clones active project configuration, keeps the original project controller and
hardlinks read-only assets into a disposable fixture. Persistence is disabled.
Opening and passage are both required; an arbitrary angle alone is insufficient.

`python scripts/verify_endorphin.py --scenario door --capture` checks two
independent leaves. Both initially remain at rest; the player opens one and a real
NPC opens the other from the opposite side. The scenario checks pivot separation,
leaf translation, both angular stops and render output. It never writes a leaf
pose to simulate opening. `node scripts/verify_physical_interactions.mjs` also
checks rotated/left/right hinge geometry, real inertia and teardown.

The engine now adds mechanisms, access, navigation gates and persistence as
specified below. Upright characters still contact the leaf through their existing
physical capsules; animated protective hands do not yet have independent
force-transmitting contact bodies. Handle operation uses a bounded physical
actuator, not simulated finger contact.

The final VehicleTest sample hinge is at `[-43.5, 1.13, 57]`, clear of the
parked Rancher and above the level's approximately 0.051 m road surface.
The native project integration checks the actual controller's body and refuses
an authored door whose anchors separate by 2 cm or more.

On the final Release, the isolated two-door test reached about +95.06/-95.10
degrees with maximum pivot separation 0.0665 mm. In the busier VehicleTest scene,
the player opened the sample about 23.88 degrees; peak pivot separation was
18.21 mm. The latter meets the current 20 mm integration bound but remains a
hinge-quality limitation under that scene's contacts. It is not millimetre
accuracy or acceptance of a fully rigid mechanical hinge. The controller neither
resets the leaf pose nor raises effort limits to conceal the displacement.

## Engine scene-binding acceptance — 2026-10-26

Release `f97a5ba08b2a76d2823764b1b0d80db95005ada342625665ed7431ae980cc68c`
was built and used by all nine final native cases. The installed Bullet DLL was
unchanged. Selected runtime, scene, physics API and Endorphin suites passed
409 tests (4 ignored); Shared checks passed 71 physical interaction checks and
272 character motion checks. Formatting, whitespace and project isolation passed.

The native cases passed:

- Both original entrance leaves, from outside and inside (four cases).
- The orange solid door with an equivalent 180-degree X/Z rotation encoding.
- A repeat entrance run after the final map cooker orientation correction.
- Shared generated doors pushed by the player and an NPC, with native stops.
- Protective reaching, moving surfaces, authority release and passive death.
- VehicleTest's original player controller, protective hands and authored door.

All 13 FirstFPS bindings activated. The selected original FirstFPS leaves kept
their source identity through unbind/rebind, opened through measured native
contacts and allowed the actual player body through the opening. The largest
measured pivot separation across those six project runs was about 0.417 mm.
This measurement covers those tested leaves and contacts; it is not a guarantee
for every model or crowded scene. Closed/open captures were inspected.

The map cooker cross-checks full rotations against all 13 native placements.
It accepts this map's upright, unit-scale placements; the engine binding itself
retains full source rotation and scale. Re-running the cooker reproduces the
same runtime data. No leaf pose is written by the opening test.

Machine evidence is in
`.newviso/cache/firstfps-door-work/20261026/acceptance.json`.
Global repository checks still report the prior 5 audio-format boundary findings
and 223 code-health findings. The new scene-hinge modules add no findings there.
This work does not resolve the previously failing Endorphin standing scenarios.

## Mechanisms and passage planning — 2026-10-27

`newviso-doors` is a provider-neutral mechanism and geometry crate. It has no
Bullet, project or asset dependencies. The runtime owns its state, the two real
hinge joints, contact observations and actor reservations. Shared only authors
instances, arbitrates input and presents authoritative results.

A profile's `mechanism` object is merged with the instance's object. Set the
instance `mechanism:false` for a freely swinging hinge. Example of a keyed door:

```json
{
  "id": "office.entry",
  "profile": "swing",
  "display_name": "Office",
  "hinge_position": [0, 1.06, -5],
  "size": [1.2, 2.1, 0.06],
  "rotation_degrees": [0, 0, 0],
  "hinge_side": -1,
  "joint_ids": [900088001, 900088002],
  "mechanism": {
    "latch": "handle",
    "initially_locked": true,
    "key_item": "key.office",
    "free_egress_side": -1
  }
}
```

Declare `key.office` as an ordinary engine item. Native access checks query the
actor's actual inventory; interaction packets cannot provide a trusted "has key"
flag. A key is not consumed. An unlocked handle can be operated without a key.
Locking/unlocking requires the matching item or the authored free-egress side.
A door without a key item is not automatically unlockable from outside. Sides
are relative to the closed leaf's local +Z normal. Omit free egress when neither
side should bypass the key. This also supports privacy locks with inside release.

The latch and bolt are separate. Latch types are `none`, `roller`, `handle`.
A roller yields to contact torque; a handle retracts through an interaction.
The bolt supplies independent retention while locked. Hardware captures only
within its authored alignment and speed tolerances. A close request is refused
while a live capsule occupies the portal, and the closer suppresses closing
effort while occupied. Opening holds the handle retracted until motion completes,
then uses the configured hold period. A timed-out operation becomes obstructed,
releases its operator effort and temporarily blocks that route.

All retention, operator and closer efforts are finite. A retained latch/bolt
adds a native point constraint between the free edge and the strike plate on
the hinge frame. Its force cap is torque capacity divided by the actual lever
arm. Releasing hardware removes this constraint immediately; original hinge
limits remain intact. This avoids relying on an angular motor as a substitute
for the strike plate. Constraint identity is private, allocated without aliasing
existing hinge or mechanism joints, and retired with the door.
The leaf is never frozen or repositioned to conceal constraint errors.
Damage uses the available normal incident kinetic energy of the approaching
body and its measured lever fraction, plus elastic deflection energy, against
separate authored latch/bolt capacities. Bolt fracture spends its capacity first;
only the remaining work can damage the latch. Elastic work is spent once.
After both restraints fail, any remaining work can continue as a native impulse,
bounded by available energy, hinge inertia and incoming momentum, after accounting
for the other body's outgoing energy. This is a discrete fracture continuation,
not finite-element simulation of a bolt or strike plate.
Contact observations are recorded once immediately after each provider fixed
step. A render-only frame cannot erase a contact before the mechanism consumes it.
Bullet's final internal substep may report zero impulse after a genuine CCD hit;
measured approach mass/velocity preserve its work and catch-release momentum.
The engine does not estimate locked-door impact work from free-leaf inertia.
Persistent walking contacts are not repeatedly counted as new impact energy.
Repair requires authorized access and an aligned, unoccupied door.

| Configuration | Units / purpose |
| --- | --- |
| `capture_metres`, `capture_speed_mps` | Edge alignment and closing speed |
| `roller_release_nm` | Torque that releases a roller catch |
| `latch_torque_nm`, `bolt_torque_nm` | Separate retention capacities |
| `retention_stiffness` | Equivalent hardware stiffness for elastic damage energy, Nm/rad |
| `latch_fracture_joules`, `bolt_fracture_joules` | Hardware damage capacities |
| `operating_torque_nm`, `operating_stiffness` | Handle/operator actuator |
| `closer_torque_nm`, `closer_stiffness` | Closing spring; zero torque disables it |
| `open_angle_radians` | Desired opening, clamped to the original hinge envelope |
| `hold_seconds`, `operation_timeout_seconds` | Hold and bounded operation duration |
| `reservation_seconds`, `retry_seconds` | Stale/stalled owner lease and obstruction cooldown |
| `interaction_reach`, `passage_margin` | Handle reach and capsule clearance in metres |

Defaults live in Shared's runtime XML; authoring can override them per profile
or per instance. `handle_local` and `latch_local` may author exact targets in the physical leaf's
local COM space. Omit them for geometry-derived defaults; latch lever must be
nonzero. These fields are independent from model names and rig joint names.
Invalid dimensions, nonfinite values, duplicated identities,
joint ownership conflicts and unsupported providers are rejected.

### Commands, events and input

`door.configure` attaches a mechanism to an existing two-socket hinge.
Shared uses the original authored ID as its stable save identity.
`door.remove` removes only mechanism ownership and restores baseline hinge
settings. Repeated identical configuration and cleanup are idempotent.
`door.interact` accepts `actor_id` and actions `open`, `close`, `toggle`,
`lock`, `unlock`, `lock_toggle`, `release`, `repair`. The engine resolves
a live physical actor, reach, line of sight, real keys and reservation ownership.
Death, ragdoll handoff, task interruption, body retirement and stale ownership
release reservations.

Common input is E to operate and Shift+E to change the lock. It respects
cursor capture, focus, protective hand authority and the nearer pickup.
Inventory and door interaction share a frame claim, preventing one press from
both picking up an item and operating a door. Projects with their own player
controller use the same Shared controls.

`runtime.physics.doors` publishes actual angle, angular speed, hardware state,
handle geometry, occupancy, queue and sequenced events. `door.action_result`
is a request result, not proof of movement. `door.opened`, `door.closed`,
`door.latched`, `door.unlatched`, `door.locked`, `door.unlocked`,
`door.broken`, `door.obstructed` follow physical state transitions. Shared
dispatches each sequence once through Events. Door-specific audio may subscribe
to these edges; no unrelated sound asset is substituted for lock/handle audio.

### Navigation and real NPC travel

Navigation gates are built from original hinge geometry. They check capsule
radius, height, current access direction and key inventory. A* includes door
action cost, excludes inaccessible/obstructed gates and can choose another
corridor. Funnel smoothing is revalidated, so it cannot cut back through a
locked shortcut. Gate changes invalidate stale routes without continuously
changing the revision for an unchanged scene.

The native character controller approaches the handle, reserves a FIFO turn,
opens within the original envelope, waits for geometric clearance of the finite
leaf, crosses and releases the passage. Opposite-side requests use the same
reservation. For a one-way hinge the actor retreats outside the swing arc while
pulling, keeping its facing toward the handle. Waiting actors use their existing
collision/steering controller. No path stage writes actor velocity as test-only
movement or teleports an NPC through the leaf.

This requires traversable navigation topology around the actual opening.
A gate does not manufacture a doorway through solid map collision. Set
`navigation:false` for nonpassage panels such as cabinets. The FirstFPS cooker
derives this flag from the fitted panel height and the shared player clearance,
rather than an engine branch for a named map/model.

Player and FirstFPS movement now submit `physics.character.drive` intents
through a shared ground drive instead of overwriting horizontal speed every
frame. Native fixed steps bound traction by support friction, slope and authored
acceleration; moving support velocity is accounted for. Air input is bounded,
jump requires support, stale intent brakes, and explicit teleports/handoffs
cancel the prior intent. The monotonic fixed-step seconds clock keeps timeouts
correct when fixed Hz changes.

### Saving and restoration

World saves optionally include `newviso.doors.save.v1` records by stable
authored ID: physical pose, velocities, original hinge/shape signature and
hardware damage/lock/latch state. Old saves without doors remain readable.
Transient goals, actor leases and queues do not survive restart.
Restore waits for original resident hinge geometry and validates frame, size,
orientation and anchor alignment. A changed map rejects stale pose restoration.
Shutdown stashes records before removing shared scene assemblies.

### Verification of this extension

Run `cargo test --offline --lib -p newviso-doors -p newviso-navigation -p
newviso-character -p newviso-runtime`, `node scripts/verify_door_mechanisms.mjs`
and `python scripts/verify_door_mechanisms.py`. The native harness creates a
disposable project using the real shared lifecycle, player drive, item inventory
and two real NPC controllers. Its final acceptance receipt and source/build
identity are recorded under `.newviso/cache/door-mechanisms-work/20261027`.
The final measured results are appended after native verification.

## Shared textured model

The default generated door now uses the retained FirstFPS
`v_ilev_door_orangesolid` geometry, handles, two material slots and original
texture dictionary. Shared owns its authoring inputs and runtime closure;
another project does not mount or depend on the FirstFPS directory.

`Shared/Source/models/props/doors/cook_office_panel.py` reproduces both
`models/props/doors/office_panel.ydr` and `office_panel_right.ydr`.
It centres and normalizes the broad panel offline, transforms normal/tangent
bases correctly, and preserves UVs, indices, material parameters and texture
bytes. The opposite hinge variant applies a proper half-turn, retaining tangent
handedness and putting the handle on the free edge. The canonical native cooker
writes the drawable container and its integrity metadata; runtime does not
reinterpret or rewrite model files.

The profile selects `model`/`model_right` by authored hinge side, uses a neutral
white instance tint and scales the model to the physical panel. Existing FirstFPS
scene bindings retain their original individual models, materials and textures.

`python scripts/verify_door_models.py` checks both variants, source topology/UVs,
finite orthogonal tangent bases, mirrored handle orientation, drawable routing
and the complete texture-file closure. The native mechanism proof captures the
door in the actual window and checks loaded GPU material bindings.

## Final mechanism acceptance — 2026-10-27

Release `d117acd5443f836b06b1b1547f78f278ddab8be8edaccd54959d8a1d1ed1ca50`
was built and used for the final native checks. The installed Bullet and renderer
DLLs were unchanged. Workspace `cargo check --offline --workspace -j1` passed.
Selected character, doors, Endorphin, navigation, physics API, runtime and scene
suites passed **451 tests (4 ignored)**. Shared checks passed 24 door checks,
71 physical-interaction checks and 274 character-motion checks.

All **13 final native cases** passed on that executable:

- Key denial, wrong-key denial, common E input with a full 64-bit entity ID,
  finite walking against a bolt, actual opening/closing and occupied-portal refusal.
- Two real NPCs passing FIFO; the first uses its actual inventory key to unlock.
- One-way hinge pulling through the real character controller.
- Restart of a locked/latched door with retained key and no transient reservation.
- A real static obstruction ending the operator attempt and enabling cooldown.
- An 80 kg native sphere at 7 m/s breaking the retained hardware.
- Restart preserving that physical broken-door state.
- A real NPC using directional free egress without a key.
- Original FirstFPS entrance leaves, outside and inside (two separate cases).
- Common player/NPC push doors and physical stops.
- Protective reaching, native muscle authority and passive death.
- VehicleTest's existing controller, protective hands and physical door.
- Shared and actual FirstFPS movement/audio, with two NPCs (two separate cases).

The first eight bullets describe six mechanism boot cases; the pull, key and FIFO
checks share the main exercise boot. The remaining bullets are seven regression
cases. Native receipts explicitly record each case, so this count does not treat
individual assertions as separate launches.

Both shared model variants were rendered and inspected. The left-hinge exercise
and right-hinge free-egress scene each confirmed two loaded GPU material slots
(`tl_v_office_aluminium` and `xj_v_orange`). Source UV/topology and texture bytes
are preserved; the right variant has its own model identity.

The isolated mechanism exercise measured about 0.0118 mm peak anchor separation;
the impact/restart cases reached about 0.738 mm. The two selected original
FirstFPS leaves reached about 0.276 mm. VehicleTest reached about 13.74 mm under
its busier contacts, below the existing 20 mm integration bound. These are bounds
from the specified tests, not a claim of universal hinge precision or completed
active human balance.

Finite air control retains sprint momentum. The flat FirstFPS motion proof now
uses the clear side lane of its disposable yard; the old centre lane introduced
another physical lift during runout. Event thresholds and production movement
were not weakened. Both final player/NPC motion cases require exactly one jump
and one landing per actor and confirm real audio contacts.

Evidence, screenshots and source/build fingerprints are under
`.newviso/cache/door-mechanisms-work/20261027/acceptance.json`.
Workspace layout, isolation, whitespace and model closure pass. Global
documentation, boundary and code-health checks still report repository debt;
their complete results are included rather than represented as passing.
This work does not resolve the prior Endorphin standing failures. Handles use
a bounded operator actuator; independent finger/palm force bodies and a detailed
internal tumbler/bolt deformation simulation are outside this implementation.
