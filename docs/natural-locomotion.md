# Natural foot motion

The common character terrain adapter retargets authored locomotion. The clip
continues to supply cadence, stride and limb articulation. The controller
adjusts contact, clearance and reach using each supporting limb's own motion.

## Motion signal

Each limb measures the lowest point of its **authored, rolled sole** relative
to the model origin. Its contact reference is independent of the other feet;
running or jumping can therefore leave every limb in swing. Measuring ankle
height alone would mistake heel/toe roll for foot clearance. Measuring height
against the lowest *other* foot would force a swinging limb toward the ground.

Source world velocity combines model-relative animation displacement with
native body velocity. It does not differentiate fixed-step world positions
as if they were sampled at the animation/render frequency.

Landing preparation and load readiness have separate continuous weights.
A descending foot can prepare an endpoint while still unloaded. Actual load
also requires proximity to real collision geometry. Both native and script
character sources are solved once after script commands and the current scene
animation tick. FK is read with previous terrain IK and posture correction
removed, preventing feedback. Joint attachments are refreshed after the final
posture correction, before the late camera update.

The published foot snapshot is an immutable capture of that final evaluation.
A later script query does not recompute its bones against a newer body root.

## Contacts and correction

- Hidden landing offsets are prepared during swing and become visible through
  the landing weight; an unobstructed free swing retains authored horizontal
  motion.
- Terrain tilt composes with authored heel/toe roll. Small angular errors are
  softened under load using the profile's roll compliance.
- A held foot stores a neutral support frame. Its centre of pressure migrates
  between heel and toe; the ankle pivots around that point rather than being
  frozen in space. Support-local frames follow moving/rotating bodies.
- Goal filtering limits voluntary correction speed and acceleration using
  limb length and gravity. Contact constraints resolve the smallest safe
  displacement separately from voluntary motor effort.
- Continuous sweeps of the previous and proposed sole corners resolve real
  risers. Contact skin remains a separation margin even for sub-millimetre
  motion; it is not a minimum distance for running the sweep.
- Reach and swept contact are solved together after common posture correction,
  so a late anatomical projection cannot push a toe back through a riser.
  Final reach/posture corrections feed back into the correction state, allowing
  the foot to leave a reach limit continuously when the goal becomes feasible.
- Whole-foot projected fit, partial rolling contact and a completely flat
  planted sole are distinct diagnostic states.
- Common posture range derives from skeletal limb length, with a default
  limit of one quarter of supporting limb length. Per-limb reach projection
  preserves segment lengths when posture intervals differ.
- The source knee bend plane is transported with the retargeted hip-to-foot
  axis rather than using an unchanged world-space knee pole.

The `anchored_goal` and `preview_goal` diagnostics identify a saved support
reference and a predicted landing respectively. `held` describes the
unprojected hold and `contact_weight` describes actual loaded proximity.
A reach-limited target can retain an anchored reference while reporting zero
physical load. A future landing can differ horizontally from the present FK
footprint; the no-centering check applies to an unconstrained current fit.
The dense trajectory audit still checks actual swing paths and all sole
penetration regardless of goal provenance.

A character keeps moving under its existing common physics controller; the
foot adapter does not stop it to force one landing on every tread. A natural
stride may pass over an individual tread.

## Data

The default shared profile is
`Shared/Content/physics/endorphin/foot_motion.json`. Projects can select another
logical JSON asset with the runtime variable `engine_foot_motion_profile`.
The profile is loaded once and applies through the same adapter to player,
Ped and generic scene-character sources.

| Property | Unit / use |
| --- | --- |
| `contact_height_ratio` | Sole-length ratio for the contact band |
| `lift_off_speed_ratio` | Fraction of the gravity/limb-length speed scale |
| `stance_speed_ratio` | Horizontal source-speed band for load readiness |
| `transition_seconds` | Continuous phase-weight response |
| `correction_seconds` | Landing-correction response and collision-offset release |
| `anticipation_seconds` | Upper bound for clearance prediction |
| `speed_ratio` | Voluntary speed limit divided by `sqrt(gravity * limb length)` |
| `acceleration_ratio` | Voluntary acceleration limit divided by gravity |
| `pelvis_drop_ratio` | Maximum posture range relative to limb length |
| `plant_tolerance_ratio` | Sole-length ratio for settled whole-foot diagnostics |
| `anchor_engage_weight` | Contact readiness needed to acquire support |
| `anchor_release_weight` | Readiness below which held support releases |
| `anchor_reach_ratio` | Available limb reach before support releases |
| `anchor_turn_degrees` | Permitted heading change while holding support |
| `roll_compliance_degrees` | Small-roll pressure band; larger authored roll is retained |

