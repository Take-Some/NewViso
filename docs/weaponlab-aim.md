# WeaponLab weapon aim and presentation

The shared gameplay modules calibrate military pistol, Mini-14 and Malorian iron sights from their model coordinates. `weapon_sights.ysc` maps the rear/front sight line onto the rendered camera ray, including camera roll. First-person wrist reach changes sight depth instead of shifting the gun sideways. `weapon_recoil.ysc` applies an upward angular kick, bounded burst accumulation and frame-rate-independent return. The central dot remains at the viewport center during ADS.

After the first-person result was approved, its pose and alignment were preserved. `weapon_third_person_pose.ysc` owns the separate external stance: pistol grip below the head, relaxed outward elbow poles, support grip on the same weapon, and position/orientation motion sampled from animated shoulders and head. It does not replace the first-person solver. Reload lowers the aiming IK blend; firing animation and joint attachment remain on the actual weapon.

Native arm snapshots expose socket rotation, wrist position and measured anatomical reach, allowing presentation to use the rig's actual bone lengths rather than a hardcoded approximation. Exact receiver attachments for hit marks remain in use.

Validation on the release engine:

- 165 scene tests passed, 3 previously ignored tests remain ignored; release build passed.
- 81 sight geometry combinations passed, including yaw, pitch and roll; recoil recovery matched at 30/60/144 Hz; the ADS dot is centered in both camera modes.
- First-person integration: all three firearms, 3 shots, 2030 native pose measurements, sight and hand errors below 0.006/0.008 m thresholds. Live integration covers pitch +/-0.7 radians. The approved first-person behavior at more extreme pitch was not changed.
- Third-person integration: all three firearms, 3 shots, 2144 native pose measurements, hand errors below 0.008 m. Weapon movement from walking animation measured about 0.058/0.067/0.058 m for military pistol/Mini-14/Malorian.
- Target-yard regression passed: firing, cooldown, reload, switch cancellation, focus guards, target destruction, all 144 physical fragments, moving/rotating attached hit marks, reset and ammunition refill.

Run from `Projects/WeaponLab`:

```text
node --experimental-strip-types tools/test_weapon_aim.mjs
python tools/verify_weapon_aim.py
python tools/verify_weapon_aim.py --status
python tools/verify_weapon_aim.py --third-person
python tools/verify_weapon_aim.py --third-person --status
python tools/verify_weapon_lab.py
python tools/verify_weapon_lab.py --status
```

The supervisors launch asynchronously. Results and engine logs are under `.newviso/cache`. Normal play remains `Run.cmd`: click to capture, mouse to look, RMB to aim, LMB to fire, R to reload, V to change view, Escape to release.

A normal play session exposed a previously implicit one-voice concurrency policy on firearm cues. Weapon YSCDs now allow 12 overlapping fire/empty instances, with oldest replacement, and 4 handling/reload instances. Native stress verification passed 51 overlapping sounds across all three dictionaries. Encoded clip bytes are preserved; all three rebuilt NEF8 dictionaries passed hash validation. Editable inputs and the lossless concurrency rebuild are in Shared/Source/audio/weapon. Run `python Shared/Source/audio/weapon/build_weapon_audio.py` from the workspace to rebuild them; the helper uses the cached BLAKE3 crate offline. Run `python tools/verify_weapon_audio.py` and `python tools/verify_weapon_audio.py --status` from WeaponLab for the native audio test.
