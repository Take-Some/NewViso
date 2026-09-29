# Sky dome and cloud motion pipeline

## Source behavior reproduced

The reference implementation was inspected in the local GTA V source tree:

- game/vfx/sky/Sky.cpp
- game/vfx/sky/SkySettings.cpp
- game/vfx/sky/SkySettings.h
- game/vfx/clouds/CloudHat.cpp
- game/vfx/clouds/CloudHat.h

The procedural sky dome and the streamed CloudHat system are separate layers.

### Dome placement

The procedural dome uses a roughly 20 km scale. It follows camera position in the
horizontal plane, but its vertical origin stays at the world horizon/water level.
The projection is forced to infinite depth. NewViso reproduces that contract with
dome_scale, horizon_level, camera height and gl_Position.z = gl_Position.w.

### Procedural cloud motion

The large cloud body does not use time multiplied by one shared velocity.

The reference sky keeps a persistent 2D base-noise phase. Every simulation step it
samples global AIR wind, normalizes the XY direction and advances the phase by:

    direction * large_speed * noise_phase_scale^2 * dt

The source default noise_phase_scale is 0.01 and the default large-cloud speed is 5.

Three additional channels are evaluated from continuous time-cycle position measured
in days (daysBetween + dayRatio), not wrapped seconds-of-day:

- small/filler clouds;
- overall detail;
- edge detail.

Each phase follows the same wrapped form:

    fract(0.5 + speed * cycle_days / 11.0)

The reference defaults for all three detail speeds are 1.

NewViso keeps those four motion channels independent. The base phase is integrated on
CPU from real frame time and the generic cloud wind vector. Small, overall-detail and
edge-detail phases are generated from a continuous cloud-cycle clock so midnight does
not reset cloud texture motion.

### Source textures

FirstFPS now uses the same authored noise roles exposed by the Shared skydome
dictionary:

- textures/skydome.ytd@baseperlinnoise3channel
- textures/skydome.ytd@noise16_p
- textures/skydome.ytd@starfield

The shader preserves the three base-noise channels, builds the broad cloud body from
large + small phases, then uses independently moving overall and edge detail to erode
and sculpt that body.

## Generic NewViso contract

The engine does not contain GTA/RSC7 format checks. The cloud descriptor exposes
generic render parameters:

- speed: global 2D air-wind vector; its direction drives the base phase;
- large_speed;
- small_speed;
- overall_detail_speed;
- edge_detail_speed;
- noise_phase_scale;
- existing coverage/density/shape/detail/weather controls.

Scene3dRuntime owns the accumulated phase state. Project files and weather scripts
only provide parameters.

## Atmospheric cloud geometry / CloudHat-equivalent runtime

The reference CloudHat subsystem is a second cloud layer made from real drawable
geometry. NewViso now implements the same architectural separation as a generic
AtmosphericCloudResources runtime instead of folding the feature into the
procedural dome shader.

Each atmospheric layer owns:

- an authored semantic mesh, or a generated world-horizontal cloud-sheet fallback;
- an independently addressable texture;
- authored position, scale and base rotation;
- accumulated angular rotation driven by angular_velocity_degrees * dt * cloud_hat_speed;
- per-axis camera-follow scaling;
- lower/upper altitude triggers with independent fade ranges;
- three UV animation channels with explicit COMBINE or SCULPT roles;
- wind-speed modulation of UV animation from the resolved `scene.weather.state` wind, with procedural-cloud speed retained only as a fallback;
- weather target weights, weighted CloudList fragment selection and a separate script alpha override;
- transition time, density, softness, opacity and tint.

The renderer owns a dedicated atmospheric-cloud pipeline. It depth-tests against
opaque scene geometry, does not write depth, and alpha blends before ordinary
transparent world surfaces. This keeps buildings and terrain in front of distant
cloud geometry while windows and particles may still composite afterward.

Project scripts can temporarily override a layer with:

    scene.atmospheric_cloud_layer.target.set

and restore weather ownership with:

    scene.atmospheric_cloud_layer.target.clear

### Weather preset and CloudsPS keyframe fidelity

GTA CloudList data is treated as two different contracts rather than one alpha mask:

- `mBits` defines which CloudHat fragments are eligible for the active cloud preset;
- `mProbability` supplies the weighted choice between those eligible fragments.

