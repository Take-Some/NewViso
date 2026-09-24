# Lifeinvader vertical slice

This package is the FirstFPS authored-world vertical slice for a compact office + exterior test.

## Current state

The checked-in source is a **clean-room proxy** used to validate NewViso's map pipeline. It is not a reconstruction or extraction of Rockstar-owned geometry.

```text
source/lifeinvader_proxy.placements.json
        ↓ cellize_placements.py
generated/index.json
generated/cells/*.json
        ↓ temporary projection
../../scenes/lifeinvader_slice.scene.json
```

Current proxy:

- 110 placements
- 19 independently addressable 16 m cells
- exterior parking/entry
- lobby/reception
- open office
- side rooms
- conference room
- server room
- lounge/kitchen
- static collision and LOD metadata

## Replacement contract

A lawful export can replace the proxy source without changing FirstFPS gameplay code.

Preferred final path:

```text
maps/lifeinvader.ymap@map
  → @cell/x/z
      → definitions/lifeinvader.ytyp@entry
          → models/*.ydd
          → materials/*.ymt
          → textures/*.ytd
```

YMAP owns topology and placements only. YTYP owns reusable definitions. Geometry/material/texture payloads stay external.

The temporary `newviso.scene.v1` projection exists only until NewViso's runtime consumes `engine.assets.maps` directly.
