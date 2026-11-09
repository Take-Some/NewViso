# Endorphin

Endorphin is the native body-response system. The portable `newviso-endorphin`
crate owns authored rig/model types, anatomy, local damage capacity, muscle and
foot-support planning, physical recovery
evidence, reaction definitions and the reusable physical-skeleton palette.
`newviso-runtime/application_endorphin` supplies installed skeleton matrices,
resident physics contacts and gameplay capacity, and applies the resulting
bodies, constraints and presentation. The core has no internal engine dependency,
provider, asset service, renderer, scripting runtime or global application access.

Active standing balance remains experimental. The strict native standing test
currently fails after a small perturbation. The accepted behavior and the failed
acceptance target are recorded separately below.

## Runtime physical control and ownership

The architectural target is a real-time physical character-control layer working
with AI intentions, animation targets, body damage and resident physics feedback.
Behavior selection does not require a neural-network model. Authored controllers
and a future learned policy can use the same physical observation/actuation boundary.
The current hybrid still uses an animated capsule for ordinary locomotion and
creates articulated bodies on demand. Continuous full/partial-body physical
animation, procedural gait, protective reaching and physical LOD remain unfinished.

The portable core now resolves one control-authority observation for the host:
death, pending reset, articulated ownership, recovery, vehicle response and
additive hit reaction. It returns the current body owner and permissions for
capsule locomotion and voluntary combat. It imports no Bullet or engine types.

| Current owner/state | Capsule locomotion | Voluntary combat |
|---|---|---|
| Locomotion | Allowed | Allowed |
| Locomotion with supported additive hit | Allowed | Suspended |
| Endorphin articulation | Suspended | Suspended |
| Recovery animation/body handoff | Suspended | Suspended |
| Vehicle physical response | Suspended | Suspended |
| Passive death physics | Suspended | Suspended |
| Pending reset | Suspended | Suspended |

Movement bindings, Combat Director body observations, firearm requests, melee
requests/contact resolution and published diagnostics consume that decision.
Animation can still supply target poses; physical skeleton ownership determines
the rendered body pose. This contract does not by itself implement continuously
articulated locomotion or arbitrate future per-limb motor tasks.

Physical ownership transfer immediately cancels an existing guard or strike and
invalidates its sampled sweep. New guard requests are rejected while voluntary
combat is suspended; disabling guard remains available. Supported flinches retain
locomotion ownership. Current evidence restores permissions after recovery/reset,
without introducing a second stun timer or health/weapon-specific thresholds.

The native authority regression first failed on the previous executable: the
actor stayed in an upright combat guard after articulation. Its corrected cases
exercise active guard, physical transfer, denied attack, interrupted windup,
passive death, reset and supported flinch on both installed rigs.
Run: python scripts/verify_endorphin.py --scenario authority.

## Skeleton bindings and shared weapon posture

The controller uses semantic anatomy and explicit rig data, rather than selecting
behavior by NPC identity. An Endorphin asset can supply a joint_map from template
joint names to the installed skeleton's names. Resolution is simultaneous and
cached at asset load; logical body parts, trauma, mass weights and function names
remain stable. Anatomy orientation, physical endpoints/anchors, weapon joints and
grip finger joints are retargeted together. The host also supplies this map to the
NPC's melee profile; an explicitly authored melee mapping takes precedence.
Grip holding joints and damage-receiver segments are resolved from that same map
when the raw grip model is installed. Serialized/explicit resolved grip models
are not remapped a second time.

The optional weapon_rig declares the eye marker, ordered upper-arm/forearm/hand/
weapon-socket joints, front-facing yaw/pitch envelope, firing alignment tolerance,
and calibrated hand/finger poses. Grip calibration belongs to the rig data;
there are no Abby/GTA name tests in the grip solver. The existing player
calibration lives in runtime.defaults.xml; the default NPC calibration lives in
physics/endorphin/humanoid.json. A custom rig may omit finger poses or use the
hand itself as its weapon socket. Helper/twist joints between the three semantic
arm joints are supported without changing their lengths.

Installed skeleton identity and hand ancestry are validated once per installation,
including finger ownership. Missing or incompatible joints publish a weapon-pose
capability error and suspend that pose rather than applying another rig's names.
Semantic names cannot be inferred reliably for an arbitrary unlabelled topology:
a new skeleton needs its anatomy, orientation and bindings authored. This is a
universal binding mechanism, not evidence that every asset/topology has already
been calibrated, or that biped balance/gait works on a non-biped body.

Resident NPC posture is driven by measured shoulders, elbows, wrists, sockets and
reach. Requested joint markers share one sampled palette, replacing repeated
full-pose queries. Weapon stance/pole offsets scale with measured arm reach;
stance ratios are weapon presentation data. The shared presentation enumerates
all resident native NPCs, including ambient actors outside the encounter list,
using indexed snapshot maps rather than repeated per-actor scans.

Walking still faces collision-resolved velocity. A rearward target releases the
voluntary arm overlay until the body turns; wrist targets never chase the target
through the character's back. Native firearm admission independently checks the
same body envelope and the equipped prop's actual sight ray. Missing/unsettled
weapons cannot spend ammunition. Accepted bullets start at the attached weapon's
actual muzzle and follow its barrel direction, with the authored spread; they no
longer originate at a fixed torso height or home toward a target behind the body.

Run the native weapon regression with:
python scripts/verify_endorphin.py --scenario weapon_aim
and its ambient/renamed-binding case with:
python scripts/verify_endorphin.py --scenario weapon_aim --ambient --semantic-map.

## Implemented behavior

- Native death, hard landings, loss of balance and vehicle knockdowns can transfer
  an installed humanoid skeleton to an articulated physical body while the actor
  is alive. The default authored rig has fifteen parts. Upright locomotion keeps
  its existing capsule/controller; articulation is created on demand.
- Body part positions and lengths come from the installed animated skeleton.
  Capsule tips match bone endpoints; short segments use spheres. Authored soles use
  oriented boxes with heel reach, width, thickness and vertical offset derived from
  the neutral foot span and collision radius. Their mass, volume and inertia match
  the box geometry; foot support is not a rolling sphere. Missing or
  degenerate bone data prevents ownership transfer and retains the animated
  capsule fallback, rather than creating identity bones.
- Mass is distributed by authored weights. Each body's principal inertia comes
  from its actual capsule, sphere or sole-box geometry. Transfer conserves total linear and
  angular momentum, including the source COM offset from the new articulated COM.
  It does not add a predetermined spin, clamp inherited spin, or prescribe a
  vehicle launch speed. This transfers rigid-body motion; sampled animation
  velocities of individual limbs are not yet included.
- Old upright pose/velocity commands are removed at ownership transfer so the
  next fixed step cannot move the new pelvis back to the old capsule pose.
  Physical impulses remain physical commands.
- Live physical recovery observes all part velocities and angular energies,
  supporting contact normals, supporting point velocity, available standing
  space, body scale, gravity and continuous vitality/endurance capacity.
  A still pelvis cannot conceal a moving limb.
- Recovery clearance sweeps the standing volume against resident world geometry
  and native bodies, excluding the actor's own articulated parts. A low ceiling
  or vehicle can prevent get-up. Missing physical state invalidates clearance.
- A live actor returns from articulated physics through the existing directional
  get-up animation and native capsule, then to normal controller authority.
  Death keeps physical ownership; explicit reset/stream-out removes limb bodies,
  constraints and the physical skeleton.
- Grip support observes physical ownership. Weapon loss still uses the native
  generation-checked Items transfer; falling and later dying cannot duplicate
  the same weapon. Recovery itself does not manufacture equipment or heal wounds.

## Active articulation and local damage

The optional `anatomy` block describes every rig part's anatomical region,
damage multiplier, trauma response, angular limits and muscle parameters. It also
declares functional chains and balance support parts. Limits and muscle targets
use radians: local Y is twist; X/Z span the swing plane. Asymmetric intervals
distinguish flexion from hyperextension and restrict lateral knee/elbow bending.

Anatomical frames come from the installed neutral skeleton, including its
bilateral and vertical references. Body-to-bone alignment retains the sampled
animation orientation. The pose at activation is a muscle target, not a new
anatomical zero. A bent knee cannot redefine its permissible range at transfer.

The provider supplies compliant swing/twist stops and implicit PD muscle drives
using native effective inertia and the fixed timestep. Torques are bounded and
equal/opposite across each joint. They do not reset simulated poses or add a
world-space root support force. Changing targets updates existing joint records;
native sockets and collision filters are retained. The implicit drive predictor
projects mobility through permanent socket Jacobians, then alternates bounded
motor impulses with unilateral contact and finite Coulomb-friction predictions.
Zero-load touching points may accept load. A positive contact gap permits reaction
only when predicted motion closes it. Unloading cannot create tensile support.
Islands are solved independently. Bullet owns actual reactions and integration.

Living articulation blends pose support and authored protective targets using
whole-body kinetic energy, body size, gravity, vitality and stamina. Local trauma
reduces the relevant joint's available stiffness and torque. Dead actors submit
no muscle drives; anatomical stops and positional sockets remain physical.

