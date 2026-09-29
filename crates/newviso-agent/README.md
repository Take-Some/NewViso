# newviso-agent

`newviso-agent` is the engine-neutral autonomous-agent intelligence layer for NewViso.

It does not own persistent world existence, rendering, physics, animation, pedestrian archetypes, factions, combat rules, traffic rules, or project-specific behavior.

## Boundary

```text
newviso-world snapshot
    |
    v
newviso-agent
    perception memory
    blackboard
    AI LOD/timeslicing
    task lanes
    task arbitration
    task lifecycle
    |
    v
generic AgentCommand intentions
    |
    v
newviso-runtime
```

The crate intentionally has no internal NewViso dependencies. The composition root maps typed world state into `AgentWorldSnapshot` and maps `AgentCommand` back into engine capabilities.

## Task lanes

Control priority is:

```text
reaction
primary
movement
ambient
```

Higher lanes preempt lower lanes. When a running travel task is preempted, the agent emits `CancelTravel` and returns the task to pending state so it can resume later.

## Current task types

- `idle`
- `wait`
- `travel_to_node`

## AI LOD

Each agent has independent thinking cadence for the world actor's current simulation tier:

- full;
- reduced;
- background.

Defaults are 50 ms, 250 ms, and 2 seconds respectively. Project data may override them through the agent script command.

See `../../docs/npc-runtime.md` for the complete NPC architecture and remaining production work.