Anatomy still comes from the compatible per-model foot profile or Endorphin
rig. No joint names, limb count, model name, project name or tread index select
a special gait branch. Full articulated balance and synthesis of new gait
cycles remain separate from this authored-motion retargeting layer.

## Verification

`python scripts/verify_entity_stairs.py --profile debug --tag natural-check --trace`
runs three ordinary scene entities with the legacy player/Ped sources disabled.
It checks real tread/riser collisions, full-course ascent/descent, safe support,
bone lengths, unloaded swing, rolling contact, target continuity, component
lifecycle and body rebinding. Five close gait captures and per-frame foot traces
are available in the tagged cache folder.

Each foot snapshot includes `sample_seconds`, the controller time at which
its targets were published. The scripting snapshot can contain the previous
presentation pose; its callback delta is therefore not the elapsed time
between the two observed foot poses. Temporal checks use the published
controller clock and reject backwards timestamps.

The trajectory audit validates every published sole, not only selected planted
frames. It rejects penetration and large target jumps. Free-swing horizontal
error is checked separately from the measured residual of real collision/reach
projection (`constraint_response`); that response has no exemption from sole
penetration or target-continuity checks. It does not require a
flat landing on every tread. Unit coverage includes source-velocity aliasing,
independent swing height, bounded motor correction, pelvis translation,
heel/toe pivots and explicit contact ownership.

## Release verification (2026-10-28)

Release executable SHA-256:
`b81fc2e8024d650f8352890172a4991a20357a5c371f6fb13cd0d35b2bb009f9`.

506 selected unit tests passed; 6 opt-in checks were ignored. Workspace offline
compilation, scoped Rust formatting and whitespace checks passed. Shared source
verification emitted `STAIR_SHARED_ADAPTER_CHECKS_PASS 324`; camera source
verification reported 3606 checks.

Nine fresh native scenarios passed on this executable: generic scene entities
and rigid props; Shared stairs; actual FirstFPS input on individual treads;
original FirstFPS triangle stairs; tall-wall rejection; insufficient headroom;
FirstFPS movement/materials with two NPCs; Shared third-person camera; and
protective reaching with physical-pose handoff.

The generic course recorded 2067 trajectory observations for three entities
using two rigs, including native and script sources, a named body proxy, body
rebinding and airborne retirement. Its maximum unconstrained free-swing
horizontal error was 14.74 mm; the maximum including measured collision/reach
response was 19.81 mm. Maximum required vertical collision projection was
1.925 mm; free-swing rotation error was 0.0296 radians. These are sole-proxy
and evaluated IK measurements, not rendered shoe-vertex accuracy or articulated
joint/balance certification.

The original triangle course verified real contact points within 5 mm of a sole
corner at eight intermediate heights for the player and five for each NPC.
This checks heel/toe contact separately from high load readiness. It does not
require a flat landing on every tread or tune the gait to tread indices.

Fresh close gait frames and the original FirstFPS mid-flight capture were
visually inspected. Reusable profile/rig data, phase ownership and collision
queries are common to the player, Peds and generic scene characters. Captured
foot snapshots reuse the finalized pose; actual contact point/gap reuse the
existing clearance queries rather than adding another ray pass.

The authoritative aggregate is
`.newviso/cache/natural-stairs-work/acceptance.json`.
Build evidence is `release5.json` and native evidence is
`native-final6.json` in the same directory. The aggregate binds
source, executable, provider, native receipt and capture hashes.

Authored clips still supply the gait. Synthesis of new gait cycles, production
articulated balance and a measured mass-NPC CPU budget are not certified by
this verification.