Balance feedback measures the mass-weighted COM and velocity against the convex
area of actual foot contacts, including finite sole padding and supporting-surface
velocity. A nonnegative pressure solve projects the requested pressure center onto
the actual contact patch. The inverse-dynamics pass accounts for segment weight,
desired body translation and the distributed foot loads, then accumulates subtree
wrenches to compute each joint's feed-forward moment. Native motors bound the
combined pose and feed-forward torque by the injured actuator's available effort.
Centroidal angular feedback uses the whole body's inertia and angular momentum.
Segment angular inertia and translational acceleration enter joint-wrench accumulation;
full gyroscopic inverse dynamics are not implemented.
Foot placement goals retain observed planted positions, follow support motion and
lose confidence continuously after contact loss. Jacobian-transpose corrections
produce internal joint couples. Parent-world hip and child-world ankle goals share
one injured muscle effort limit with relative pose and feedforward. Sole orientation
strength follows gravitational load and sole leverage. A free foot can follow a
verified capture-landing target; an absent contact supplies no supporting wrench.
The old stale-normal alignment fallback was removed. Contact confidence also controls pose support, avoiding an abrupt
switch to resting tone on a single missing contact observation.
No supporting contact can pull on the floor. The native contact solver still owns
the achievable reactions; this calculation supplies only internal joint couples.
Missing foot support cannot supply these balancing moments. Lying-body contact is
not evidence of upright balance. An upright body can return directly to its capsule;
a fallen body still uses directional get-up clips and clearance checks. These
implemented calculations have not yet passed sustained standing acceptance.

Physical limb identity determines articulated impact ownership. Upright anatomical
classification uses the rig's installed bone segments and authored radii. Scripted
damage can provide `hit_part` (part or installed bone name), `hit_point`, or the
existing `hit_zone`. Received damage uses authored anatomy when available.
Only vitality actually lost after armour enters local trauma. The normalized
accumulation is order-independent and affects the corresponding functional chain,
rather than injuring the opposite arm or both legs. Body damage is saved with ped
vitals and restored after physical-body retirement; explicit reset clears it.

The command `ped.endorphin.activate` transfers a resident installed body to
physical ownership. It accepts `actor_id`, optional `direction`, physical
`impulse` and world `point`. Repeated activation applies an additional real
impulse to the existing articulation rather than recreating it.

The physics wire contract adds optional `angular_limits` and `motor` fields.
Providers advertise `JointAngularLimits` and `JointMotors` independently of
legacy `JointConstraints`. Motors include an optional local-frame
`feed_forward_torque` and an optional parent/child `world_drive` orientation goal;
both sides receive equal/opposite physical torque.
`ContactManifolds` adds all native patch samples without changing the legacy
representative contact event. The runtime sends advanced joint fields only when both
articulation features are advertised. Old authored rigs and old packets retain their cone
response. The portable Endorphin crate still has no provider dependency.

Weapon support also observes the appropriate anatomical grip chain. Articulated
support blends continuously with measured upright control rather than assigning
the same hand capacity to every physical pose. Release obstruction probes exclude
the complete owner body family. A pickup temporarily ignores overlapping owner
parts until their rotated bounds separate. Transfer invalidates old relative hand
velocity samples because a capsule COM and a pelvis COM are different references;
changing ownership cannot create a false throw velocity.

## Skeleton integrity

Physical skinning follows the captured skeleton hierarchy. Physics supplies
world rotations and translation of a physical root; connected descendants keep
their captured local bone offsets and scale. Intermediate spine/neck bones and
derived helper drivers retain their hierarchy. Requested physical matrices are
orthogonalized with the captured handedness, preventing scale or shear from
turning solver error into rubber-like deformation. An invalid request is rejected
before replacing the previous physical pose.

Physical worker results are applied as complete poses on both GPU and CPU
skinning paths. The animation presentation layer does not blend physical matrices
or packed vertices component by component: those blends can shrink a rotating
limb even when both endpoint skeletons have correct bone lengths. Physical motion
retains the native sampled pose cadence; quaternion/hierarchy interpolation can
be added separately without violating these invariants.

The skinning update caches integrity metrics from the actual rendered hierarchy.
Native verification checks connected bone lengths, scale, basis shear and physical
socket separation throughout both living and dead articulation. This structural
preservation is independent of muscle activation or a get-up animation.

## Grounded motion and coherent observations

Upright native characters use a velocity-increment budget derived from resident
body/support friction, world gravity and the supporting normal. The controller
samples the fixed physics clock and retains a command across duplicate render
frames. It predicts bounded native contact drag along the previous drive
direction before advancing the intent, so contact friction does not cancel the
whole walking increment. Collision losses cannot accumulate an unrestricted
launch command behind an obstacle. Final-waypoint speed follows the remaining
stopping distance. These are grounded capsule drives; sustained articulated
standing and active-ragdoll walking remain open acceptance targets.

Additive hit reactions retain upright locomotion ownership. Combat interruption
supplies the authored stun interval; the fall/get-up body timer remains owned by
physical recovery. The native grip comparison observes actual hit clips on both
actors and checks that their supported capsules remain controlled throughout.

Weapon-load acceleration samples use native fixed ticks and their complete
elapsed interval. Extra render frames do not repeatedly differentiate or decay
the same physical velocity. Animation-relative hand motion has a separate
presentation sample used for automatic grip-loss release momentum. Replacing a
body or restarting its clock starts fresh motion evidence.

Foot-ground probes evaluate surface velocity at the actual contact point:
the sphere-probe centre minus its radius along the normal. Both translation and
angular point velocity contribute. This fixes disagreement with the recovery
support observation on rotating surfaces. Resident body supports now retain a
local contact/normal/clearance anchor. Each fixed observation transports planted
feet, swing landings, sole orientation and the common balance intent through the
actual support pose. Contact-point velocity is resampled, including angular
motion; a stopped body cannot keep advecting an old velocity. Landing rebinds the
observed foot position. Missing/invalid resident bodies invalidate their anchors.
These targets are motor intent, not extra contacts, fixed joints or root forces.
General terrain replacement and complete moving-support gait remain unfinished.

## Authored model

The default VFS resource is `physics/endorphin/humanoid.json`, stored under
`Shared/Content`. A project can overlay that logical path. A specific actor can
select another model with `PedDefinition.endorphinModel` /
`locomotion.endorphin_model`. Assets are validated once and shared by reference
within the application; changes are loaded by a new session, not polled every frame.

The JSON contains the complete default rig and response tuning:

| Group | Authored quantities |
|---|---|
| `rig` | Total mass (kg), ordered parent topology, bone/anchor names, radii (m), mass weights, legacy cone limits (degrees), optional sole geometry |
| `anatomy` | Neutral orientation references, swing/twist limits (radians), stiffness/damping/torque, protective targets, local trauma and functional chains |
| `anatomy.balance` | Support parts, per-joint effort weights, pressure/feedback gains, sole padding, lean limit and upright handoff alignment |
| `ragdoll` | Friction, restitution, body damping, constraint force limit, cone stiffness and damping |
| `recovery` | Energy allowance, support alignment, evidence response time, ready confidence, balance reach, fall clearance, landing absorption, minimum remaining capacity |
| `hit` | Normalized injury response gain, additive weight and fractions of authored clip time used to interrupt combat |

An explicitly supplied `locomotion.ragdoll` remains an authored per-actor rig
override. The legacy Shared `pedRagdoll` helper remains available for existing
callers; normal ped creation now uses the Endorphin VFS rig instead.

For recovery, kinetic energy is compared with available gravitational support
work, `m*g*support_radius*settling_energy_fraction*capacity`. For an articulated
body, energy includes every part's translation relative to the supporting surface
and its actual rotational inertia. Evidence approaches current stability using
an exponential response whose time scale is proportional to
`sqrt(body_height/gravity)/sqrt(capacity)`. Losing support or clearance immediately
invalidates evidence. There is no fixed HP percentage or fixed two-second get-up
delay. Clip duration still controls animation playback.

Airborne onset uses accumulated descent relative to body clearance. Landing
severity compares observed descent energy with an absorbable height derived from
body size and capacity. Additive hit strength/interruption uses actual vitality
lost after armour, normalized by that actor's maximum vitality and authored clip
length. These are independently authored NewViso models, not recovered GTA formulas.

## Cost and diagnostics

The native adapter observes each actor only after the fixed physics clock advances.
Render-only frames do not supply empty contact observations or erase recovery evidence.
The family support query scans current contacts once, honors the latest contact-end
state and excludes the actor's own limbs and contacts above its root COM. Supporting
height is retained between physical observations. Dead actors skip recovery and
get-up clearance calculations. The adapter only queries standing space for recovering actors. Model
loading is cached. Rig construction and topology allocation occur at transfer,
not on every frame. Support planning uses bounded per-observation arrays and
reuses pressure-iteration buffers. Physical-pose refresh reuses joint names and matrix storage;
unchanged body poses skip scene palette submission. Physics/contact participation
continues while presentation is unchanged. Whole `PedBody` cloning was removed
from the recovery loop, and the body type is no longer clonable.