FirstFPS resolves that weighted selection once per weather-cycle/variant seed and writes
the stable imported fragment id (for example `gtav.12`) into
`scene.weather.state.effects.current_cloud_variant/next_cloud_variant`. The renderer
then crossfades only the selected current/next fragment groups. A selected fragment may
still contain several drawable layers (horizon, body and altitude layers); those layers
remain authored as one CloudHat formation.

The original `cloudkeyframes.xml` curves are no longer import-only data. FirstFPS
samples and weather-blends the active preset curves each frame and submits them through
`scene.cloudhat.keyframe.set`. The CloudHat shader consumes the original CloudsPS
semantic channels:

- CloudColor and CloudLightColor;
- CloudAmbientColor, CloudSkyColor and CloudBounceColor;
- CloudEastColor and CloudWestColor;
- CloudScaleFillColors;
- CloudDensityShift_Scale_ScatteringConst_Scale;
- CloudPiercingLightPower_Strength_NormalStrength_Thickness;
- CloudScaleDiffuseFillAmbient_WrapAmount.

Projects without this keyframe state retain the generic NewViso atmospheric fallback.

### Transition and residency fidelity

The remaining CloudHat state-machine behavior is implemented independently from the
sky dome:

- transition_in_time_percent and transition_out_time_percent scale the requested
  transition duration per layer;
- transition_delay_percent delays layer admission and fade start. As in the
  reference implementation, the same authored incoming-delay percentage is applied
  on both transition-in and transition-out;
- transition_midpoint uses the reference two-half piecewise remap. A midpoint of
  0.5 is linear, while earlier/later midpoints redistribute the first and second
  halves of the visible transition;
- transition_alpha_range supplies the source-style early container alpha ramp.
  The GTA CloudsPS path does not receive a transition-progress input, so its
  source density closure is left untouched and the transition is applied to
  output alpha. NewViso's density-threshold grow/erode behavior is retained only
  for generic non-GTA atmospheric layers;
- every resident layer occupies cost_factor from a generic streaming_budget.
  Incoming layers cannot become resident until both their delay and remaining cost
  allow it. Outgoing layers keep their cost until their fade reaches zero;
- renderer resources follow admission state: vertex/index buffers, texture, uniform
  buffer and bind group are created on page-in and destroyed on page-out.

Waiting for residency advances the request/delay clock but does not consume the
layer's fade clock. This preserves the important source behavior where streaming
latency cannot cause a newly available cloud to pop directly to the end of its fade.

### CloudsPS AnimSoft shader closure

The GTA-backed path follows the original PSCloudsVertScatterPiercing_AnimSoft
instruction semantics rather than a visually similar approximation.

Vertex COLOR0 remains linear RGBA in source order. For the AnimSoft path:

- COLOR.a is the final per-vertex density/alpha multiplier;
- COLOR.g masks the DetailDensity2 contribution;
- COLOR.b biases the sculpt field;
- COLOR.r is not consumed by this pixel-shader path.

Animated density uses the three green density samples and the runtime CloudHat
animation vectors:

    shaped = (1 - density.rgb^2) * gAnimBlendWeights
    normalWeights = shaped * gAnimCombine
    sculpt = dot(shaped, gAnimSculpt) + COLOR.b
    detailMax = max(DetailDensity2.rgb)
    density = COLOR.g * detailMax
              - (1 - COLOR.g * detailMax) * sculpt
    density = saturate((density - DensityShift) * DensityScale)

The three sampled tangent-space normals are combined by normalWeights and normalized.

Lighting preserves the source hemispheric ordering: west/east uses the horizontal
normal factor, BounceColor uses the second factor and SkyColor the third. CloudColor
multiplies only the diffuse/fill/ambient contribution; forward scatter and piercing
are added independently. The phase function uses the camera-to-fragment/sun cosine,
and piercing uses the projected normal term plus the powered view-sun term from the
original VS/PS sequence.

gAnimCombine, gAnimSculpt and gAnimBlendWeights are native float3 fields. NewViso
keeps their XYZ semantics clean; an engine-only animated-variant flag lives only in
the padding W component of the GPU ABI.

CloudScaleFillColors is retained in the sampled CloudHat keyframe state, but it is not
currently applied as an invented global color multiplier because it is a CPU-side
CloudHat control rather than a CloudsPS cbuffer field. Its exact source CPU
preconditioning rule remains a separate fidelity item.

### Soft scene-depth intersection

GTA supplies scene depth and near/far projection constants to the CloudHat shader for
a soft-particle intersection term. NewViso keeps the same rendering purpose but uses
a provider-neutral implementation suitable for the current immediate renderer path.

