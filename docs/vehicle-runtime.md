# NewViso Vehicle Runtime

## Scope

Vehicle simulation lives in the source-format-neutral crate:

- crates/newviso-vehicle
- runtime bridge: crates/newviso-runtime/src/application_vehicles.rs
- fixed-step physics integration: crates/newviso-runtime/src/application_physics.rs

The implementation is clean-room: the reference vehicle stack was used to identify
behavioral responsibilities and data semantics, while NewViso owns its own data
types, algorithms, physics-provider integration and scripting contract.

## Reference responsibilities reproduced

The runtime is split into the major responsibilities that a production vehicle
stack needs:

1. Handling data and one-time authoring-unit conversion.
2. Automatic transmission / clutch / engine-speed state.
3. Per-wheel suspension ray probes.
4. Spring, compression damping, rebound damping and progressive bump stop.
5. Front/rear suspension bias and anti-roll load transfer.
6. Steering at the tyre contact basis.
7. Longitudinal slip, lateral slip angle and a peak/falloff traction curve.
8. Friction-circle clamping of combined longitudinal/lateral tyre forces.
9. Front/rear drive bias, driven-wheel torque distribution, brakes and handbrake.
10. Wheel angular speed and rotation telemetry.
11. Speed-dependent aerodynamic drag and vehicle downforce.
12. Bike roll stabilization that fades out at low speed.
13. Plane thrust/lift/sideslip and pitch/roll/yaw control.
14. Helicopter collective/cyclic/yaw stabilization.
15. Boat/submarine thrust, resistance, buoyancy and steering hooks.
16. Physics-provider-independent runtime; NewViso maps generated impulses and
    wheel probes onto the active engine.physics provider.

Vehicle simulation runs inside the same fixed-timestep loop as the physics
backend. Suspension queries produced by tick N are consumed before tick N+1;
when multiple fixed substeps are required in one rendered frame, vehicle
simulation participates in every substep.

## Handling units

ReferenceHandlingData is an authoring-space structure with aliases for the
reference handling field names. HandlingData::from_reference_units converts
once into runtime units.

Important conversions currently implemented:

- max flat velocity: km/h -> m/s;
- steering and lateral traction angles: degrees -> radians;
- authoring drag coefficient -> runtime drag scale;
- suspension compression/rebound damping -> runtime damping scale;
- drive, brake, traction and suspension front biases are normalized to 0..1.

The engine runtime only consumes HandlingData; conversion is not repeated per
frame.


## YFT fragment and presentation layer

YFT is a first-class semantic model source. The RSC7 codec emits the normal
northstar.model.runtime.v1 payload plus an optional
northstar.model.fragment.v1 block. Fragment geometry remains ordinary
ModelResource geometry; the fragment block supplies semantic articulation
metadata.

Imported parts are classified into roles such as body, wheel, suspension,
wheel_hub, door, bonnet, boot, glass, body_panel, breakable, extra, light,
siren, exhaust, engine, seat, weapon_mount, roof, spoiler and steering.

For the four canonical wheel families the importer preserves semantic wheel
slots: front_left, front_right, rear_left and rear_right. When vehicle.upsert
does not provide an explicit wheel layout, NewViso derives wheel centers,
radius, width, suspension travel, steering, driven axle and opposite-wheel
pairing from the imported fragment. Explicit project-authored wheels always
override this automatic layout.

Fragment vehicle instances use entity-local mutable vertex ranges. The scene
therefore can rotate/translate a wheel, door or panel without mutating another
instance of the same model. Presentation state is rebuilt from immutable bind
geometry, preventing cumulative transform drift.

The live presentation pass currently drives:

- wheel spin, steering and suspension travel;
- suspension and hub travel;
- doors, bonnet and boot opening;
- steering-wheel rotation;
- collision-driven panel deformation;
- glass/breakable visibility after damage;
- head lights, brake lights, reverse lights, indicators/hazard and siren lights;
- looping engine, tyre-skid and siren audio;
- YPT exhaust and tyre particle effects at imported part pivots;
- seat/occupant presentation attachment.

Impact damage is projected from physics contact points into vehicle-local space
and assigned to the nearest damageable imported fragment part. Glass and
breakable/light parts have higher fragility than body panels. The current
deformation is articulated/parametric; it is not a soft-body vertex-crumple
solver.

## Surface grip policy

Physics keeps collision surface ids opaque. Vehicle policy maps those ids to
source-neutral semantic classes such as asphalt, concrete, gravel, dirt,
grass, snow, ice, sand, mud, metal and water.

Per-wheel traction multiplies the tyre curve by the selected surface profile.
The profile blends dry_grip toward wet_grip using accumulated scene wetness
(and immediate authored rain), then toward snow_grip using active snow/snow
mist. The resulting surface id, semantic class and grip multiplier are exposed
in wheel telemetry.

A project or Shared policy can configure the mapping without putting
source-format material knowledge into engine.physics:

    {
      "op": "vehicle.surface_policy.set",
      "clear": true,
      "default": { "class": "asphalt" },
      "surfaces": [
        { "surface_id": 55, "class": "ice" },
        { "surface_id": 12, "class": "snow" },
        {
          "surface_id": 7,
          "class": "gravel",
          "dry_grip": 0.78,
          "wet_grip": 0.64
        }
      ]
    }