`runtime.peds.physics.bodies[].endorphin` reports the model path, response time
scale and recovery evidence/energy/clearance. `ragdoll` reports part/joint states,
physical skeleton ownership, actual joint motor/limit descriptors, part capacity,
`pose_updates`/`pose_skips` and cached
`skeleton_integrity` (bone-length, scale and shear error). These counters
demonstrate skipped submissions, not a claimed percentage FPS improvement.

The provider forms J M^-1 and its Schur matrix directly from the bodies touched
by each constraint row. Joint-response vectors are cached as contiguous rows and
applied with the original component accumulation order inside PGS. Native
constraints, contact complementarity, friction limits and actuator torque limits
keep the same physical model. Provider joint/mobility tests now live in their
own modules; the native torque-integral regression covers rotated principal axes
at 60, 120 and 240 Hz with four native substeps.

A captured 15-part articulation replay over 360 fixed ticks measured median wall
time of 354.2898 ms before and 293.5004 ms after the solver changes, five samples
per variant (17.1581% reduction). All ten logged COM/foot state traces, energy and
outcome samples matched. Both variants lost stance at 0.6083 s and completed zero
steps. This timing covers the headless body/control test; renderer frame rate was
not measured. Raw samples and trace hashes are in
`.newviso/cache/endorphin-work/20261023/realism-performance.json`.

## Reference and limits

Read-only inspection covered `TaskNM.cpp` (control and balance feedback),
`TaskNMBalance.cpp` (standing/push/failure feedback), `TaskNMShot.cpp` (component
impulses and moving-floor/balance context), and `TaskGetUp.cpp` (physical-to-animation
recovery). NewViso's implementation is independently authored. The source is not
copied into the engine.

This is an operational body system, not complete GTA V / NaturalMotion parity.
The current articulation has native positional sockets, compliant anatomical stops,
bounded muscle drives, energy-driven protective targets and foot-support balance
feedback. The strict `active_control` scenario still loses upright stance after a
1 N·s perturbation; stable standing is not an accepted feature. Predictive contact
mobility does not yet include the mass coupling of an external dynamic support.
The experimental stepping planner predicts capture position, probes floor/swing
clearance, solves length-preserving leg IK and requires actual lift/contact return
before counting a landing. Stance goal changes follow the gravitational response
time. A loaded swing foot waits until the remaining actual patch supports the
measured capture point. A foot already off the floor is already unloaded and can
reach for capture support; it must not wait for an impossible unload transition.
Loss of the intended stance invalidates a pending unload and replans from the
remaining actual contacts. Landing still requires observed separation and return
near the requested landing, not completion of an animation timer.
This has not solved native stance: the latest two strict runs both release a
swing, but still lose upright balance. Rig0 reaches landing with zero completed
steps; rig1 completes one step and then loses stance during the next swing.
Stable dynamic gait, external-support mass coupling and general terrain
replacement remain unfinished. Collision-predicted hand placement,
reaching/grabbing and a finger solver are also not implemented.
Physical-to-get-up transition uses the existing authored directional clips;
full pose-matched active-ragdoll recovery remains work. Upright detailed receivers
still classify against animated anatomical segments before capsule contact;
they are not fifteen upright collision bodies. Existing vehicle health-damage
rules and tactical decisions remain separate from Endorphin. Swept StrikeCapsules,
an authored knife model and a bounded tactical Combat Director are implemented
in their owning combat modules; see [Shared combat](combat.md). These do not
establish active-ragdoll locomotion parity.

## Verification

- `cargo test -p newviso-physics-api -p newviso-endorphin -p newviso-runtime -p newviso-scene -p newviso-agent -p newviso-items --lib --offline`
- `python scripts/verify_endorphin.py`: real imported skeleton, live fall,
  overhead clearance, physical settling, get-up handoff, death, reset and weapon uniqueness.
- `python scripts/verify_endorphin.py --scenario functional_control`: actual muscle
  descriptors, anatomical joints, local motor weakening, persistent damage, armour,
  death and reset. This test does not assert standing balance.
- `python scripts/verify_endorphin.py --scenario active_control --trace`: the separate
  strict standing target. It requires upright physical response without a get-up
  animation; it currently fails.
- `python scripts/verify_endorphin.py --scenario active_rest`: near-zero-gravity
  transfer with actual muscle drives enabled and no artificial motion.
- `python scripts/verify_endorphin.py --scenario transfer_rest`: zero-gravity
  transfer, angular energy, real release momentum and temporary owner-overlap filters.
- `python scripts/verify_endorphin.py --scenario data_override`: same executable,
  isolated project VFS response-model override.
- `python scripts/verify_endorphin.py --actor-index 1`: another authored combatant.
- `python scripts/verify_endorphin.py --skinning cpu`: the same invariants through CPU skinning; the default is GPU skinning.
- `python scripts/verify_combat.py`, `python scripts/verify_melee.py`
- `python scripts/verify_combat_loot.py --scenario adaptive` and `--scenario rearm`
- Architecture, isolation and direct-console-output verifiers.

Native test projects and reports are disposable under `NewViso/.newviso/cache`;
authored project scenes are not replaced.

### Acceptance snapshot — 2026-10-21

The Release executable and deployed Bullet provider were built from the tested
sources. Engine module tests: 382 passed, 3 pre-existing tests ignored. Provider
tests: 46 passed, 1 pre-existing benchmark ignored. Architecture, isolation and
direct-console-output checks passed.

Twelve of thirteen native scenarios passed: functional control on the first rig
through GPU skinning and the second rig through CPU skinning; active and passive
rest transfer; living fall/get-up on both rigs through GPU skinning and the first rig through
CPU skinning; VFS model
override; existing combat and melee; adaptive live disarm/pickup; and rearm with
generation rejection. Strict standing acceptance failed.

The leg-damage proof lost 5.76 vitality from the authored calf hit, persisted local
trauma 0.3456001 across body retirement and measured a left/right native motor
torque-limit ratio of 0.7077954. Fully absorbed armour damage created no local
tissue trauma. Death removed every muscle command while retaining anatomical
constraints.

The active-body fall runs measured maximum anchor separation of 7.763 mm on the
first rig and 4.845 mm on the second. The earlier approximately 1.67 mm result was
for the passive rig. Active-body constraint convergence remains an improvement
target. The rendered hierarchy retained its captured lengths, scale and rigid
bases throughout the checks; solver separation is not converted to bone stretch.

Reports and logs are under `NewViso/.newviso/cache/endorphin-work` and the
individual `endorphin-*-proof` directories. Reports record executable and provider
SHA-256 values. No FPS improvement is claimed from these functional checks.

### Acceptance snapshot — 2026-10-23

This supersedes the older acceptance snapshot above for current maturity.

- Release executable: `604a15c93fd18da85df9bef8ff849114e1e9c06ef49ef93bb3aaae084035c315`.
- Deployed Bullet provider: `3ff85f4ede43a7755b209a7ab8ae0dafca857d65d007f63cf2cf4fb03cafd55c`.
- Selected engine suites: **432 passed, 4 ignored**, including 46 agent,
  5 steering, 34 Endorphin and 145 runtime tests. Provider: **56 passed, 1 ignored**.
- Workspace cargo check, Release build, boundaries, isolation, direct-console-output
  and workspace layout checks passed.
- Native integration: **20 of 22 unique scenarios passed**. Strict
  `active_control` failed on rig 0/GPU and rig 1/CPU.
- Additional repeatability: **4/4 adaptive retention runs** and **3/3 Director runs**
  passed on this Release; one of each is already in the unique matrix.
  The three preceding Debug Director runs also passed.

| Native target | Outcome |
|---|---|
| Living fall, clearance, settling, get-up, death and reset; both rigs through CPU and GPU | 4 passed |
| Anatomical motors, local capacity, damage persistence and armour; rig 0/GPU and rig 1/CPU | 2 passed |
| Stationary transfer with active muscles; rig 0/GPU and rig 1/CPU | 2 passed |
| Passive rest transfer and VFS Endorphin override | 2 passed |
| Unarmed, knife and tactical Director | 3 passed |
| Existing firearm combat | 1 passed |
| Adaptive grip, VFS grip override, rearm, strong injury, hand injury and checked action release | 6 passed |
| Standing after a 1 N*s perturbation; rig 0/GPU and rig 1/CPU | 2 failed |

The four accepted fall/skinning runs measured these maximum physical anchor
separations. These are scenario measurements, not a global solver bound. Failed
standing runs are not included.

| Rig/skinning | Maximum anchor separation (mm) |
|---|---:|
| 0/cpu | 1.528511 |
| 0/gpu | 0.336414 |
| 1/cpu | 0.365515 |
| 1/gpu | 0.346928 |

Maximum observed relative rendered scale error was 2.384185791e-7. The final
rendered palettes reported absolute bone-length errors no greater than
5.960464478e-8 m. Tiny subresolution segments can have larger relative error.
Native socket error is not converted into visible limb stretch.

