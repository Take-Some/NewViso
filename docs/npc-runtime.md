# NPC and Autonomous Agent Runtime

NewViso separates persistent world existence, autonomous decision making, local navigation, physical locomotion, and presentation. These are independent replacement boundaries rather than one hard-coded pedestrian subsystem.

## Runtime architecture

```text
newviso-world
  world time / persistent actors / simulation LOD
  coarse route graph / background travel / stimuli / relationships
          |
          | typed world snapshot
          v
newviso-agent
  perception memory / blackboard / task lanes / task arbitration
  generic autonomous intentions
          |
          | travel intent
          v
newviso-runtime
          |
          +---------------- background/reduced ----------------+
          |                                                     |
          |                          LivingWorld coarse travel   |
          |                                                     |
          +------------------- full/materialized ----------------+
                                      |
                                      v
                              newviso-navigation
                              streamed walkable polygons
                              async/budgeted path requests
                              polygon corridor + funnel
                                      |
                                      v
                               newviso-steering
                              local neighbor/obstacle avoidance
                                      |
                                      v
                               newviso-character
                              grounding / slopes / steps
                              continuous collision-safe motion
                                      |
                                      v
                         LivingWorld physical-motion authority
                                      |
                                      v
                         Scene presentation / animation / render
```

The same stack can be used for pedestrians, animals, robots, workers, enemies, or other autonomous characters. Concrete game behavior does not live in these engine crates.

## Rockstar-derived architecture observations

The GTA V source tree was used as an architectural reference, not as code to copy.

Relevant subsystems are split across:

```text
game/Peds/PedIntelligence*
game/Peds/PedIntelligence/*
game/task/System/*
game/task/Movement/*
game/task/Default/*
game/event/*
game/pathserver/*
game/ik/*
game/PedGroup/*
game/Peds/Population*
```

The important pattern is the separation of concerns:

```text
persistent ped/entity
        |
        v
intelligence
  perception + event memory + decision policy
        |
        v
priority task trees
        |
        v
navigation/path requests
        |
        v
local steering/movement
        |
        v
animation + IK + physics
```

The path-server reference also shows that pathfinding does not stop at graph search: endpoint adjustment, agent-radius constraints, dynamic-obstacle handling, corridor processing, and string pulling are separate phases. GTA V also applies AI LOD/timeslicing so distant actors do not execute nearby-character work every frame.

## Persistent actor layer

`newviso-world` provides:

- world actors that exist without a player;
- full/reduced/background simulation tiers with hysteresis;
- fixed world time;
- a generic coarse navigation graph;
- background travel;
- scenario points and reservations;
- relationships;
- temporary spatial stimuli;
- population metadata and streaming budgets;
- physical/proxy/abstract representation transitions;
- external physical-motion authority for full-tier character control.

The coarse route graph is intentionally retained. It is the cheap world-scale representation used for background actors, offline simulation, cross-region movement, and large maps.

## Agent intelligence layer

`newviso-agent` provides:

- an agent bound to a persistent world actor;
- per-agent blackboard data;
- spatial stimulus perception;
- bounded perception memory;
- priority task lanes;
- deterministic task arbitration;
- task lifecycle state and travel ownership/preemption;
- full/reduced/background AI thinking cadence;
- generic agent command output.

Task lanes, from lowest to highest control priority:

```text
ambient
movement
primary
reaction
```

Implemented task types:

```text
idle
wait
travel_to_node
```

A higher-priority task can preempt a running travel task. The old physical/coarse travel intent is cancelled rather than continuing invisibly, and the lower-priority task may resume later.

## Local navigation

`newviso-navigation` is the local path-refinement layer for materialized actors.

Implemented capabilities:

- streamed navigation tiles derived from resident generic collision triangles;
- upward-facing walkable-slope filtering;
- polygon connectivity through shared-edge portals;
- cross-tile adjacency;
- A* polygon-corridor search;
- funnel/string-pulling waypoint reduction;
- budgeted asynchronous path requests;
- agent-radius portal clearance;
- dynamic navigation obstacles;
- off-mesh connectivity;
- path invalidation when navigation residency changes.

Navigation residency follows collision residency. When a streamed collision mesh materializes, the runtime derives a navigation tile from the same transformed geometry. When it leaves residency, the navigation tile is removed and affected local paths are invalidated.

The local navmesh does not replace the LivingWorld route graph. A physical actor refines the current coarse route segment against resident local geometry.

## Character locomotion

`newviso-character` owns generic collision-safe character motion.

The current controller provides:

- capsule-style collision using multiple continuous sphere sweeps;
- grounding and ground snap;
- walkable slope limits;
- wall sliding;
- depenetration skin;
- step-up / step-down;
- moving-support linear velocity inheritance;
- collision-safe velocity application;
- persistent physical character state.

The runtime uses the same resident collision BVHs and physics-body geometry used by camera/physics queries. It does not maintain a second unrelated collision world.

A full-tier bound actor remains physically controlled even while idle, so grounding and moving-platform behavior do not disappear merely because the agent has no travel task.

## Local steering

`newviso-steering` refines desired path velocity before character motion.

It currently handles:

- moving-neighbor prediction;
- time-to-collision scoring;
- close-range separation;
- static/dynamic obstacle horizons;
- velocity continuity;
- desired-path deviation.

The steering layer does not choose goals. Agent tasks choose intent, navigation chooses the corridor, steering chooses a safe local velocity, and character motion applies it through collision.

## Logical/physical authority handoff

There must never be two owners of an NPC position.

For reduced/background actors:

```text
LivingWorld route graph
        |
        v
authoritative logical position
```

For a bound full-tier actor:

```text
LivingWorld travel intent
        |
        v
physical navigation + steering + character controller
        |
        v
LivingWorld.set_actor_external_motion(...)
        |
        v
authoritative logical position
```

While physical authority is active, LivingWorld keeps the route/travel intent but does not independently advance the actor along the coarse segment. The actual collision-safe position and velocity are written back every frame and real travelled distance is accumulated.

When the actor leaves full simulation, the physical binding is released. Coarse travel resumes from the accumulated real position without rewinding or consuming stale elapsed time.

## Runtime frame integration

The relevant frame order is:

```text
physics backend step
        ↓
LivingWorld fixed-step simulation
        ↓
agent perception / task arbitration
        ↓
physical character navigation + steering + motion
        ↓
world actor presentation synchronization
        ↓
project scripts
        ↓
scene update / rendering
```

The script frame snapshot exposes `agents` and `physical_characters` runtime state.

## Script API

The engine command surface includes:

```text
agent.upsert
agent.remove
agent.task.set
agent.task.clear
agent.blackboard.set
agent.blackboard.remove

character.world_actor.bind
character.world_actor.unbind

navigation.configure
navigation.obstacle.upsert
navigation.obstacle.remove
navigation.off_mesh_link.upsert
navigation.off_mesh_link.remove
```

Typed Shared APIs are available at:

```text
Shared/Content/scripts/newviso/world/agents.ysc
Shared/Content/scripts/newviso/world/characters.ysc
Shared/Content/scripts/newviso/world/navigation.ysc
```

Example:

```ts
const worker = new Agent("agent.worker.1", "actor.worker.1");
const character = new WorldActorCharacter("actor.worker.1");

return [
  worker.upsert({
    perception: {
      radius: 30.0,
      memorySeconds: 8.0,
      maxMemories: 32,
    },
    thinking: {
      fullIntervalSeconds: 0.05,
      reducedIntervalSeconds: 0.25,
      backgroundIntervalSeconds: 2.0,
    },
  }),

  character.bind({
    controller: {
      radius: 0.32,
      half_height: 0.58,
      max_slope_degrees: 50.0,
      step_height: 0.35,
    },
    arrivalRadius: 0.18,
    steeringTimeHorizon: 1.5,
  }),

  worker.setTask({
    id: "go.to.lobby",
    lane: "movement",
    kind: "travel_to_node",
    destinationNode: "lobby.entry",
    speed: 1.4,
    mode: "walk",
  }),
];
```

The world actor and coarse navigation graph must exist before a travel task starts. Physical navigation becomes active when the actor is in the full simulation tier and relevant collision/navigation tiles are resident.

## Remaining production NPC work

The three critical physical layers now have working foundations. Remaining work is more specialized:

1. **Navigation production hardening**
   - clearance/height-field generation rather than triangle-only walkability;
   - robust obstacle carving instead of coarse polygon/portal blocking;
   - background nav-tile build jobs and cache persistence;
   - hierarchical planning across very large streamed regions;
   - explicit traversal actions for off-mesh links such as doors, ladders, vaults and jumps.

2. **Character locomotion refinement**
   - exact native capsule cast/query when the replaceable physics ABI exposes one;
   - moving-platform angular motion;
   - ledge/drop limits and traversal probes;
   - crouch/stance resizing;
   - deterministic stuck recovery.

3. **Crowd refinement**
   - corridor-boundary constraints;
   - spatial broad phase for dense crowds;
   - reciprocal velocity-obstacle/ORCA-class solving if needed;
   - group/formation steering.

4. **Perception queries**
   - visual field and line of sight;
   - hearing attenuation and occlusion;
   - entity scanning;
   - stimulus filtering/cooldowns;
   - relationship-aware decision inputs.

5. **Animation motion**
   - locomotion state machine/blend spaces;
   - animation-driven turning;
   - root motion;
   - transition synchronization;
   - upper/lower-body task layering.

6. **IK and physical reactions**
   - look-at/head/torso IK;
   - foot placement and slope correction;
   - hand targets;
   - ragdoll transition and recovery.

7. **Population execution**
   - spawn/despawn realization from population channels;
   - navigation-safe spawn positions;
   - model-set selection;
   - creation/removal budgets;
   - persistent identity rules.

8. **Higher-level project behavior**
   - ambient schedules/scenarios;
   - group coordination;
   - combat/flee/cover;
   - vehicle AI;
   - dialogue/social behavior.

## Non-goals

The engine layers must not become a hard-coded GTA pedestrian system. They do not contain fixed factions, police logic, combat rules, traffic laws, weapon choices, or named gameplay archetypes.

Those policies belong to project scripts or reusable project-level behavior modules.
