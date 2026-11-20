# NewViso engine UI and main-menu contract

## Separation of responsibilities

The game/runtime owns `MainMenuState` and transitions. It never chooses a UI
implementation, a renderer implementation, or a game-specific visual theme.

```text
Project UI resource + engine lifecycle
               |
         newviso-runtime
               |
   newviso-ui-client (engine.ui)
               |
     capability ui.backend
               |
    any compatible UI provider
               |
       provider draw list
               |
     newviso-render-client
               |
    selected rendering backend
```

The UI backend is a **required project capability** when a project declares a
front-end. It is resolved by the provider registry; a project does not name an
Aurelia, Egui, or other concrete backend unless it deliberately wants to pin
one in its capability configuration. `ui.backend` without a `provider`
constraint is preferred for portable projects.

## Authoring a project menu

Add a project UI file path to the ordinary manifest:

```json
{
  "files": {"ui": "ui/main_menu.json"},
  "capabilities": {
    "required": [{"id": "ui.backend", "min_version": 1}]
  }
}
```

The UI asset uses the existing generic `UiSurfaceNode` JSON semantic schema.
A front-end is opt-in with `metrics.startup_menu: true` and a stable
`surface_id`. All component types, colors, typography, animation/transition
hints, clipping, and asset references are project-authored data. A compatible
UI provider is free to implement them using its own layout, text shaping and
GPU strategy; the engine does not import provider-specific widget types.

Built-in command identifiers understood by the engine:

| Action id | Meaning |
| --- | --- |
| `newviso.menu.start` | Request first game launch |
| `newviso.menu.resume` | Return to already started game |
| `newviso.menu.quit` | Request clean application exit |

During initial startup, the engine removes Resume from the published surface.
After the game has started, it removes Start instead. This is an engine
lifecycle decision, independent of the UI provider.

## Project-authored loading screen

A frontend can opt in to a second semantic surface without naming a provider:

```json
"metrics": {
  "startup_menu": true,
  "loading_surface": "ui/loading_screen.json"
}
```

When Play is requested, the runtime loads this JSON surface via the project's
VFS once and publishes it to `engine.ui`. It owns **no game-specific typography,
layout, branding or effect stack**. The authored surface supplies those; the
runtime updates exact data-binding IDs in its `components` array:

- `loading.progress.track`, `loading.progress.fill` and `loading.progress.percent`
- `loading.status`
- For the four asset groups `models`, `dependencies`, `gpu-textures`,
  `gpu-materials`: `loading.categories.<id>.track`, `.fill`, `.count`, `.label`

Widths are derived from each authored track rather than hard-coded pixel
assumptions. An unknown total remains indeterminate (`WAIT`) and is never
represented as fabricated progress. When streaming finishes, the same authored
surface ID is republished as `visible: false`. Without the optional resource,
the runtime uses its neutral built-in loading fallback. The display backend
remains a capability; no Egui or Aurelia reference occurs in orchestration.

## Runtime lifecycle

```text
Window + renderer initialized
  -> publish project front-end (System UI domain)
  -> wait for user action (world simulation and script on_start deferred)
  -> Play -> engine.ui System-domain runtime loading screen (same game HWND)
  -> real model / dependency / GPU texture / material progress updated each tick
  -> map residency ready -> script on_start exactly once
  -> gameplay + HUD (GameViewport UI domain)
  -> Escape -> front-end / pause menu (System UI domain)
  -> Resume -> gameplay
  -> Quit -> runtime shutdown
```

The editor uses `Editor` UI domain. Gameplay UI does **not** share
the editor chrome or get rendered in the Editor domain.
The existing `engine.ui` service methods `ui.surface_node_v1`,
`ui.dispatch_input_v1`, and `draw_frame_v1` are the only UI-provider
calls made by this menu orchestration.

**No backend is privileged.** Backend substitutions must pass the same
surface, input dispatch and draw-list contract.

## Typography and visual assets

A project can use the engine's `.yfd` font dictionary format for resident
font faces and can reference those faces through ordinary font tokens.
These assets belong to the project or Shared Assets, not a UI plugin source
directory. Unsupported fonts or visual effects must be reported as an explicit
capability gap; silently approximating an unavailable source effect is not
equivalent to successfully importing the original resource.

For `KaylaNightCity`, the project-local dictionary is at
`assets/fonts/editor.yfd` (8 validated imported source faces). The original
fonts and source UI assets can be inspected and converted independently of
which provider is later selected. A verified draw-frame test is still required
before claiming the rendered typography or source game UI effects are identical.