The solver replay measured 354.2898 to 293.5004 ms median wall time for a captured
15-part body/control test over 360 ticks, five samples per variant: **17.1581% less
wall time**. All logged COM/foot traces and outcome samples matched. Both variants
still fell and completed zero steps. This does not measure renderer FPS or prove
gait stability; see the cost section for scope.

Grounded motion now has finite friction/gravity-dependent acceleration and braking;
grip derivatives use native fixed ticks. Supported additive flinches preserve capsule
control. Director waiting is discharged by actual attacks/shots, and steering admits
gap/horizon-derived slow approaches. RED tests reproduced the former priority reset
and close-approach stall. No reach inflation, target collision exclusion,
per-weapon release condition or grip-model retuning was used.

Temporary production capture/trace hooks and experiment branches were removed.
Generated diagnostics/backups remain under `Tools/tmp/endorphin-work` and
`.newviso/cache/endorphin-work`.

The overall workspace is **not fully green**: code-health reports 221 findings
including oversized modules, duplicate blocks and source-tree backups. The broader
documentation verifier still reports stale paths and other active-documentation
issues. Neither gate was waived.

**Stable standing, commanded articulated walking/running, reliable moving-support
gait and complete pose-matched active-ragdoll recovery are not accepted features.**
Both strict runs failed during load transfer, with no released swing or completed
step. Full reaching/grabbing/finger contact and general squad orders remain outside
the implemented scope. This is not complete GTA V/NaturalMotion parity.

Current receipt: `endorphin-work/20261023/final-realism-verification.json`.
Worker records: `final-director-release.json`, `final-director-native.json`.
Endorphin reports use the `realism-final-oct23` tag. Earlier failed and
superseded runs remain in their own worker logs.

Rigid weapon models can declare presentation.muzzle_point in model space;
articulated weapons can declare muzzle_joint plus muzzle_joint_offset. Neither
path requires a fabricated skeleton socket. The ray selects a resident attachment
whose asset matches the authoritative equipped item, so unrelated props on the
same skeleton cannot supply its muzzle. The selection uses a typed scene query
instead of allocating a full entity diagnostic snapshot.

Malorian's static muzzle point was measured from the original mesh's upper
circular bore/lip rings. Calibration evidence and the unchanged mesh hash are in
Shared/Source/models/weapon/malorian/muzzle.calibration.json. A weapon-authored
grip_offset is applied consistently to its hand targets and attachment transform.

The weapon regression accepts --weapon, --rear-speed and --decoy. The decoy
places a different attached weapon at the head, far from the held muzzle.
Variant names are part of each disposable proof directory. Standard military
pistol trials use rear speed 1.2m/s; the rifle/Malorian pose trials use 0.15m/s
and require the weapon to remain held. The earlier abrupt 1.2m/s rifle stress
episode lost support/grip and fell; it is not accepted as stable locomotion.
No grip capacity, slip threshold or standing assertion was relaxed.

## Verification closeout: 2026-10-24

The executable cohorts below have separate receipts and must not be merged into
one same-build acceptance count.

| Cohort | Executable SHA-256 prefix | Selected library tests | Native result |
|---|---|---|---|
| Full gameplay regression | a7a51ba0f1e7 | 445 passed, 4 ignored | 30/32 unique scenarios; 3/3 additional adaptive/Director repeats |
| Equipped-attachment selection | 2dcf775de067 | 445 passed, 4 ignored | 13/15 focused scenarios, including 6/6 corrected weapon/decoy cases |
| Final grip-loader module extraction | 73f430d17a68 | 450 passed, 4 ignored | 3/3 final weapon smoke scenarios |

The latest workspace check and Release build pass. The selected libraries are
physics-api, endorphin, runtime, scene, agent, items, collision and steering.
The current workspace runtime library has 153 tests; earlier cohorts had 148.

Weapon cases cover both installed skeletons, CPU/GPU, encounter and ambient NPCs,
opaque semantic/grip bindings, military pistol, rifle and rigid Malorian.
An unrelated weapon is attached to the head, far from the held muzzle.
Rear-facing observations preserve ammunition; actual firing resumes after the
body turns. Solved wrists stay forward and the muzzle stays near the held hand.
The final three post-extraction cases repeat all three weapons with mapped
ambient rigs and that decoy attached.

Strict standing on rig0/GPU and rig1/CPU still fails in both broader cohorts.
Full gait, continuously physical partial-body animation, protective reaching and
physical LOD remain unfinished. New/unlabelled skeletons require authored
anatomy, orientation and bindings; the entire asset catalogue is not calibrated.
No new end-to-end FPS or crowd CPU budget was measured.

The abrupt 1.2m/s rifle stress case lost support/grip and fell; it is not a stable
locomotion pass. Rifle/Malorian pose proofs use gentle 0.15m/s. Standard military
pistol proofs retain 1.2m/s. No grip capacities, slip limits, muscle gains or
standing/Director assertions were relaxed.

Workspace layout, compilation, isolation, no-console, recoil and affected-path
whitespace checks pass. Broader boundaries still flag five concrete audio-format
markers in audio modules. Code-health has 223 findings after splitting the
oversized grip adapter by model-loading responsibility. Workspace documentation
has 55 stale-path/language/migration findings. Global workspace gates are not green.

Malformed initial fixture commands, a wrong-item equipment request and the
missing decoy base_color were corrected and rerun. Rejected harness attempts
remain archived and are not counted as successful acceptance.

Latest executable SHA-256:
73f430d17a68a91b0b54470d7502c888b4f31a9ae3f403631cb216e6eaed44a4

Verified physics provider SHA-256:
3ff85f4ede43a7755b209a7ab8ae0dafca857d65d007f63cf2cf4fb03cafd55c

Detailed receipts under .newviso/cache/endorphin-work/20261024:
universal-final2-acceptance.json, universal-final3-acceptance.json and
universal-closeout-acceptance.json. Grip model loading now lives in
application_peds/grip/model.rs; per-frame load/release logic remains in grip.rs.

## Continuation verification and static flash: 2026-10-24

The shared NPC presentation now uses the native measured muzzle for flash and
smoke, including rigid weapons without a skeleton. Fire audio uses that origin
when available. A missing muzzle does not create a guessed camera-relative flash.
Rigged weapons retain their equipped joint binding. No weapon/model identity
branch was added.

A new native regression failed before the change with:
static weapon flash is not at the measured muzzle.
After the fix, military pistol, rifle and Malorian all pass. Evidence records
flash_source as joint for the first two and model_point for Malorian.
These are mapped ambient NPC cases with an unrelated head attachment;
CPU/GPU and both installed skeletons are represented across the cases.

Fresh verification is tied to separate binaries:
- Workspace library tests: 454 passed, 5 ignored.
- Body/standing verification on 277da0c8f64848587bb8cdfbb1d5321bc0cb6ce044bacc2efd4bfe47061bb89c:
  the three weapon cases passed; both strict standing cases failed.
- Final flash verification on 507c7d2e67caf3bf5a4b9ca2154bdb5dab5d572b65d86435012a4bc5b7d11ab2:
  3/3 weapon/flash cases passed.
- Native logs explicitly resolve engine.physics.bullet, with the verified
  provider SHA3ff85f4ede43a7755b209a7ab8ae0dafca857d65d007f63cf2cf4fb03cafd55c.

The failed standing snapshots lose support in load_transfer with no completed
steps. Neutral sole geometry already projects its span onto the anatomy plane;
no speculative sole/controller change was adopted in this continuation.
Stable balance/gait and the previously listed physical-animation work remain open.

Fresh broader gates: isolation passes; boundaries report5 findings, code-health226,
and no-console reports one benchmark println in
application_physics/frame_events/benchmarks.rs. These are open gate failures.
No full crowd/FPS benchmark or automatic whole-catalogue skeleton calibration is
claimed. Detailed evidence, hashes, the causal RED and final GREEN results are in
.newviso/cache/endorphin-work/20261024/continuation-acceptance.json.

## Surface-relative support and capture transitions — 2026-10-25

The portable core owns `SupportAnchor` and support transport in
`stepping/ground.rs` and `stepping/support_motion.rs`. The runtime supplies only
a generic entity key and `BodyPose`; Bullet types, character identities and asset
names do not enter the planner. Authored semantic leg bindings remain required.

Native support targets retain the contact point and normal in the support body's
local frame, plus the probe clearance. Translation, rotation, acceleration and
stopping use each observed support pose and point velocity. Sole orientation and
the shared balance goal follow a common support frame. Separate supporting bodies
keep separate foot targets. A retired body cancels its pending landing; a measured
landing establishes a new local anchor at the actual sole.

The load-transfer repair distinguishes a loaded foot from one already airborne.
Only a loaded foot needs proof that the other contact patch supports the capture
point before release. When the planned stance disappears, the controller
re-evaluates which foot remains supported. This does not grant fictitious support
or complete a step by elapsed time.

Support memory is now observed separately from the Jacobian-transpose force task.
When the procedural leg planner owns the legs, runtime no longer calculates and
discards the second force task. Existing foot targets also avoid repeated pose
lookups for duplicate contact points. A regression compares the exact memory
state against the full task while requiring fewer pose samples. No end-to-end FPS
or crowd-budget improvement is inferred from this operation removal.

