# Actors

`ActorRegistry` is a small ECS-backed ownership boundary. Bevy entities hold
components, while a stable monotonic `Actor(u64)` identifier is the public
handle. The registry maps identifiers to entities and owns spawn/despawn,
positions, velocities, pawn configuration, physical configuration, possession,
and proxy extraction.

There are three meaningful actor forms. An ordinary actor has position. An
`ActorPawn` adds control and movement capabilities. A physical actor has
`ActorPhysicalConfiguration` and is integrated as a dynamic CPU physics body.
Pawns and physical actors use circle, capsule, or rectangle shapes. Noclip
pawns skip physical proxies; walking and swimming pawns publish both CPU
physics and cellular proxy state.

The fixed tick first syncs existing proxies and steps the CPU physics world,
then simulates pawn movement, syncs pawn proxies again, and records logical
actor contact changes. Contact events are deduplicated actor pairs with
`Started`/`Ended` state and are drained from `Scene`; they are not persistent
collision history.

Walking resolves gravity-relative up, slope limits, acceleration, grounded
state, and cellular drive. Swimming samples the derived fluid field and applies
buoyancy/drag-style behavior. Flying and noclip use their configured movement
rules. Possession only selects which possessable pawn receives the translated
control state and which position drives camera/streaming follow.

### Relevant Implementation

- `engine_physics/src/actors/` — public actor identifiers, contacts, pawns, and
  configurations.
- `engine_physics/src/actors_utility/actor_registry.rs` — ECS ownership,
  stable-ID mapping, spawning, proxy extraction, and state application.
- `engine_physics/src/actors_utility/actor_collision_shape.rs` — shared shape
  validation and conversions.
- `engine_physics/src/simulation_actors/` — pawn movement and swimming stages.
- `engine_physics/src/simulation_rigid_bodies/scene_physics_world.rs` — CPU
  physics body ownership.
- `engine_physics/src/simulation_rigid_bodies/` —
  `scene_physics_world_actor_contacts.rs` extracts actor-pair contact events.
- `engine_physics/src/scenes/scene_actor_contacts.rs` — scene event drain API.
