# newviso-steering

`newviso-steering` is the engine-neutral local-avoidance layer used by materialized autonomous characters.

It samples candidate velocities around the desired path velocity and scores them using:

- predicted time-to-collision with moving neighbors;
- static/dynamic obstacle collision horizon;
- close-range directional separation;
- deviation from desired velocity;
- velocity continuity.

The module does not choose goals or paths. `newviso-agent` chooses intent, `newviso-navigation` supplies the path corridor, and `newviso-character` applies the selected velocity through collision-safe motion.