Causal regressions were reproduced before the corrections: the previous Release
failed the elevated-platform test because its foot target had no support binding;
the former release gate left an already airborne foot waiting in load_transfer.
The platform test now exercises real native articulation, motors and skeleton
integrity during translation, rotation, stopping and support removal. It is a
target-transport test, explicitly not acceptance of stable standing on a moving
platform. Use:

```text
python scripts/verify_endorphin.py --scenario support_motion
python scripts/verify_endorphin.py --scenario support_motion --actor-index 1 --skinning cpu --ambient --semantic-map
```

### Verification of the 2026-10-25 continuation

All final native results below use the same Release executable:
`3b117cf42af4135044688ec23a6601e38c93dcd9a891d1389bc33cc0da256e98`.
The deployed Bullet provider remains
`3ff85f4ede43a7755b209a7ab8ae0dafca857d65d007f63cf2cf4fb03cafd55c`;
the renderer remains the previously verified `2e896ab600fe...` artifact.

| Verification | Result |
|---|---|
| Eight selected engine library suites | 469 passed, 5 explicitly ignored |
| Endorphin portable core within that total | 52 passed |
| Release build | Passed |
| Native support transport: rig0/GPU, rig1/CPU, mapped ambient rig1/CPU | 3 passed |
| Native muscles/local injuries/armour, falls/recovery/skinning and authority on both rigs | 6 passed |
| Native mapped weapon/decoy, knife, Director, adaptive grip and shared motion | 5 passed |
| Strict standing after the unchanged 1 N*s perturbation, both rigs | 2 failed |
| Affected-file formatting, isolation and whitespace | Passed |

Thus **14 of 16** final native scenarios passed. Both strict standing failures
remain failures. Rig0 releases its swing and reaches the landing phase without a
counted landing. Rig1 completes one observed contact-return step, then loses
upright stance during the next swing. This is progress beyond the prior
load-transfer deadlock, not acceptance of sustained gait or stable balance.

The authored humanoid model, muscle gains/effort limits and strict standing
fixture are byte-for-byte unchanged. Thirteen Bullet backend sources match the
snapshot taken before this task; no experimental solver change was deployed.
Intermediate executable results are excluded from the final same-artifact count.

Global workspace gates remain open: five existing concrete audio-format boundary
findings, one existing benchmark console print and 226 code-health findings.
The code-health output contains no finding for the changed Endorphin planner or
control adapter. No crowd CPU/FPS benchmark was performed in this continuation.

The machine-readable receipt, source hashes, exact artifacts, native reports,
standing traces, rejected attempts and gate results are in
`.newviso/cache/endorphin-work/20261025/continuation-acceptance.json`.

## Predictive protective reaching - 2026-10-26

The portable core now owns a protective reaching controller. Every resident
PedBody and the configured player's actual physics body can participate,
independently of combat registration and player controller ownership. Profiles
describe anatomy rather than actor identities. Shipped bindings cover the two
installed GTA NPC skeletons and Abby; another skeleton needs an anatomically
valid authored profile. This does not infer an unknown creature's limbs.

The runtime probes head and shoulder trajectories, including falling
acceleration, static surfaces and relative movement of dynamic/kinematic bodies.
Pre-solver contacts supplement prediction. Self bodies and triggers are excluded.
Reach, activation timing, clearance and effort depend on measured limb lengths,
gravity, approach and injury capacity. A receding obstacle does not become an
incoming threat. Death and entity retirement clear the reflex.

Animated characters use a separate arm channel, blended over the weapon channel.
Two-bone solving keeps limb lengths; wrists remain in the anatomical front
hemisphere. Rear threats produce a lateral head guard. Palm bases and finger
poses are data. An equipped weapon retains its finger grip instead of receiving
an open-hand pose. Releasing protection restores the retained weapon targets.

Articulated characters use the same goals through existing anatomical muscles,
joint limits and damage-dependent torque capacities. Their render IK channel is
cleared while physics owns the arms. Protective control adds no pelvis force,
root position pin or unlimited actuator. It does not guarantee that the character
will avoid the collision or remain standing.

While hands are reserved, shared player firing/melee input and native combat
authority pause voluntary attacks; movement remains available. The native
ballistic entry point also rejects a reserved source. Player reload in progress
is interrupted. The current implementation does not add a separate interruption
policy to every NPC reload behavior.

The runtime snapshot exposes readiness/errors, mode, targets, reservation and
achieved joints. Goals include the observation frame which produced them:
script snapshots can already contain a subsequent root turn, so comparing a
previous goal against a later orientation is invalid. Transient palette
unavailability retries; profile/skeleton changes invalidate the binding cache.
One palette samples an actor's observation; inactive actors skip achieved-pose
diagnostics. No crowd CPU/FPS result is claimed.

Visual review caught two issues beyond initial height-only assertions: the
bilateral shoulder cross product had the wrong handedness, and the humanoid
palm-up basis opposed its wrist-to-middle-finger direction. Both were corrected,
with rotated/scaled frame tests and native front/finger-direction assertions.
The previous executable failed the independent front-hemisphere assertion.

The falling-skeleton diagnostic also distinguishes intrinsic edge deformation
from f32 world-position quantization. Raw global relative error remains exposed;
the added local relative metric measures the connected offset before adding
world translation. The absolute global error limit is unchanged. A micron-edge
regression explicitly retains a large raw relative roundoff while requiring
negligible intrinsic/absolute error. This is a measurement correction, not a
physical joint repair.

### Scope still open

Upright protective arms are animation/IK targets, without independent dynamic
hand contacts. Articulated hands use the existing physical hand volumes; fingers
are not individual collision bodies. The system does not yet transfer a measured
upright palm force into the body or door. Existing capsules push doors. Full
continuous physical animation, sustained balance/stepping, more complete hand
contact and physical LOD remain separate unfinished work.

Physical door configuration, native hinge semantics and remaining door gameplay
are documented in [physical-doors.md](physical-doors.md).

### Support prediction and verification

Upright capsules now predict a friction-supported sole landing before accepting
a later head/floor threat. The friction cone uses the actual body materials.
This filters the impossible continuation of a feet-first landing through its
support surface, while retaining an incoming wall. Articulated bodies keep the
full ballistic prediction. A native pickup/fire regression reproduced the false
floor reservation before this correction.

An adaptive pickup test also exercises a genuine NPC contact: requested shots
while hands are reserved spend no ammunition; after the player leaves contact
and protection releases, the collected weapon fires with its inherited magazine.
The test explicitly waits for this authority handoff rather than requiring a shot
during a valid guard.

VehicleTest's real project controller and actual proxy body 900000100 are covered
in a separate native integration. This found an idle-NPC presentation crash when
both the shared player pose and combat target were absent. Idle presentation now
uses the measured body forward, or clears an unavailable arm target. Shared
regressions also cover a rear target retaining that measured forward.

All final native results use Release executable
`4e00c221f1d37b7242cb2538f97d627976e5a8b6fffc8fc4393c00a3b4121bb3`.
The Bullet DLL remains
`3ff85f4ede43a7755b209a7ab8ae0dafca857d65d007f63cf2cf4fb03cafd55c`.
The final shared presentation correction has additional VehicleTest, Director
and JavaScript validation; earlier superseded failures remain in the receipt.

| Verification | Result |
|---|---|
| Four selected engine library suites | 405 passed, 4 explicitly ignored |
| Endorphin portable core within that total | 59 passed |
| Release build, affected Rust formatting and whitespace | Passed |
| Shared physical interaction/presentation checks | 51 passed |
| Shared motion and script checks across eight projects | 272 passed |
| Protective hands: NPC0/GPU, player view, mapped ambient NPC1/CPU | 3 native passes |
| Door contact from player and NPC, opposite directions | Native pass |
| Real VehicleTest project controller, protective hands and authored push door | Native pass |
| Muscles/injuries, falls/CPU-GPU, authority and support transport on both NPC rigs | 8 native passes |
| Mapped rear weapon/decoy, knife, Director and adaptive pickup/fire | 4 native passes |
| Unchanged strict standing test on both NPC rigs | 2 native failures |

The final selected native matrix therefore passes **17 of 19** cases. The two
standing failures remain open; protection is not acceptance of sustained balance.
No Bullet source or DLL replacement was made. Global gates remain open: five
existing concrete audio-format boundary findings, the existing benchmark console
print, and 223 code-health findings. Isolation passes. No end-to-end performance
or crowd benchmark was performed.

The exact case selection, superseded attempts, source hashes, artifacts, native
reports and gate outputs are recorded in
`.newviso/cache/endorphin-protection-work/20261026/acceptance.json`.

## Protective response continuity - 2026-11-02

The reference review covered vehicle brace target updates in
`TaskNMBrace::UpdateBracePosition` and airborne/contact transitions in
`TaskNMHighFall::ControlBehaviour` and `ProcessPhysics`, under the supplied
`C:/Games/gta v source code/GTAV Source/src/dev_ng/game/task/Physics` tree.
The changes below remain native Endorphin controllers.

