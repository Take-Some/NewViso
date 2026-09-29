# newviso-character

`newviso-character` owns generic collision-safe character motion. It has no NPC, player, faction, animation, or game-policy semantics.

The controller uses a capsule approximation built from continuous sphere sweeps and provides:

- grounding and ground snap;
- walkable slope limits;
- wall sliding;
- depenetration skin;
- step-up / step-down;
- moving-support velocity inheritance;
- collision-safe velocity application;
- persistent character state.

Collision is supplied through the `CharacterCollisionWorld` trait so the runtime can query the same resident geometry used by the physics/camera systems.
