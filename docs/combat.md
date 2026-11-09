# Shared combat

`variables.engine_combat.enabled: true` opts a project into the Shared combat lifecycle.
WeaponLab supplies four male combatants inside the existing test yard. Other maps do not enable combat.
Shared owns the gameplay lifecycle, feedback and HUD. Native agent/Items systems own combat state and ownership; Endorphin owns physical body response. Projects provide configuration and placements.

The engine's existing ped vitals own armour, health, hit reactions and death. A `PedProfile.damage_entity`
can bind vitals to an externally controlled physics body, such as the Shared player capsule. That binding
never creates another character controller, AI agent or ragdoll for the player. Incoming bullets and
vehicle impacts use the same armour-first damage rules. Player death releases movement/weapon input;
F restores the player and the encounter after death or area clearance.

An optional `PedProfile.combat` configures `target_actor`, `sight_range`, `memory_seconds`,
`reaction_seconds`, `magazine_capacity`, `reserve`, `reload_seconds`, `spread` and `melee_only`.
The host probes resident geometry at the authored Director sensing interval. Visible targets
enter tactical allocation; occluded targets retain only observed position and memory.
Expired memory restores ambient patrol tasks. Gunshots can be heard through cover, but hearing does
not permit an attack through it. Ballistic rays resolve the actual nearest collision receiver.

Combat ammunition is stateful: shots consume the magazine, reloads consume finite reserves, and death
prevents further attacks. The Shared presentation uses original weapon models, socket attachments,
third-person arm poses, weapon animation and spatial audio. First-person aim solvers are unchanged.

Male character inputs come from the locally installed game's original component models, textures and
male locomotion dictionaries. Rebuild with `Scripts/Assets/cook_shared_combat_peds.py` and then
`Scripts/Assets/cook_shared_ped_reactions.py --ped g_m_y_lost_01` (and `g_m_y_lost_02`).
Source hashes and import provenance are in `Shared/Source/models/characters/gta/` and
`Shared/Source/animations/characters/gta/reactions/`. Runtime consumes cooked Shared/Content resources.

Unarmed combat is opt-in through `variables.engine_combat.melee.enabled`. The optional
`player` object supplies the player's native `MeleeProfile`; individual NPCs use
`profile.melee`. Profile fields are `range`, `damage`, `windup_seconds`,
`recovery_seconds`, `max_stamina`, `stamina_regen`, `attack_cost`, `block_cost`
and `block_damage_scale`, plus `combo_window_seconds`, `combo_damage_bonus`, and `combo_max_hits` (1–8). Consecutive attacks inside the combo window advance the sequence and increase strike damage; expired windows, stun and cancellation reset the sequence. These are native combat parameters, not animation-authored combo transitions. Values are validated by the native ped runtime.

4 holsters the weapon; LMB requests a punch and RMB holds frontal guard. F cycles back to a weapon.
A punch has windup, an active interval and recovery. Damage requires a swept animated
hand/finger contact with a live receiver. Ownership, interruption and resident obstruction are
rechecked; leaving the actual hand sweep or moving behind cover makes it miss.
Guard reduces frontal damage and spends stamina; rear hits bypass guard, and an exhausted guard
breaks into recovery. Damage interrupts pending attacks. Death cancels all pending contacts.
AI brawlers use `combat.melee_only: true`; armed combatants with a melee profile switch to fists
at close range or after exhausting ammunition. Native state owns all health, timing and stamina.

Shared combat presentation is split into input, fist pose and NPC presentation modules. Closed fists
and alternating punches use the original character skeleton and rig-specific joint axes. Damage
reactions release the weapon/fist IK overlay so the existing original directional animation is visible.
Physical firearm recoil uses an analytically integrated damped spring for rearward motion and recovery,
including NPC weapons. The accepted camera recoil, sights, ballistic and first-person aim solvers remain
separate. No generated textures, replacement models or synthetic audio banks are introduced.

WeaponLab configures two male brawlers and two armed opponents; other projects remain disabled.
Combat HUD includes health, armour, stamina, guard state and unarmed controls.

Validation:

- `cargo test --workspace --offline`
- `python scripts/verify_combat.py` after building release `newviso`
- `python scripts/verify_melee.py`
- `node scripts/verify_recoil_pose.mjs`
- `python ../Projects/WeaponLab/tools/verify_weapon_aim.py --run`
- `python ../Projects/WeaponLab/tools/verify_weapon_lab.py --run`
- `python scripts/verify_boundaries.py`
- `python scripts/verify_no_console_output.py`
- `python scripts/verify_isolation.py`