- During protective release, the retained contact plane advances with its last
  measured surface velocity and its impact countdown ages. A fresh observation
  replaces that prediction directly, without another integration step.
  This short release prediction assumes constant translation; changing surface
  rotation or acceleration still requires fresh sensor observations.
- The articulated chest sensor uses the mean velocity of its measured shoulders,
  including each body's angular point velocity. Its prediction horizon observes
  the faster of head/chest motion. It no longer substitutes pelvis translation
  for a rotating upper body or makes that additional pelvis-state query.
- Missing offsets, limb/head poses or a usable anatomical frame reset protective
  goals, preventing a previous observation from continuing to drive the arms.
- The observation frame reuses its shoulder-vector capacity between valid ticks.

Three new core regressions cover moving-plane release at 30/60/144/240 Hz,
fresh-contact replacement, and updated body frames after translation/rotation.

Verification on this source:

| Check | Result |
|---|---|
| `cargo test -p newviso-endorphin --offline` | 66 passed |
| `cargo check -p newviso-runtime --offline` | Passed |
| Debug and Release executable builds | Passed |
| Native protection: Debug NPC0/GPU and NPC1/CPU | Both passed |
| Native protection: Release NPC0/GPU | Passed |

The native cases include player/NPC reaching, a moving incoming surface, release
of reserved authority, articulated muscle targets, passive death and preserved
arm lengths. The tested Release executable SHA-256 is
`772cba770da890c9e9fbe064f45534847d51ebedd83d1a199dd9bc9bfb37a790`.
Its report is
`.newviso/cache/endorphin-protection-quality1102-release-proof/acceptance.json`.

This focused pass does not resolve the documented sustained-standing failure,
continuous physical locomotion or physical LOD. No crowd timing or FPS gain was
measured.

## Extended recovery and reference inventory - 2026-11-02

The portable recovery observation now requests articulated control during a
severe descent, before ground contact. The decision uses the existing authored
landing-absorption budget, body height, gravity and remaining physical capacity.
Airborne transfer inherits the current momentum without another launch impulse.
Invalid observations retire airborne history instead of replaying an old impact.

Physical recovery samples the four authored get-up clips on the installed
skeleton. A mass-weighted fit compares the measured joint positions while allowing
heading and translation; pitch, roll and limb differences remain in the score.
The selected clip and heading are applied. The fitted translation is diagnostic;
complete root-position matching and physical-to-animation blending remain open.
Missing candidates are counted and reported rather than presented as parity.

Get-up diagnostics expose the current clearance query and the most recent query
that interrupted a get-up, including obstacle identity, fraction and normal.
The native acceptance fixture now requires airborne articulation above the floor,
evaluation of all four candidates, preserved bone geometry and authority/reset
cleanup. It also rejects an unexpected get-up interruption after ceiling removal.

Verification so far:
- Portable Endorphin: 74 tests passed, including eight new recovery/pose cases.
- Runtime compile check and Debug executable build passed.
- The extended default native case passed on NPC0/GPU.
- NPC1/CPU with renamed semantic bindings has a successful strict native run,
  but earlier runs failed in clearance/recovery stages 4 and 5.
  The intermittent blocker has not been diagnosed or claimed fixed.
- No Release acceptance of these extended changes is recorded.
- No crowd timing, FPS gain or original-runtime golden-trace comparison is claimed.

The full selected-directory inventory is recorded in
`docs/endorphin-reference-coverage.json`: 31 game-task units, 76 native-behaviour
implementation units and 32 native-support units. Seventeen have explicit partial
native mappings; the other 122 have no declared mapping. These are implementation
unit counts, including helpers/jobs, not a count of accepted behaviours.
All entries explicitly retain `exact_parity_verified: false`.

Refresh and check that inventory with:
`python scripts/verify_endorphin_reference_coverage.py`
and `python scripts/verify_endorphin_reference_coverage.py --check`.
Source hashes make reference changes visible. This inventory covers the declared
Physics task and native rockstar implementation directories; authored tuning,
assets, other integration/runtime directories and networking require separate
acceptance. Sustained standing, continuous physical locomotion, wound-reaching
families and the unmapped specialised controllers remain open.

Build/test logs and preserved failing/successful runs are under
`.newviso/cache/endorphin-parity-20261102` and the corresponding
`endorphin-default-*-parity1102*-proof` directories.

## Recovery clearance, streamed ownership and measured load - 2026-11-03

This pass supersedes the earlier unclassified clearance blocker and Release-pending notes for the tested configurations.

Standing clearance releases initial contacts only when rising separates from them. This is independent of the walking-slope limit: a tilted dropped object can have a shallow upward normal and still be separating. A numerical tolerance keeps a nearly vertical wall blocking. Walls, ceilings, stationary/descending overlaps and later contacts remain obstacles. Filtering occurs before nearest-hit selection, including inside the mesh BVH, so an initial floor cannot hide a ceiling in the same mesh. Ordinary character sweep semantics are unchanged.

Six geometry regressions cover floor/ceiling ordering, a shared streamed mesh, initial wall/ceiling overlap, query flags and ignored bodies, an 80-degree object versus a nearly vertical wall, and stationary/descending motion.

A zero-impulse transfer attempted before skeleton residency can retain an articulation intent. After binding, the host retries with the latest physical pose and velocity, without adding an impulse. Success, reset and cleanup cancel the intent. Rejected explicit impulses are not replayed. The diagnostic flag is `articulation_pending`. A forced asset-delay regression is still needed for this path.

The fixed-step controller reuses per-actor support-contact and contact-point buffers. This removes temporary contact-vector allocations without skipping ticks, removing collision checks or changing motor gains. No before/after FPS gain is claimed.

| Check on final binaries | Result |
|---|---|
| Portable Endorphin | 74 passed |
| Collision | 28 passed, 2 ignored |
| Runtime | 203 passed, 1 ignored |
| Debug and Release builds | Passed |
| Default/protection: NPC0 GPU and NPC1 CPU with semantic mapping, both profiles | 8 passed |
| Additional Debug NPC1 CPU recovery repeats | 2 passed |
| Reference inventory, Python syntax and affected tracked whitespace | Passed |

Default cases require airborne articulation, all four get-up candidates, recovery after ceiling removal without unexpected interruption, passive death, one weapon drop, reset cleanup and rigid skeletal geometry. Protection cases retain physical muscle/authority checks.

The load fixture waits for all physical actors, inserts ceiling geometry only after each rig is below it, and samples eight seconds of blocked recovery. It discards 30 full-load frames and requires at least 30 remaining frames, a finished run and final physical evidence: resident skeletons, finite poses/velocities, measured floor support, joint gaps at most 0.03 m and no body centers below the fixture floor tolerance. Incomplete loads fail.

| Profile | Active rigs | Sample frames | Physics steps | Endorphin mean ms/step | p95 ms/step |
|---|---:|---:|---:|---:|---:|
| debug | 1 | 698 | 971 | 0.0462 | 0.0507 |
| debug | 8 | 84 | 504 | 0.4734 | 0.4824 |
| release | 1 | 1313 | 1038 | 0.0334 | 0.0364 |
| release | 8 | 101 | 606 | 0.3628 | 0.3676 |

These are single-host CPU controller timings for a bounded fixture, excluding provider solve/render time. Values are normalized per frame before aggregation. Trace body/joint fields are frame submission totals, not resident counts. This is not an open-world crowd benchmark or an original-runtime comparison.

The initial Debug single-actor run exceeded the scripting provider's 50 ms module initialization budget and was rejected. The first Release single-actor run hit the 1400-frame safety cap before finishing and was also rejected. Both are preserved. Standalone reruns passed; the safety cap is now 10000 frames while the eight-second measurement and physical gates remain unchanged. Cold scripting startup reliability is not certified by these reruns.

Reference review includes TaskBlendFromNM.cpp and the recovery implementation in task/Movement/TaskGetUp.cpp. The inventory now has 31 Physics task units, one recovery task unit, 76 native behaviour implementation units and 32 support units: 140 selected source units, 18 partial mappings and 122 unmapped units. These are not behaviour counts or a functionality percentage. Every entry retains `exact_parity_verified: false`.

| Original responsibility | Remaining gap |
|---|---|
| Point-cloud search and root placement | Four initial clip poses are sampled; translation is diagnostic and full root blending is unfinished. |
| Lying-body flatness and get-up bounds | Four ground probes and complete authored recovery bounds are not ported. |
| Blocking entities and contextual recovery | Mass/type rules, attached-door/vehicle exceptions and crawl choices need separate implementation and acceptance. |
| Broader physical control | Sustained standing, continuous physical locomotion, physical LOD and specialized behaviour families remain open. |

Final Debug SHA-256: `f9ec2358794b46036ac20ae2e0dce2a183ce37f3c9e49b6137cf9a3bb46b6628`.

Final Release SHA-256: `ee763bccaa3b14ebf0089fae83b893242f62541906c65d1762fa2dc8f8c32c16`.

