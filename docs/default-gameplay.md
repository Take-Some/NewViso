# Default game capabilities

Every normal NewViso project receives Shared engine scripts before its project scripts,
including projects without a `scripts` manifest block. Disabling project scripts disables
only the project hooks. Safe mode continues to suppress scripting.

## Shared owners

- `Shared/Content/config/engine/scripts.defaults.xml`: provider and engine lifecycle modules.
- `Shared/Content/config/engine/environment.defaults.xml`: sky dome, cloud and weather defaults.
- `Shared/Content/config/engine/runtime.defaults.xml`: default player, input, camera and physics settings.
- `Shared/Content/scripts/newviso/default_game.ysc`: world/player lifecycle.
- `Shared/Content/scripts/newviso/world/default_world.ysc`: atmosphere, default sun/moon and sky clock.
- `Shared/Content/scripts/newviso/gameplay/player.ysc`: physical player and locomotion.
- `Shared/Content/scripts/newviso/gameplay/player_camera.ysc`: first/third person camera.
- `Shared/Content/scripts/newviso/gameplay/player_presentation.ysc`: body visibility and the authored arm skeleton.
- Shared model, textures, skeleton and locomotion dictionary: reusable default character assets.

The host composes the engine and project lifecycle graphs. It loads Shared settings, deep-merges
project settings, activates a physics backend automatically for the Shared player, and exposes
generic commands. Character selection, movement policy and view switching are authored in Shared
TypeScript. The sky is assigned by `scene.sky.dome.set` during Shared startup before GPU allocation.

Sky geometry alone is not a visible atmosphere. Shared startup additionally applies
the complete nonzero atmosphere profile from engine_world.sky.atmosphere in the
Shared runtime XML. The Shared sky controller creates a sun atmosphere driver and
moon visual, and advances their orbit with the configured 48-minute day. This
initializes day/night sky colors, cloud opacity, stars and the silver lining.
The default moon billboard comes from the Shared sky texture dictionary.
VehicleTest and ProceduralMap inherit this controller automatically.
FirstFPS, ForestChop and NorthYankton explicitly set engine_world.sky.controller
to "project" because their lighting scripts already own celestial/weather state.
Their atmosphere baseline remains overrideable by project commands.


The Shared player waits for the map startup barrier and configured streamed colliders before
spawning its dynamic capsule. It uses resolved physics positions, including presentation interpolation,
and a camera collision sweep. Movement uses WASD, Shift to run, Space to jump, V to switch views,
left-click to capture and Escape to release. Third person supports mouse-wheel zoom.
First person samples the real head after animation and IK. Accurate aim uses the calibrated
eye/sight line and an authored near clip; shots start at the constrained camera without
a forward offset that could skip nearby walls.
Head, eyes, mouth, hair and neck geometry are excluded by mesh prefixes and skeletal
subtrees. Shared skin meshes are partitioned by skeletal influence during model
preparation, so hiding a head never requires hiding the hands in the same material.

The Shared presenter keeps the authored character body and arm skeleton in first person.
Only head/neck geometry is filtered from the main view, so relaxed locomotion leaves both
arms where the real character animation places them instead of creating a separate raised
camera-relative FPS viewmodel. The same body remains the shadow caster in both views.
Equipped rigged weapons are constrained grip-joint-to-hand-socket after animation sampling,
so first- and third-person views share the same hand/weapon relationship. Weapon-specific
aim, finger, reload and tool animations can override the locomotion pose in project scripts.

Existing FirstFPS, ForestChop, ProceduralMap, VehicleTest and NorthYankton controllers
all use this Shared presenter. ForestChop and ProceduralMap also receive a visible
character and V view switching. Prologue cutscenes restore the full character and
suppress the hand presentation while the mission owns the actor and camera.

## Project overrides

A new project needs no player or sky imports and no explicit physics capability declaration.
An empty project runtime override still inherits the complete base:

```json
{ "schema": "newviso.project.runtime.v1" }
```

A project changes only the fields it needs:

```json
{
  "schema": "newviso.project.runtime.v1",
  "variables": {
    "engine_player": {
      "camera_mode": "third_person",
      "spawn_position": [0, 0, 12],
      "walk_speed": 2.0
    }
  }
}
```

`spawn_position` is the character's feet position in world metres. When null, the authored
scene camera is used as the eye position.

Existing games with their own player controller explicitly use
`variables.engine_player.controller = "project"`. Their project hooks own the player and camera;
the mandatory Shared world lifecycle still installs the sky. `enabled = false` is an explicit
project override for games that do not need a playable character.

A project may override `scripts/newviso/gameplay/player.ysc`,
`scripts/newviso/gameplay/player_camera.ysc`, or the complete
`scripts/newviso/default_game.ysc` through the standard higher-priority project VFS.
A logical engine root declared again as a project root is invoked only once.
Project hooks run after Shared hooks, so their commands take precedence.

Default ambient illumination is authored in `variables.engine_world` so a new primitive map
and its character remain visible without a project lighting script.
Projects with authored lighting explicitly override `engine_world.ambient_enabled = false`.

Sky overrides remain in the project environment file. Omitted sky fields inherit the Shared XML.
An explicit `"sky": null` disables the sky for that project.
Sky resource assignment is startup-only; changing weather/time/cloud parameters at runtime uses
the existing generic scene commands.

## Validation

`cargo test -p newviso-runtime engine_scripts` covers lifecycle composition, no-script projects,
disabled project scripts, duplicate roots and incompatible providers.
Real runtime verification must additionally load a project with no game scripts or physics declaration,
and exercise the Shared controller and project VFS override.