The native combat proof creates a disposable fixture under `.newviso/cache/combat-proof`. It checks
real incoming damage, armour, magazine/reload reserves, cover impact, target loss/reacquisition,
nonlethal animation, lethal ragdoll, player death input gating and F encounter restart. It never edits
an authored project's scene or changes the accepted first-person camera/weapon math.

The native fist proof uses `.newviso/cache/melee-proof` and the real Shared input/controller graph.
It exercises holster, timed and alternating contact, reach/cover misses, cancelled windup, directional
block, stamina cost, rear attacks, male AI, death/ragdoll and returning to firearm presentation.

## Unified anatomical damage — initial runtime implementation

Native `application_peds/hit_zones.rs` classifies head, neck, torso, abdomen, arm, and leg
contacts using an NPC's configured capsule height, radius and contact position.
`ped.damage` accepts an optional explicit `hit_zone` (including aliases `chest`,
`pelvis`, `hand`, `foot`); omitted zones preserve unmodified legacy damage.
Ballistic physics contacts on native NPCs and external character bodies use
the same multipliers; vehicle collision damage does **not** use anatomical scaling.
Configured multipliers are provisional NewViso game-balance parameters, not
verified GTA V data. Armor still absorbs computed damage before health.
Installed skeletal segments now resolve actual hit parts and anatomical regions.
Articulated native limbs retain physical identity; melee reports its swept receiver
and exact contact point. Capsule regions remain a fallback without installed bones.
Upright actors do not have fifteen independent native collision bodies.

## Original code comparison (2026-10-17)

Read-only reference files successfully inspected:
- `C:\\Games\\gta v source code\\GTAV Source\\src\\dev_ng\\game\\task\\Combat\\TaskCombatMelee.h`
- `C:\\Games\\gta v source code\\GTAV Source\\src\\dev_ng\\game\\Peds\\HealthConfig.psc`

The original melee header defines a melee finite-state machine with approach,
target facing, chase, retreat, waiting/decision, action and weapon swapping; it
also defines a `StrikeCapsule` (start, end, radius, surface material) and
distinguishes active, supporting and observing combatants. This demonstrates
the architectural difference between the current native timed hit resolver
and the original animation-driven collision and NPC tactical scheduler.
The health schema separates health, armour, endurance, and injured/hurt/dying/
fatigued thresholds. Values in that schema describe authoring defaults, not
verified regional hit multipliers.

That inspection predates the swept-contact and Director implementation below.
Its structures informed the architecture; runtime algorithms and models are independently
authored. No original source files were modified.

## Adaptive weapon retention and native release (2026-10-19)

Weapon loss is owned by native PedRuntime and ItemsRuntime, for native-controlled
peds with an actual equipped weapon. Shared Combat no longer issues NPC inventory
drop commands or maintains an "already dropped this life" latch. Shared PickupScene
owns visual materialization, dynamic pickup bodies and highlighting.

### Authored data and adaptation

`PedProfile.weapon_grip.model_asset` selects a VFS JSON model. The default asset is
`Shared/Content/physics/peds/humanoid_grip.json`; a project VFS overlay or a different
per-ped model can replace it. `weapon_grip.model` also accepts an explicit descriptor.
There are no hit-fraction, arm-damage or critical-health release thresholds in Rust.
The model asset is validated and cached for the runtime session. Two-hand arrays are
ordered `[right, left]`. Mass is kg, force N, wrist moment N*m, slip m, stiffness N/m,
damping N*s/m and inertia kg*m^2. Changing the asset takes effect when loading a new
runtime session; it does not require recompiling Rust. Missing/invalid physical data is reported,
rather than silently substituted with unrelated weapon mass or a dummy mesh.

The continuous model uses:

| Input | Authority | Effect |
|---|---|---|
| Mass, bounds, optional center of mass/inertia | Actual equipped item metadata | Required force and moment |
| Primary/support grip locations | Weapon presentation descriptor | Support span and lever arm |
| Linear/angular motion and world gravity | Physics runtime | Translational, tangential and centrifugal load |
| Recoil impulse and shot count | Weapon metadata and native combat state | Decaying recoil load |
| Applied health damage and anatomical region | Common damage resolver | Continuous per-hand wound and pain state |
| Remaining vitality and stamina | Native ped/melee state | Available motor support |
| Hit/fall/ragdoll support | Physical reaction state plus authored support data | Transient hand support |
| Slip travel, stiffness, damping | Body grip model asset | Viscoelastic holding constraint |