The receipt, logs and source hashes are in `.newviso/cache/endorphin-continue-20261103/acceptance.json`. Native reports are `*-cont1103-*-proof/acceptance.json`; accepted load reports use `cont1103-*` and `cont1103-single-*` tags under cache/profiles.

## Safe recovery-root admission - 2026-11-03

This pass supersedes the earlier note that matched translation is diagnostic only. The fitted model-origin X/Z and yaw now place the native capsule before articulated ownership is released. Model origin, skeleton root and capsule COM are distinct: presentation offsets are removed from capsule X/Z; capsule height is grounded independently. The installed skeleton hierarchy supplies the recovery root, including the subsequent animated capsule drive, without assuming a bone name.

The portable planner first tries the fitted position, then eight positions on a capsule-radius circle at 45-degree intervals. Each candidate needs a real walkable floor, a clear standing capsule and a visible path from the current skeleton root. Separating initial contacts are filtered before selecting the nearest obstruction, so they cannot hide a later wall. No safe candidate, invalid geometry or unavailable clip samples leaves the articulated rig in control. A failed capsule replacement also precedes rig teardown.

The host samples the four directional clips at time zero. Sampled joint positions are cached per actor using installed-skeleton identity, model scale, joint names and loaded-clip identity; physical observations and world admission queries remain live. Shared directional clip references reuse samples. The accepted native runs observed sample reuse. This is not a measured FPS improvement.

The local TaskGetUp.cpp reference was reviewed for point-cloud selection, FixupClipMatrix ground projection, bounded radial search and root visibility. The implementation is partial: its ground query is a ray; original capsule/mass/context policy, full bounds/flatness probes and alternative recovery families are not ported.

Validation on the binaries recorded below: 82 portable Endorphin tests, 206 runtime tests and 28 collision tests passed (the existing ignored cases remain ignored). Both build profiles passed. Four default native cases cover NPC0/GPU and NPC1/CPU with semantic mapping on Debug and Release. They assert that an admitted placement is finite, matches the native capsule X/Z and remains after recovery, while retaining airborne articulation, ceiling blockage, all four sampled candidates, death, one weapon transfer, reset cleanup and rigid skeletal geometry. Two Release protection cases retain NPC/player physical authority and muscle checks.

The eight-rig blocked-recovery load probe and executable hashes are appended from the accepted reports below. It is a bounded, single-host control measurement excluding provider solve/render time; it does not measure point-cloud search, open-world crowds or improvement over the previous build.

Remaining work includes temporal point-cloud search, captured physical-pose rebasing/blending into the recovery clip, full recovery bounds, contextual mass/type rules and specialized controllers. No original-runtime golden trace has been compared. All 140 inventory units retain exact_parity_verified: false; 18 have partial mappings.

Receipt: .newviso/cache/endorphin-getup-20261103/acceptance.json.

Accepted Release eight-rig probe: 95 sample frames, 570 physics steps; Endorphin mean 0.3733 ms/step, p95 0.3813 ms/step. Physical-integrity gates passed. These timings do not establish a before/after speedup.

Debug SHA-256: 3b9dc57e5af2f6afea8e3085e36e46a795ec14646a64a4baaddb01d62b7801f8.

Release SHA-256: 1ba96e4ebbd5bdffc9f3db390d3af71eb4a3c5ef26af1abc9bca62a334dd5d43.

## Captured physical-pose recovery and contextual interruption - 2026-11-03

This pass supersedes the earlier missing captured-pose blend note. The selected recovery clip now blends from the live physical skeleton rather than from the previous animation clip.

Before physical teardown, the scene rebases the captured hierarchy from its current model transform into the admitted recovery transform. It validates local TRS, target-clip compatibility, skeleton identity and (for CPU skinning) resident vertex ranges. Invalid transforms, shear or incompatible initial scale handedness reject admission while the physical pose remains owned.

The transition interpolates normalized local rotations with quaternion slerp. Connected offsets interpolate direction on a sphere, including opposing directions, and interpolate only explicit endpoint lengths; equal-length endpoints cannot collapse halfway through a turn. Scale changes come from the endpoint poses. Derived helpers use their driver-relative transforms. Root translation is separate from connected bone offsets. The destination clip continues to advance. Duration uses the existing dimensionless recovery response scale and body-height/gravity time, capped at half the clip duration; original per-item/tag/deep-surface blend tuning is not ported.

The first rebased skin pose is committed synchronously for GPU palettes and CPU vertex streams. Old worker generations cannot overwrite it. Subsequent recovery skin results bypass matrix and packed-vertex interpolation. Physical reactivation, a clip switch, stop or reset retires the captured blend. The immutable source pose is shared with worker jobs.

The grounded physical-recovery clip retains its accepted planar capsule anchor, including at completion. Clearance is tested at that anchor. A removed floor stops the animated capsule drive instead of freezing it at the previous bottom; native gravity then supplies the measured fall, and sustained support loss returns the current blended pose and momentum to physics without an extra launch impulse. A new blocking obstacle likewise returns ownership to physics and cancels the blend. Registered loose pickup bodies use the original object mass boundary (at most three actor masses); fixed, kinematic, joint-attached, heavy, invalid and unknown objects remain blocking. This typed policy is used consistently by placement, clearance, ground and root-path queries, rather than weakening the geometric normal tolerance. Convex items without explicit mass remain conservative; primitive density uses a bounding-volume mass upper bound. Death retains passive physical ownership; voluntary locomotion is not restored during get-up.

New native scenarios remove a real three-metre support platform or insert overhead geometry after get-up starts. They require physical reactivation with the expected cause, cancellation of the old blend, successful later recovery, one weapon transfer and reset cleanup. A pre-acceptance support-loss repeat exposed approximately 0.21 m of planar drift; that failing report is retained under the blend1103-debug-loss0b proof. The final anchor fix is validated without relaxing the original 0.1 m completion gate.

The first anchored support-loss repeat also exposed a near-tangential dropped-pistol obstruction; the failed blend1103-debug-loss0d report is retained. Three host regressions cover the reference mass boundary, fixed/kinematic/attached exclusions, and a genuine ceiling behind a yielded pickup.

Portable scene regressions cover world-space rebasing under heading/origin/scale changes, reflected TRS, half turns and opposing offsets, helper drivers, a moving destination sample, exact target completion, clock partitioning, deferred start, cancellation, invalid targets before ownership transfer, and the synchronous seed palette.

Validation counts and binary hashes below are taken only from the final accepted matrix. Blend evidence counts observed get-up frames, including completed blending; each accepted default/context case must also observe an intermediate weight and completion. Local length diagnostics exclude edges below a position-dependent f32 resolution bound. These checks do not establish original-runtime parity or a measured speedup.

TaskBlendFromNM.cpp and TaskGetUp.cpp were reviewed for physical ownership, retained ragdoll frames, model-transform changes, blend policy and interruption. The inventory remains 140 selected units, 18 partial mappings, with exact_parity_verified false everywhere. Temporal point-cloud selection, original per-item blend tuning and branch tags, complete bounds/flatness checks, deep-surface/crawl/door/vehicle rules, moving-platform recovery attachment, physical LOD and specialised controller families remain open.

Receipt: .newviso/cache/endorphin-blend-20261103/acceptance.json.

| Final check | Result |
|---|---|
| Scene, including 13 new recovery regressions | 190 passed, 3 ignored |
| Runtime | 209 passed, 1 ignored |
| Endorphin core | 82 passed |
| Collision | 28 passed, 2 ignored |
| Debug / Release builds | Passed |
| Debug default/context, NPC0 GPU and NPC1 CPU mapped | 4 passed |
| Release default/context/protection, same rigs | 6 passed |
| Reference inventory, Python syntax and affected whitespace | Passed |

Items library: 9 passed.

Maximum accepted world rebase error: 0.000001048627837008098 m. Maximum resolved local length relative error: 0.00006953777483431622; scale relative error: 8.344650268554688e-7; local basis shear: 6.854527896393847e-7. These are measured bounds for these fixtures, including f32 reconstruction error.

Debug SHA-256: a690e11f2a1d9c68ecf933eda39c08dd55ece115d3a99b09c3222c683ab4946b.

Release SHA-256: fb30faa7b2cb7265f30eea080ca1762d0d4901ed4c184d0f1cbd6ed7e317f4f3.

## Recovery on observed rigid supports - 2026-11-03

This pass supersedes the earlier missing moving-support recovery note for the tested rigid-body configurations. Recovery admission can bind the accepted ground point and forward direction in the supporting body's local frame. Capsule height and presentation offsets retain their gravity-upright/world-offset conventions. Live support position and quaternion supply translation and heading; local normal transport rejects an unwalkable tilt. A missing, invalid, disabled-query or trigger support invalidates the binding. There is no frozen world-anchor or elapsed-velocity fallback for a lost binding.

The portable RecoverySupport model samples the observed frame directly. Point velocity is v + omega cross r at the ground contact, and is supplied to the recovering capsule and final upright replacement. Stopping changes that velocity without extrapolating the old motion. The existing captured-pose blend remains in model coordinates, so support transport carries both the physical source and the advancing animation together. Carrier movement does not require re-capturing all skeletal joints.

