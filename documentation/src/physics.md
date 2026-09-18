# Physics and collision

Physics is exposed at gameplay level through actor pawn configuration and the scene’s collision-aware simulation. Choose an `ActorCollisionShape` (`Circle`, `Capsule`, or `Rectangle`) and configure one or more movement modes: walking, swimming, flying, or noclip.

Static cellular matter forms terrain; dynamic/granular cellular matter can move and fracture; rigid cellular bodies are placed with `SceneEditBatch::place_rigid_body`; fluids and gases interact with configured actors and materials. Gameplay does not need Rapier, GPU buffers, or solver internals.

There is no public “on collision” callback in the current facade. Poll the actor position/velocity and use scene/world queries or gameplay overlap bookkeeping. For a player-contact rule, compare a pickup’s position with the player’s rendered or fixed position, then despawn the pickup and update game state.

Pause is controlled by `Game::is_paused`; a pawn can opt into `simulate_when_paused`.