Forces and moments are compared with available hand/wrist support. Excess wrench
extends a damped grip constraint; release occurs at its physical travel limit.
Pain decays while holding a weapon, with empty hands and while streamed out.
Saved transient pain also decays by elapsed world time on restore; permanent wound state
and equipment ownership are preserved. The constant-load slip equation is integrated analytically.
The same injury can therefore retain a light weapon and release a heavier weapon.
Resting contacts and merely absorbed armour damage do not create wound state.

Anatomical grip regions use the current animated rig segments named in the model
asset, including separate left/right arms. Physics capsule contacts are classified
against those segments when available; the existing capsule classifier remains the
streaming fallback. These are not yet dedicated live per-bone collision receivers.
Explicit `ped.damage` supports `left_arm`, `left_hand`, `right_arm`, `right_hand`.

After release, available fist fighting and escape are compared using authored utility
weights, current hand support, injury, stamina and distance. Merely having a melee
profile no longer forces a severely impaired actor to fight with fists.

Physical acceleration is sampled on the native fixed clock rather than by
dividing a physics velocity change by render-frame time. Duplicate render frames
retain the last observation; skipped ticks use the full elapsed physical interval.
Grounded capsule motion has finite material/gravity-dependent acceleration and
braking, including bounded compensation for the native contact drag. A tactical
stop no longer changes full walking speed to zero in a single native tick.

Ordinary additive hits preserve supported capsule ownership while the authored
combat stun remains active. The former shared recovery timer incorrectly made a
flinch relinquish locomotion control. The native comparison now verifies both
actual hit clips and continued physical ownership, with the same injury and grip
JSON as before.

### Ownership and physical handoff

Each equipped slot has a persisted generation. Native release validates item, owner,
slot and generation, then atomically moves the unique item, payload and pickup pose.
Repeated or late events for an old generation cannot remove newly acquired equipment.
A later death cannot duplicate an already transferred weapon. Reacquiring the same
weapon creates a new generation and allows another legitimate release.

Release samples the resident weapon attachment, falling back to the animated hand
socket and finally the physical body pose during streaming. Rotation, parent point
velocity seed the pickup; automatic grip-loss release also carries sampled hand motion. A bounds-derived obstruction probe
projects the pose away from blocked placement. There are no fixed world-space drop
offsets. Native firearm shots/reloads stop as soon as ownership is released.
Physical reset readiness prevents a newly reset actor from evaluating the retired ragdoll.

ItemsRuntime retains a generic unique-item payload through drop, collection and save.
NPC release writes the remaining magazine/reserve while preserving other instance data.
The Shared player reads that ammunition on the new equipment generation; collection
does not silently refill a firearm. Stale player ammo writes cannot resurrect a dropped
item or overwrite another equipment generation.

Project item definitions are recursively overlaid onto canonical Shared descriptors
before registration. A project overriding reserve capacity cannot erase physics, model,
grip or audio namespaces. Explicit nested null remains an explicit override.

The common `ped.weapon.release` command accepts `actor_id`, `item_id` and
`equipment_generation`. It is the checked action-event entry point for authored
release/disarm actions. Arbitrary GTA clip Object.Release/Destroy tags are not yet
automatically imported or dispatched to that entry point.

### Reference boundary

Read-only inspection of GTA source confirmed action/reaction drop attributes,
animation object-release tags, equipped-weapon pickup construction, inherited
motion, ammunition/component handling and obstruction projection. The shipped
reference's `CanDropWeaponWhenShot` path is compile-disabled. The continuous wrench
and slip model above is a NewViso adaptation, not a claim about GTA's hidden grip formula
or a replacement for Endorphin's complete muscle/finger/contact simulation.
See `combat_reference_audit.md` for the inspected functions and remaining work.

### Acceptance

- `cargo test -p newviso-agent -p newviso-items -p newviso-runtime --lib --offline`
- `python scripts/verify_combat_loot.py` -- death, physical collection and ammunition.
- `python scripts/verify_combat_loot.py --scenario adaptive` -- equal injury, actual
  rifle/pistol physical data, different retention outcomes.