Before transport the host probes for the same physical support under the bound point and conservatively covers the complete standing capsule along the requested displacement. The two endpoint caps use ordinary sphere sweeps. Midpoint spheres have a radius of hypot(capsule query radius, half interval height), so their cross-sections cover the complete cylindrical interval, including thin ledges that three uninflated spine probes missed. The regression first failed against the three-probe implementation; the diagnostic is preserved in .newviso/cache/endorphin-support-20261103d/ledge-before2-stdout.log. A clear destination cannot bypass an intervening wall. The support itself is excluded only from that transport sweep; other obstacles remain. Loss of support or an obstructed carry cancels recovery by transferring the current mixed pose and native momentum to articulation, without a new launch impulse. Reset and physical reactivation retire the binding. Completed diagnostics retain the last sample; they do not continue controlling a retired support.

A native static-world regression exposed an unintended restart after a brief driven-capsule contact-manifold gap (the second get-up selected a new placement about 0.24 m away). The failed native log remains in .newviso/cache/endorphin-default-actor1-cpu-sup1103-r1b-static-mapped-proof. Static-world get-up now re-observes walkable geometry at its admitted ground point, with height agreement required every frame; disappearance still transfers control to physics. The same observed ground supplies capsule drive, clearance, completion and presentation. A real geometry loss and a transient native manifold gap are therefore distinguished. The fixture now rejects an unexpected support-loss restart while awaiting completion. The repaired Release CPU/mapped static case passes in three separate native runs without relaxing the original completion gate.

Eight new portable regressions cover translation/yaw at multiple body scales, tilted contact transport with a gravity-upright capsule, angular point velocity, stopping/repeated sampling, invalid/missing/steep frames, a nonidentity initial support frame, invalid admission, and heading wrap/quaternion sign. Four host regressions cover real support binding and point velocity, a thin wall between clear endpoints, a thin ledge between spine probe heights, and preservation of unrelated blockers/query validity.

Native scenarios use elevated kinematic box platforms, the two installed rigs, GPU/CPU skinning and semantic joint mapping. They exercise:
- translation, vertical movement, yaw, a stationary interval, motion during captured-pose blending, final root placement and support-point velocity handoff;
- support deletion while recovering, physical reactivation, blend/binding retirement, and a later recovery on the real world floor;
- a carrier jump to a clear destination across a thin wall, rejection before actor transport, and recovery after the obstruction/support is removed.

Existing airborne articulation, ceiling blockage, all four initial get-up candidates, rigid skeletal geometry, passive death, one weapon transfer and reset cleanup remain mandatory. Static-world and protection regressions are also included. Native per-frame gates are 0.0002 m for the bound ground point, 0.04 m for native capsule X/Z and 0.002 m/s for computed point velocity. Get-up completion retains the original 0.1 m root gate, using the transported final anchor when attached. Relative/absolute bone-length, scale and shear gates are unchanged.

TaskGetUp.cpp was reviewed for safe matrix admission, animation residency, captured-frame transfer, matched-time and heading/mover handling. This implementation extends the existing native support-frame mechanism independently; it does not claim a complete port of the original movement/attachment task pipeline. The reference inventory remains 140 selected units, 18 partial mappings, with exact_parity_verified false throughout.

Acceptance below covers resident rigid-body supports; native motion tests use kinematic boxes. Pure tests cover walkable tilt. Continuously deforming/animated surfaces, streamed meshes without a resident support-body frame, free-falling dynamic carriers, full vehicle/door contexts, and subsequent platform locomotion need separate acceptance. Temporal point-cloud selection, original per-item transition tuning/branch tags and specialised controllers remain open. No original-runtime golden trace or before/after speedup has been measured.

Receipt: .newviso/cache/endorphin-support-20261103e/acceptance.json.

| Final check | Result |
|---|---|
| Endorphin, including 8 new support regressions | 90 passed |
| Runtime, including 4 new query regressions | 213 passed, 1 ignored |
| Scene | 190 passed, 3 ignored |
| Items | 9 passed |
| Collision | 28 passed, 2 ignored |
| Debug / Release builds | Passed |
| Debug native motion/loss/blockage/static | 5 passed |
| Release native motion/loss/blockage/static/protection | 9 passed |

Maximum accepted bound-point error: 2.4669902043719944e-7 m; native capsule X/Z error: 2.4669902043719944e-7 m; point-velocity error: 4.9542753331365774e-8 m/s. Maximum exercised transport: 0.6881458913413566 m and heading change: 0.779999974585937 rad. These bounds belong to these fixtures.

Debug SHA-256: 1008f02ef81db2a623b2c4fe280b6decdce85cbcdde208c11fbfffbef7e51fda.

Release SHA-256: e0097aebc99f42b0b0393174be9531489f8ebeb9b9af771b84bd7db4d4cbf662.

## Time-matched physical recovery - 2026-11-04

Physical get-up now evaluates multiple sample times for each installed directional
clip. The sampled joint positions are cached with the installed skeleton, scale,
marker bindings, clip identity and effective duration. The best mass-weighted
fit selects the heading, translation, root sample and starting time together.
The safe-placement queries still run before physical ownership is released.

The selected time is also used to validate the captured physical-to-animation
blend, initialize the reaction clock and start playback. The blend duration is
capped by half the remaining playback time. Diagnostics expose the sampled frame
count, matched time, effective clip duration and scene playback time.

Reference review covered `TaskGetUp.cpp`, `fwanimation/posematcher.cpp` and
`crextra/posematcher.cpp`. NewViso uses the reference matcher's 30 Hz default
cadence with a provisional window of the first quarter of each clip, capped at
0.5 seconds. This is a NewViso policy. Original authored sample keys, weight
sets, contextual filters and blend tags remain open, along with a comparison
against original-runtime traces.

The native Debug default fixture evaluated 64 frames across four clips, selected
0.133333 seconds and observed the physical blend start and complete. Portable
Endorphin tests passed 92/92, runtime tests passed 213 with one ignored, and
scene tests passed 190 with three ignored. The native report is
`.newviso/cache/endorphin-default-time1104-d0b-proof/acceptance.json`.
The Debug support-loss fixture also passed, including physical reactivation and
subsequent recovery, under `.newviso/cache/endorphin-recovery_support_loss-time1104-loss-proof`.
The Debug obstruction fixture passed on the second installed rig with CPU
skinning and semantic remapping. The Release moving-support fixture passed,
including movement during the blend and support-point velocity at handoff.
Both Debug and Release builds completed.

Additional native receipts are under
`.newviso/cache/endorphin-recovery_obstructed-actor1-cpu-time1104-block-mapped-proof`
and `.newviso/cache/endorphin-recovery_moving_support-time1104-move-proof`.

## Original pose databases and authored recovery profiles - 2026-11-04

The installed GTA V archives supplied `male.ypdb` and `male_prone.ypdb`
from `x64c.rpf/anim/posematchers.rpf` and `nm_blend_out_sets.meta` from
`update.rpf/common/data/anim/pose_databases`. The importer also checked the
base metadata in `common.rpf`. The two databases contain 82 and 91 samples,
respectively. All 14 positive-weight bone tags in each database resolve on
both installed Lost skeletons. The 30 Hz field is history spacing. Each
database entry stores its own exact sample time.

`scripts/import_endorphin_recovery.py` inspects the local binary database or
exports a `recovery_match` block from it. For a profile export, supply
`--skeleton`, `--metadata` and `--bindings`. The bindings file is a JSON array
of objects with `runtime_clip`, `set`, `clip_set` and `clip`. The importer
joins the original clip-set/clip hashes to the chosen metadata entry, keeps
the stored sample times, and carries over the single-clip
`ragdollFrameBlendDuration`. It resolves bone weights against the installed
skeleton tags and rejects missing, conflicting or ambiguous bindings.

The optional Endorphin profile controls weighted marker collection from the
physical pose, the admitted directional clip keys, their exact sample times,
a score multiplier and a per-item blend duration. Zero-weight joints are
skipped, and repeated references to the same clip are evaluated once. The
existing physical clearance and support checks still govern the handoff.
Rigs without the profile continue to use the default search.

The source import check joined four `NMBS_SLOW_GETUPS` items to exact
`male_prone.ypdb` keys. Each sampled time was 0 s and the metadata set the
ragdoll-frame blend to 0.5 s. These items belong to original prone-to-seated
or prone-to-knees sequences and carry follow-up item IDs. The existing NewViso
directional clips come from `get_up@standard.ycd`. Their names and durations
do not establish the original sequence identity.

A native Debug run on the second Lost rig used the 14 original
`male_prone.ypdb` weights, CPU skinning and semantic remapping. The fixture
used three configured times on an installed NewViso clip, selected 0.1 s and
completed the physical blend at the configured 0.12 s. One unique candidate
was evaluated, two repeated entries were deduplicated and one was filtered.
The source-data verification and runtime compatibility evidence cover different
inputs. The latter used the existing clip and fixture timings.

Open work includes chaining the original follow-up items, evaluating the
gameplay conditions that select original context sets, executing the
vehicle and door controller families, and comparing frame traces against GTA V.
