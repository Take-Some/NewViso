# newviso-navigation

`newviso-navigation` is NewViso's engine-neutral local navigation service for materialized actors.

It consumes generic triangle geometry, filters upward-facing walkable surfaces by slope, builds streamed polygon tiles, joins polygons through shared-edge portals, and resolves local paths independently from the coarse `newviso-world` route graph.

Implemented capabilities:

- streamed navigation tiles;
- polygon adjacency across tile boundaries;
- A* polygon-corridor search;
- funnel/string-pulling waypoint reduction;
- budgeted asynchronous path requests;
- agent-radius portal clearance;
- off-mesh links;
- dynamic navigation obstacles;
- runtime diagnostics/state.

The coarse LivingWorld route graph remains authoritative for background/cross-region travel. This crate refines only the local physical segment for materialized actors.

Current limitation: off-mesh links participate in path connectivity, but traversal-specific actions such as ladder climbing, vault animations, or jump arcs are not executed here.