- `python scripts/verify_combat_loot.py --scenario data_override` -- an isolated
  project VFS body-model override changes retention with the same compiled executable.
- `python scripts/verify_combat_loot.py --scenario rearm` -- reacquire, reject old event,
  release a new generation, collection, subsequent death and reset.
- `python scripts/verify_combat_loot.py --scenario arm`
- `python scripts/verify_combat_loot.py --scenario heavy`
- `python scripts/verify_combat_loot.py --scenario critical` -- cumulative injury,
  without asserting a fixed HP percentage.
- `python scripts/verify_combat_loot.py --scenario ballistic` -- actual physics projectile.
- `python scripts/verify_combat_loot.py --scenario action` -- checked action release.
- `python scripts/verify_combat.py`, `python scripts/verify_melee.py`
- Architecture, isolation and direct-console-output verifiers.

Encounter restart removes native pickups by source ownership and clears their physical
bodies. The player controller remains external; this NPC retention system does not take
over player input. Generic ambient loot tables, autonomous weapon retrieval, a complete
animation-event importer remain separate work.

## Shared physical authority

The host uses the provider-independent Endorphin control-authority decision for
movement, Director availability and voluntary combat admission. Recovery,
vehicle response, pending reset and articulated ownership can no longer disagree
between those consumers. A supported additive hit retains locomotion while
suspending attacks and guard.

Articulation immediately cancels an existing guard/windup and invalidates its
sampled strike. Guard commands cannot re-enable an upright defense on a physically
owned body. Disabling guard remains accepted. This closes a native regression in
which a living articulated NPC still reported blocking=true. The new
authority scenario in verify_endorphin.py checks both rigs, physical transfer,
death/reset and the later return of normal combat permissions.

## Endorphin body response

The separate `newviso-endorphin` core now plans articulated bodies, preserves
momentum at physical ownership transfer, evaluates adaptive recovery and reuses
physical-pose storage. Hard landings and vehicle knockdowns can use the physical
skeleton while the actor survives. Get-up requires supporting contacts, settled
part energy and free standing space; physical reset clears the retired skeleton.
The authored rig/material/recovery model is `physics/endorphin/humanoid.json`.
See [Endorphin](endorphin.md) for ownership, tuning, native acceptance and remaining
active muscle/balance and pose-matching limitations.

## Swept melee and cold weapons — 2026-10-23

`newviso-collision::StrikeCapsule` continuously sweeps a complete segment and radius
against moving receiver capsules and resident solids, including triangle/shaft contacts.
Sampling runs after the final skeletal pose and weapon attachment update. Damage
requires actual contact during the authored active interval; windup also checks
world obstruction. One sequence consumes one contact. Equipment generation,
residency, interruption and live vitals are rechecked.

`combat/melee/unarmed.json` and `combat/melee/knife.json` author timing, effort,
guard penetration, damage type, wrist trajectory and joint/weapon bindings.
Rig-specific joint maps support the player and imported male characters.
The committed action facing drives both NPC arm pose and skeleton heading,
including manual attacks without an AI turn task.

The existing knife supports slash/thrust, cut/puncture damage, actual attached
blade geometry, stamina, interruption, obstruction, live physical release and
collection. Firearm ammo payloads are written only for firearms. Other cold
weapons require authored actions, bindings and assets. This is not the complete
GTA catalogue, a physical finger/parry solver or an animation-event importer.

## Tactical Combat Director — 2026-10-23

`combat/director/humanoid.json` supplies sensing/probe budgets, slot limits,
angular separation, leases, utility weights and movement clearance.
Readiness, capacity, stamina, distance, waiting and threat determine active,
supporting and observing roles. Running attacks retain their leases; death,
invalid observation and lost residency release them. Allocation gates actual
firearm and melee requests.

Waiting priority is discharged by an actual melee-action or shot counter advance,
not by receiving permission to approach. A lease that expires before contact
therefore cannot repeatedly erase an actor's priority. Local steering includes
a slow approach candidate derived from the free capsule gap and prediction horizon;
separation and collision-horizon costs are continuous. Supporting actors remain
collision obstacles rather than being ignored to make an attack succeed.

The host verifies candidate floor, standing volume, a direct clear route and
sightlines against resident shapes. AABBs propose positions; they do not invent
occluders. Arrival precision follows authored clearance so peeking reaches the
verified sightline. Support choices include flank, observe, guard, retreat and
recover. Observed windup/recovery enables guard/counter decisions.