Before atmospheric geometry is drawn, Scene builds a low-resolution R32F linear-depth
proxy from the current camera using the already compacted opaque/cutout indexed-indirect
submission stream. The proxy stores camera/view-forward depth (the same projective
domain represented by clip.w), not radial Euclidean distance.

CloudHat samples that texture in screen space and applies the source clouds_soft /
clouds_animsoft relationship:

    separation = scene_view_depth - cloud_view_depth
    soft = saturate(separation / gSoftParticleRange)

The imported material's native gSoftParticleRange is authoritative. In the shipped
CloudHat closure NewViso currently observes 49 runtime draw layers at 175 units,
37 layers at 500 units and 28 non-soft variants at zero.

The same depth proxy is shared with the optional volumetric-cloud renderer. That path
converts view depth back to ray distance with:

    scene_ray_t = scene_view_depth / dot(ray, camera_forward)

so changing CloudHat to the source depth domain does not distort volumetric occlusion.

Normal hardware depth testing remains enabled and depth writes remain disabled.
Therefore fully occluded cloud fragments are still rejected by depth, while soft
CloudHat variants fade linearly before the hard intersection seam.

## CloudHat frame-comparison harness

FirstFPS includes a zero-dependency comparator at:

    tools/compare_cloudhat_frames.py

It accepts either one reference/candidate PNG pair or two directories containing
matching PNG frame names. The report records SHA-256 identities, dimensions, crop,
MAE, RMSE, PSNR, maximum 8-bit channel error and the fraction of pixels above a
configurable threshold. An optional diff directory receives false-color PNG error
maps.

For a meaningful GTA/NewViso fidelity measurement, both captures must use the same
camera transform/FOV, viewport, time-of-day, weather preset/blend and selected
CloudHat fragment. The comparator deliberately does not resize or align mismatched
frames because doing so would hide camera/projection errors.

No original GTA reference capture is currently stored in FirstFPS, so a numeric
GTA-vs-NewViso pixel-delta result is not claimed yet.

## True volumetric cloud runtime

NewViso also has a third cloud capability: a camera-ray volumetric renderer. This is
the primary cloud path used by FirstFPS. It does not rasterize a cloud carrier mesh
and does not sample the sky sphere as cloud geometry.

The render graph is:

    opaque/cutout visibility stream
              |
              v
    low-resolution linear scene depth
              |
              v
    world-space cloud raymarch
              |
              v
    temporal reprojection/history clamp
              |
              v
    full-resolution alpha composite
              |
              v
    transparent world geometry / particles

The raymarch intersects the camera ray with a project-authored world-Y altitude slab.
Every sample evaluates a real XYZ-dependent density field. The current renderer API
does not expose Texture3D, so the density implementation builds a 3D field from:

- a low-frequency XZ weather/coverage map;
- an authored height profile between base_altitude and top_altitude;
- three decorrelated detail projections over XZ, XY and ZY;
- vertical detail modulation and edge erosion.

The result is still volumetric: moving the sample point in X, Y or Z changes density,
and the camera integrates participating media through a finite world-space thickness.
There is no plane or dome participating in the primary image.

Per-ray transmittance follows Beer-Lambert extinction. Samples receive direct solar
lighting from a secondary short march toward the atmosphere-driving sun, with a
Henyey-Greenstein forward phase term, height-dependent ambient contribution, powder
term and a cheap multiple-scattering approximation. Opaque scene distance clamps the
ray integration endpoint so buildings and terrain occlude the volume naturally.

The pass is intentionally low resolution and temporally accumulated. Two Rgba16Float
history targets ping-pong between frames. The temporal pass reconstructs a
representative world point at the cloud layer midpoint, projects it through the
previous view-projection matrix, clamps history to the current local neighborhood and
reduces history weight when cloud alpha changes sharply. This is an approximate
cloud-layer reprojection; an exact per-pixel cloud-depth/MRT reprojection remains a
possible future refinement.

Project data controls base/top altitude, maximum distance, resolution scale,
view/light step budgets, coverage/density, shape/detail scales, erosion, extinction,
scattering, ambient light, phase anisotropy, powder strength, temporal blend and
jitter. The engine owns only the generic participating-medium renderer.

FirstFPS currently keeps GTA CloudHat as the authored geometry layer and leaves the
world-space volumetric renderer disabled for this fidelity path. Procedural sky clouds
remain a separate compatible atmospheric layer, matching the architectural separation
between procedural sky clouds and authored CloudHat formations rather than replacing
one system with the other.