## Script commands

### vehicle.upsert

Registers a vehicle and, by default, creates a dynamic physics body.

    {
      "op": "vehicle.upsert",
      "id": "car.player",
      "class": "automobile",
      "position": [0.0, 1.2, 0.0],
      "reference_handling": {
        "m_fMass": 1500.0,
        "m_fInitialDragCoeff": 8.0,
        "m_fDriveBiasFront": 0.0,
        "m_nInitialDriveGears": 6,
        "m_fInitialDriveForce": 0.31,
        "m_fDriveInertia": 1.0,
        "m_fInitialDriveMaxFlatVel": 210.0,
        "m_fBrakeForce": 0.9,
        "m_fBrakeBiasFront": 0.58,
        "m_fHandBrakeForce": 0.8,
        "m_fSteeringLock": 35.0,
        "m_fTractionCurveMax": 2.45,
        "m_fTractionCurveMin": 2.15,
        "m_fTractionCurveLateral": 22.5,
        "m_fSuspensionForce": 2.2,
        "m_fSuspensionCompDamp": 1.3,
        "m_fSuspensionReboundDamp": 2.2,
        "m_fSuspensionBiasFront": 0.52,
        "m_fAntiRollBarForce": 0.7
      }
    }

A complete native definition object can be supplied instead of the shorthand
fields. Set create_body=false when another system already owns the physics
body. Set recreate_body=true to rebuild an existing body from the supplied
chassis definition.

### vehicle.input.set

    {
      "op": "vehicle.input.set",
      "id": "car.player",
      "throttle": 1.0,
      "brake": 0.0,
      "steer": -0.25,
      "handbrake": 0.0,
      "pitch": 0.0,
      "roll": 0.0,
      "yaw": 0.0,
      "collective": 0.0
    }

All input axes are sanitized by the runtime. Ground vehicles use
throttle/brake/steer/handbrake. Aircraft and watercraft additionally consume
pitch/roll/yaw/collective as appropriate.

### vehicle.enabled.set

Temporarily enables/disables the vehicle solver without destroying the physics
body.

### vehicle.remove

Removes vehicle simulation. destroy_body defaults to true.

## Runtime telemetry

The script frame state and full runtime diagnostics contain:

    vehicles
      schema
      count
      pending_probes
      vehicles[]
        entity
        class
        speed_mps
        speed_forward_mps
        gear
        engine_speed
        clutch
        input
        wheels[]
          contact
          compression
          suspension_velocity
          normal_force
          longitudinal_slip
          lateral_slip_angle
          angular_velocity
          rotation_angle
          steer_angle
          surface_entity

This state is intended for wheel-bone presentation, dashboards, audio,
particles, debugging, ABS/traction-control layers and AI driving.

## Physics ownership

The chassis is a normal NewViso dynamic body. The vehicle crate never calls a
specific backend directly. Each fixed physics tick follows:

    PhysicsRuntime body snapshots
            |
            v
    VehicleRuntime
      transmission
      wheel suspension
      tyre forces
      class dynamics
            |
            +--> point impulses / angular velocity corrections
            |
            +--> wheel ray probes
                        |
                        v
                  engine.physics
                        |
                        v
                 query hit batch
                        |
                        v
            next fixed vehicle tick

This preserves the replaceable physics-provider boundary and avoids embedding
Jolt/Gravitas implementation details in the vehicle model.

## Presentation script commands

The presentation layer adds:

- vehicle.part.set: open/visible/damage for an imported named part;
- vehicle.lights.set: headlights, indicators, hazard and siren;
- vehicle.audio_fx.configure: engine/tyre/siren loop clips and exhaust/tyre
  particle effects;
- vehicle.occupant.set: attach or clear a scene entity at an imported seat;
- vehicle.surface_policy.set: map opaque collision surface ids to semantic grip
  profiles.

Persistent vehicle audio currently uses the canonical audio voice API, which
provides gain/pitch/loop control but no per-voice 3D position. Exhaust and tyre
particles are spatial and use imported fragment pivots. Occupant attachment is
presentation ownership; higher-level enter/exit gameplay must coordinate an
occupant's independent physics body.

## Current next fidelity boundaries

The core dynamics and the first complete asset/presentation path are present.
The remaining fidelity boundaries are narrower:

- exact vehicle skeleton/non-render-marker recovery beyond renderable fragment
  children;
- physically constrained door/bonnet/boot hinges instead of presentation-only
  articulation;
- soft-body/crumple vertex deformation and damaged drawable variants;
- per-voice spatial vehicle audio once the canonical audio ABI supports voice
  position/velocity;
- higher-level enter/exit state machines, occupant physics ownership and weapon
  behavior on imported weapon_mount parts;
- traffic AI/path following and vehicle avoidance;
- trailer articulation and train rail constraints;
- richer authored plane/heli/boat sub-handling profiles.

These layers can extend the stable newviso.vehicle.runtime.v1 and
northstar.model.fragment.v1 contracts without coupling the engine to a source
game's runtime types.