Run the deployed-runtime regression check from the workspace root:

    python Scripts/Verify/verify_default_gameplay.py

It creates disposable projects and checks automatic Shared activation, disabled project scripts,
explicit controller replacement, script VFS overlays, partial settings inheritance,
physical grounding, movement, jumping, two-way view switching and UI input gating.

Run the project controller view probes separately:

    python Scripts/Verify/verify_shared_player_views.py

They exercise V in both directions, head draw policies, body-authored arm retention,
absence of duplicate FPS hand geometry and GPU sky residency in all five project controllers. Probe projects
reuse the authored map/content and copy scripts into a disposable test project.
FirstFPS excludes its startup particle spawns because its raw RSC7 core.ypt
currently has no deployed runtime codec. NorthYankton runs its player controller
without an authored cutscene so V can be tested during gameplay. These probes do
not replace the complete project startup smoke:

    python Scripts/Verify/verify_shared_player_projects.py

The complete FirstFPS startup currently reports the uncooked particle asset error;
the other four complete project starts and all five controller view probes pass.

The input provider library remains mapped for the process lifetime because native gamepad
worker threads can survive its shutdown callback. Shutdown and registry cleanup still execute.
This prevents those workers from resuming in unmapped code during engine termination.


## Generic scene commands

- `scene.entity.main_view_meshes.set`: `id`, `hidden_mesh_prefixes` and optional
  `hidden_joint_roots`; empty arrays restore the complete main view.
- `scene.entity.joint_mesh.copy`: copies a selected skeletal subtree into an existing
  entity using `source`, `joint_roots`, `root_joint`, `tip_joint`, `direction` and
  `up`. It retries harmlessly while the source model is streaming.
- `scene.main_view_geometry` runtime telemetry reports submitted hand geometry and
  excluded/retained source vertices for regression verification.

Character anatomy, weapon hand sockets and eye clearance are authored in
`engine_player.first_person` in Shared XML. Each project can override only its bone
names/socket mapping without duplicating the presentation implementation.



## Reference weapon and shooter camera port

`NewViso/scripts/import_weapon_camera_reference.py` extracts the installed `update.rpf`
camera/weapon metadata and resolves field names using the original `CameraMetadata.psc`.
`Shared/Content/config/engine/weapon_camera.reference.xml` preserves all 91 weapon records,
the selected shooter-camera dependency closure and its matching gameplay director.
The generated `weapon_camera_reference.ysc` is a runtime projection, with source SHA-256
values and explicit corrections for malformed source vectors.

`camera_dynamics.ysc` ports `camDampedSpring`, the ADSR envelope phases, sine and damped
sine oscillators, and shake instance/interval limits. It preserves the reference's
exponential critical-damping branch, sine easing and phase overshoot behavior.
`gameplay_camera.ysc` consumes real head animation deltas relative to the rig root,
with the imported bind-root orientation removed and camera pitch/heading/roll decomposition,
uses authored gait weights and aim suppression, interpolates the reference FOV, and
applies ordered rotation/translation/FOV shake components. Its first-person anchor is
sampled again after animation/attachments. Vehicle cabin look remains relative to the
seated actor. `gameplay_view.ysc` fixes the equipment → constrained camera → ballistic
shot → weapon IK → rendered recoil command order.

The common firearm state machine handles semi-auto trigger caching, held automatic
cadence, magazine/reload transfer, cancellation on weapon changes, recoil accuracy
recovery, batch spread, muzzle-joint flashes and audio/weapon-animation hooks.
AccuracySpread is a linear cone offset normalized at 120 m, **not degrees**.
First-person shots use the camera origin. Third-person shots project the muzzle depth
onto the camera sight line, with the original 0.15 m subtraction. Native commands sample
the already constrained camera before adding cone spread.

Explicit `metadata.weapon.reference_name` selects a source profile. Military pistol uses
`WEAPON_PISTOL`, Mini-14 uses `WEAPON_CARBINERIFLE`, and Malorian uses `WEAPON_PISTOL50`.
This changes their clip size, cadence, automatic flag, damage/range/falloff and recoil
profile to those reference weapons while retaining the existing models, grip calibration,
audio and gun animations. Set `reference_ballistics: false` to retain custom balance.
The prologue's carbine, pistol and shotgun also use the common firing state machine.

Settings: `engine_player.camera.first_person_fov_slider` selects the authored 45–55 degree
range, and `head_bob_enabled` controls animation-derived head motion. Camera effect events
use `game.camera.shake` with `{ ref, amplitude, actor_id? }`, and `game.camera.shake.stop`
with `{ group?, actor_id? }`. `engine.camera.effects` reports current head blend, FOV,
active shakes and unsupported channels.

This ports the available on-foot/cabin camera and instant-hit firearm paths. It is not a
complete replacement for every GTA task or renderer effect. The original cover/lock-on
and sticky-aim task graph, scopes/weapon attachments, thrown/projectile weapons,
full ped reload/aim clipsets, vehicle entry/exit camera interpolation, ragdoll/death
transitions, DOF and motion blur still require their corresponding engine subsystems.
Imported settings alone do not implement those paths. Custom character/weapon assets
also prevent a pixel-identical result with the original game.

Run `python Scripts/Verify/verify_weapon_camera.py` for the reference math/state golden
checks and real VehicleTest input/shot integration. The existing
`verify_shared_player_views.py` covers all five controllers. Native tests under
`application_script_commands::physics::camera_shot_tests` verify camera origins and the
world-up accuracy cone independently of script-generated directions.
