# Third-person camera stability

Shared gameplay cameras separate user look, the follow boom and optical effects.
All controllers using GameplayCamera/GameplayView receive the same behavior.

The follow boom uses the player's requested yaw and pitch, configured distance,
shoulder and height. Automatic recoil and impacts do not orbit this boom.
Third-person optical effects default to pitch only: heading, roll and positional
shake have zero gain. Deliberate horizontal mouse input continues to turn the
view; strafing, animation, head motion and recoil cannot add an unwanted heading.

These gains are engine data under engine_player.camera.third_person_effects:
pitch_scale, heading_scale, roll_scale and translation_scale, each in [0,1].
Defaults are 1,0,0,0. First-person authored recoil/head/shake behavior is retained.

## Collision contract

A scene.camera.set command can opt into collision.preserve_direction = true.
When physics shortens the camera boom, the target translates with the resolved
camera position. The view direction remains unchanged. Without the flag an
authored world-space look-at target keeps its original position.

GameplayCamera uses a target one unit in front of the requested camera. The old
handler shortened only the camera position. If the corrected boom became shorter
than this one-unit target offset, the target could end up behind the camera.
A native reproduction measured a direction dot product of -0.94129 with the
requested forward vector. This was a true reversal, not character animation.

## Verification

node scripts/verify_third_person_camera.mjs exercises recoil, impacts, moving
anchors, yaw wrap and 30/60/120/240 Hz, as well as first-person behavior and
configurable effect gains.

python scripts/verify_third_person_camera.py --adapter direct --tag direct
runs the native collision contract, optical effects and follow probe.
Adapters shared and FirstFPS also exercise real mouse input, view switching,
walking, strafing, vertical look, deliberate turning and blocked/clear booms.
Receipts and captures are under .newviso/cache/third-person-camera-work.

## Accepted validation, 2026-10-28

Release built; the workspace check passed. Runtime tests: 182 passed, 0 failed,
1 opt-in benchmark ignored. Shared camera checks: 3606 passed, covering
30/60/120/240 Hz. Existing shared motion checks (324) and the recoil-pose
regression also passed. The shared runtime XML parses and scoped git whitespace
checks are clean.

Five native scenarios passed with one executable SHA-256:
956b78b40bd5b158dde5384bb4fadb8a0888c137ae9baf46a733a6b84d0b4491

| Native scenario | Result |
| --- | --- |
| Direct camera: constrained boom, recoil, impact, moving anchor and user turn | Passed |
| Actual Shared controller: V toggle, walking, strafing, vertical look and user turn | Passed |
| Actual FirstFPS controller: same third-person behavior, blocked and clear boom | Passed |
| FirstFPS first-person movement/jump/material and compiled coverage regression | Passed |
| Original FirstFPS stair flight and descent with two physical NPC skeletons | Passed |

The three camera probes checked 6350 actual native poses; every checked view
retained a dot product above 0.9999 with its requested direction. Zero horizontal
input also retained the requested heading through impact effects. Deliberate
horizontal input remained functional. Shared and FirstFPS captures were visually
inspected: upright horizon, visible complete head, camera following the body.

The authoritative aggregate is
.newviso/cache/third-person-camera-work/acceptance.json, with source and native
receipt hashes. Build/run evidence is implementation.json. Baseline-valid logs
record the original boom recoil and native reversal failures.
