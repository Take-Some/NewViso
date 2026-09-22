# NewViso Architecture

NewViso is a small composition host around replaceable runtime providers. The source tree is split by replacement boundary, not by feature count.

## Dependency direction

~~~text
apps/newviso
    |
    v
newviso-runtime ---------------------> newviso-config
    |                                  newviso-core
    |
    +----> newviso-platform ----------> newviso-host -------> newviso-compat-abi
    |
    +----> newviso-provider-runtime --> newviso-host
    |                  |
    |                  +--------------> newviso-compat-abi
    |
    +----> newviso-scene -------------> newviso-render-client ---> newviso-host
                       |
                       +---------------> newviso-host
~~~

Dependencies flow downward only. scripts/verify_boundaries.py enforces the allowed direct NewViso dependencies.

## Crate responsibilities

- newviso-compat-abi: binary compatibility types and symbols for deployed external provider DLLs. No engine policy and no provider loading.
- newviso-config: bootstrap defaults, adjacent config.json, environment overrides, CLI overrides, and provider-role selection.
- newviso-core: engine-neutral runtime state primitives.
- newviso-host: service registry, aliases, event bus, host callbacks, and host-owned platform snapshot service.
- newviso-provider-runtime: provider discovery, ABI probing, DLL ownership, and provider lifecycle.
- newviso-platform: platform runtime bridge and event loop. It knows only the generic PlatformApplication callback interface; it does not know about Vulkan, scenes, ECS, or rendering.
- newviso-render-client: typed client facade over the provider-neutral engine.render service protocol.
- newviso-scene: scene/ECS extraction, camera/orbit control, scene math, and scene-to-render preparation. It does not know the raw render service wire format.
- newviso-runtime: composition root. Chooses providers by configured role and wires platform, renderer, scene, host, and lifecycle together.
- apps/newviso: executable entry only.

## Provider replacement

Provider role IDs come from configuration and may be overridden without rebuilding NewViso.

~~~cmd
NewViso.exe --set providers.renderer=engine.render.other
NewViso.exe --platform-provider engine.platform.other
~~~

A replacement provider must satisfy the same runtime service/ABI contract expected for that role.

## Rules

1. No NewViso crate may depend on source crates from older engine trees.
2. ABI compatibility is isolated in newviso-compat-abi.
3. The platform layer never owns a renderer or scene implementation.
4. Scene code never speaks the raw renderer JSON wire protocol.
5. The executable is not a composition root; newviso-runtime is.
6. Backend/provider selection is configuration, not source code.
7. New internal dependencies require an explicit architecture-rule update and must preserve acyclic dependency direction.