Ranged actors retain cover through occluded reloads, then try verified peeks.
Hidden relocation cannot update observed memory or authorize fire. Expiration
cancels the engagement task. Planning is bounded and local; it does not replace
global navigation or implement general squad orders.

Native Release proofs: `verify_melee.py --scenario unarmed`, `--scenario weapons`
and `--scenario director`. Reports record executable/provider hashes.

## Release acceptance — 2026-10-23

The current Release uses executable
`604a15c93fd18da85df9bef8ff849114e1e9c06ef49ef93bb3aaae084035c315`
and deployed Bullet provider
`3ff85f4ede43a7755b209a7ab8ae0dafca857d65d007f63cf2cf4fb03cafd55c`.

Unarmed, knife, tactical Director, firearm combat, adaptive retention, VFS grip
override, rearm, strong injury, hand injury and checked action-release scenarios
passed. Director passed all three Release runs; adaptive retention passed all four.
Repeats are separate from the 22 unique Endorphin/combat scenarios. Two strict
standing cases failed: the combined unique matrix is **20/22**.

NPC presentation obtains current position from the resident physical body when the
throttled actor list is absent. Ped snapshots publish the action's world clock.
Strike correctness does not depend on increasing world-snapshot frequency.
Hit support follows the displayed additive blend weight. The live comparison now
observes both actual hit clips and continued supported capsule ownership.

A former instantaneous tactical stop could release a healthy pistol through a
one-tick velocity discontinuity. Grounded drives now budget acceleration/braking
using resident material friction and gravity, accounting for bounded native contact
drag. Grip-load derivatives use complete fixed-physics intervals. Body/grip JSON
and equal 20-point torso injury were unchanged: the rifle releases and the pistol
remains held while both actors survive at 80 vitality. Strong/hand injury still
causes live release. Pickup preserves ammunition; later death cannot duplicate it.

Director waiting now persists until an actual attack or shot starts; receiving an
approach lease no longer counts as service. Steering admits a gap/horizon-derived
slow approach and continuous costs at zero separation velocity and the prediction
horizon. Existing assertions for all three participants, death reassignment, guard,
counter, reload cover, peek, hidden relocation and memory expiry remain intact.

The action-release fixture supplies known owner velocity and checked release in
one ordered command batch, retaining its exact parent point-momentum assertion.
Automatic grip-loss release additionally includes sampled hand motion. Fixtures
use the coherent ped world clock; temporary diagnostic traces were removed.

Selected engine suites: **432 passed, 4 ignored**. Provider: **56 passed, 1 ignored**.
Release, workspace cargo check, boundaries, isolation, direct-console-output and
layout passed. Code-health and broader workspace documentation gates still fail.

These results do not establish stable active-ragdoll gait, complete recovery, a
full cold-weapon catalogue or GTA parity. See [Endorphin](endorphin.md).
Receipt: `endorphin-work/20261023/final-realism-verification.json`. Individual
`melee-*-proof`, `combat-proof` and `combat-disarm-*-proof` reports
are under `NewViso/.newviso/cache`. Earlier failed worker runs are retained.

## Geometry-bound NPC firearm presentation

NPC arm/weapon presentation now follows all resident native peds, including
ambient actors outside engine_combat.enemies. The encounter list still owns only
encounter setup/reset. A per-rig weapon binding comes from the Endorphin asset;
explicit authored aim_rig settings remain available. The rig's semantic joint
map is also inherited by native melee, with explicit melee overrides preserved.

The displayed body keeps its movement-facing policy. Targets outside the
authored front-facing envelope release arm aiming until the body turns.
Measured per-NPC shoulders and arm reach replace fixed NPC shoulder offsets and
the inherited player's dimensions. Weapon-authored stance ratios scale with this
reach. Missing/incompatible joint bindings publish diagnostics instead of
applying a different model's hand names.

The native firing path checks both the body's aim envelope and the equipped
weapon's solved sight direction before consuming ammunition. Its origin is the
actual attached muzzle and its direction is the barrel ray plus authored spread.
An unavailable or misaligned prop produces no shot. This closes the previous
conflict where movement faced one way while hand IK and ballistic rays aimed
independently behind the torso.

Regression: scripts/verify_endorphin.py --scenario weapon_aim.
Add --ambient --semantic-map to use a spawned NPC outside the encounter list and
opaque author joint names retargeted through the common binding.


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
