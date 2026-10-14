# Vehicle specifications and damage state

This pass extends the common vehicle reference port with executable model
capabilities, symmetric state events and native XML specifications. It does not
complete specialised class dynamics or game/network event parity.

## Owners and authoring

`crates/newviso-vehicle/src/specification.rs` owns source-neutral capabilities.
`application_vehicles/specifications.rs` reads the Shared VFS catalog once per
application, validates it, and applies a profile when a model binds to a vehicle.
The final model path segment is matched case-insensitively with `_hi` removed.
The optional catalog is `Shared/Content/config/vehicles/specifications.xml`, with
schema `newviso.vehicle.specifications.v1`. Invalid schemas, duplicate names,
unknown fields, negative/nonfinite scales and invalid regions fail explicitly.

Generate the catalog from local metadata; repeat input arguments to overlay
additional packs, with later definitions winning:

```powershell
python scripts/import_vehicle_specifications.py `
  --handling ../Shared/Source/vehicles/reference/data/handling.meta `
  --vehicles ../Shared/Source/vehicles/reference/data/levels/gta5/vehicles.meta
```

The supplied base metadata has 248 handling records and generates 298 model
specifications: 7 electric models and 2 fuel trailers. Unresolved handling IDs
fail import. This count covers these inputs, not every installed DLC or model.
The source tree remains unchanged.

Model name, handling ID, layout, source class/type, model/handling/door damage
masks and named flags remain available in telemetry. The catalog executes only
the capabilities below. It does not import all numeric handling, geometry,
seat layouts or specialised sub-handling. Numeric dynamics still belong to the
authored `VehicleDefinition.handling`; `handling_id` alone does not select them.
The existing reference-handling importer now retains `m_fWeaponDamageMult`.

## Capability mapping

| Source property | Native behavior |
| --- | --- |
| `MF_INDESTRUCTIBLE` (`0x04000000`) | Blocks ordinary damage, oil-induced health loss, autonomous fire damage and explosion. |
| `MF_NO_WHEEL_BURST` (`0x02000000`) | Automatic tyre damage and burst progression cannot puncture or burst tyres. |
| `MF_NO_WHEEL_BREAK` (`0x40000000`) | Automatic wheel/suspension progression cannot detach wheels. Suspension can still be damaged. |
| `MF_NO_EXHAUST` (`0x00001000`) | Suppresses exhaust presentation. |
| `HF_ARMOURED` (`0x08000000`) | Basic bullet proof; special armour, weapon and glass branches remain separate. |
| `HF_NO_HANDBRAKE` (`0x10`), `HF_NO_REVERSE` (`0x200`) | Constrains control input; prohibited manual reverse is rejected. |
| Rear/all/handbrake rear steering (`0x20`, `0x80`, `0x40`) | Overrides configured wheel steering selection. |
| `FLAG_IS_ELECTRIC` | Electric motor has no combustion oil/fuel and no exhaust. |
| Trailer/bicycle source type | No engine; bicycle pedal/rider dynamics are not implemented by this capability. |
| Fuel trailer explosion flags | Fuel tank exists independently of a motor. Special contact explosion/spray behavior is not implemented. |
| Reference leak defaults | Imported profiles use `player_only`; generic native defaults remain `enabled` for compatibility. |

Native overrides also expose engine/tank damage gates, engine fire/misfire
gates, independent body/engine/tank damage scales and leak policies
`disabled`, `player_only`, `enabled`. Damage regions are optional spheres with
`center` in engine-local coordinates and a positive `radius`. Explicit component
hits remain authoritative; missing regions retain the prior spatial fallback.
The metadata importer does not invent regions from reference render geometry.

## Script contract and state

`vehicle.specification.set` and `Vehicles.specification(target, specification)`
replace the complete specification, filling omitted fields with native defaults.
Preserve existing fields when changing a single capability:

```typescript
const profile = car.specification;
const command = Vehicles.specification("test.rancher", {
  ...profile,
  damage: { ...profile.damage, oil_leaks: "disabled", tyres_can_burst: false },
});
```

An explicit script override survives subsequent model binding and repair.
Replaying an unchanged profile emits no `specification_changed`. Updating a
profile preserves wheel damage by wheel identity and fluid fractions; changing
from electric to combustion leaves a newly available tank empty until refuel or
repair. Short repair commands preserve handling, fluid capacities and automatic
wheel ownership. Explicit authored tyre/part commands remain authoritative.

Telemetry exposes the effective specification, `driveable_player`,
`driveable_ai`, `engine_output_multiplier` and `powertrain_state`:
`off`, `starting`, `running`, `fuel_starved`, `disabled`, `wrecked`, `unpowered`.
`engine_running` reports actual ignition state. An already-running player engine
can retain motor output while burning, while AI driveability is false; ignition
stops at the fire finish threshold. This is not exact reference torque parity.
Plane/heli health misfires affect output even with full fuel. Oil-loss damage
also respects the upside-down factor and capability/proof gates.

Weapon damage uses the handling weapon multiplier; collision does not.
Aircraft fire damage uses the reference class factor. Fatal bullet/explosion
tank damage is resolved on the next frame. Explosion proof can defer a terminal
tank explosion; removing it resolves the pending wreck once. Repair, forced
explosion, profile changes and normal frame progression clear active effects
and publish their corresponding end transitions.

## Added event topics

All topics use the existing `engine.vehicle.` prefix, after-commit delivery and
exact decimal sequence strings. There are now 64 kinds, with these 8 additions:

| Kind | Meaning |
| --- | --- |
| `damage_applied` | Accepted core damage transaction, with raw damage and compact before/after health, fluid and operational snapshots. |
| `specification_changed` | Effective profile changed, with complete before/after specifications. |
| `driveability_changed` | Player and/or AI damage driveability changed. |
| `vehicle_restored` | At least one damage driveability state became available. |
| `oil_leak_stopped`, `petrol_leak_stopped` | Active leakage ended through repair, depletion, context or profile changes. |
| `engine_fire_stopped`, `petrol_fire_stopped` | Active fire ended through repair, completion, profile changes or explosion. |

Driveability events describe core damage state, not every solver/occupancy
restriction. A protected/rejected hit emits no `damage_applied`. The snapshot
does not include later fragment mesh/debris mutations. Existing misfire start/end
events now cover both fuel and aircraft-health triggers. Repeated repair can
still acknowledge `vehicle_repaired`; transition events require a real edge.

## Verification and remaining scope

`cargo test --offline -p newviso-vehicle -p newviso-runtime --lib` covers 63
vehicle and 58 runtime tests. The real-provider specification scenario has 31
checks over catalog binding, protected controls/tyres, after-commit payloads,
leak enable/disable, repair preservation, burning-engine output, electric drive,
explosion, restoration and exact event sequences:

```powershell
python ../Projects/VehicleTest/tools/verify_specification.py
```

The runner accepts `NEWVISO_TEST_ENGINE_EXE` for a separately built binary.
The current executable was locked by an existing process; verification uses
`.newviso/build/vehicle-specification/release/newviso.exe`. Evidence is in
`.newviso/verification/vehicle-specification` and the VehicleTest generated
summaries. See [vehicle-reference-port.md](vehicle-reference-port.md) for the
complete regression commands and current run results.

Remaining work includes source-authored region extraction, special armour and
weapon rules, recent-player/network leakage context, exact steering/transmission
branches, bicycle/motorcycle rider state, aircraft and watercraft breakdown,
trailer/rail articulation, NPC/crime/mission responses, replication/persistence,
GPU deformation and exact fragment collision constraints. Retained source flags
are not evidence that those behaviors execute.
