+++
title = "Actors, cameras, and pawns"
description = "Spawn scene actors, select a camera, and opt a pawn into keyboard movement."
weight = 30
[extra]
sources = ["engine_world/src/actors", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/scene_position.rs", "engine_world/src/scenes/collisions/collision.rs", "engine/src/games/game_instance.rs", "engine/src/games/game.rs", "engine/src/lib.rs", "engine_graphics/src/world/scenes/scene_graphics_lighting.rs"]
+++

## Spawn actors with components

An `Actor` is a scene-local handle. `ActorRegistry::spawn` accepts a Bevy component bundle and returns the handle; the registry adds its identity component automatically. Get a component with `component`, mutate one with `component_mutable`, add or replace components with `insert`, and remove one with `remove`. `spawn` can return `ActorError::ActorIdentifiersExhausted`. Registry operations on an existing actor return `ActorError::ActorNotLoaded` if it is not loaded; component lookup, mutation, and removal also return an error if the component is missing.

```rust
use engine::{
    actors::{components::{ActorCameraComponent, ActorKinematicsPositionComponent}, Actor},
    scenes::{Scene, ScenePosition},
};

fn spawn_camera(scene: &mut Scene) -> Result<Actor, Box<dyn std::error::Error>> {
    Ok(scene.actor_registry_mutable().spawn((
        ActorKinematicsPositionComponent {
            position: ScenePosition { tile_x: 0, tile_y: 0, offset_x: 0, offset_y: 0 },
        },
        ActorCameraComponent { width: 80, height: 45, zoom: 1.0 },
    ))?)
}
```

A `ScenePosition` is a cellular tile (`tile_x`, `tile_y`) plus a fixed-point sub-tile offset (`offset_x`, `offset_y`); see [position representation](@/external-api/scenes.md#position-representation).

## Cameras

`ActorCameraComponent` defines a view centered on its actor.

- **Size:** `width` and `height` are cellular tiles at zoom `1.0`; `zoom` scales both.
- **Streaming:** the engine requests a region twice the camera size. Zoom does not change it.
- **Contract:** `Game::scene` returns `Some((&mut Scene, camera_actor))`, or `None` when the game has no scene. The camera actor must have `ActorKinematicsPositionComponent` and `ActorCameraComponent`.
- **Override:** a host can replace the actor with `GameInstance::set_camera_actor`, with the same component requirement.

## Add a keyboard-controlled pawn

A pawn is an actor with `ActorPawnMovementComponent`; a possessed pawn also has `ActorPawnPossessionComponent` and receives keyboard control. Pawn movement requires `ActorKinematicsPositionComponent`, `ActorPawnMovementComponent`, and a kinematic `ActorCollisionComponent` with a physical collider. See [actor components](@/external-api/actor-components.md) for component fields and [actor collision](@/examples/collision-setup.md) for collision setup.

- **Controls:** Add `ActorPawnPossessionComponent` to opt into keyboard control. For the scene returned by `Game::scene`, `GameInstance` writes A/left and D/right movement, W/up jump, and S/down crouch controls to each possessed actor before `Game::update`; game logic can override them for that update.
- **Jumping:** A new W/up press jumps while grounded; holding the key does not repeat jumps.
- **Default tuning:** Speed is `10.0` and crouching speed `4.0` tiles per second, acceleration is `80.0` tiles per second squared, and jump speed is `18.0` tiles per second.

See [pawn movement](@/internals/actors/pawn-movement.md) for movement mechanics and tuning fields.

## Actor lifetime

Actors outside the active region are despawned unless marked persistent; actor components are not saved with chunk data. See [actor lifetime](@/internals/actors/actor-lifetime.md) for the rules.

Add `ActorLightComponent` alongside an actor position to emit colored light from a disc. Its default is white, intensity `1.0`, radius `0.5` tiles; see [lighting](@/external-api/lighting.md) for details.

{{ <rustdoc path="engine/actors/struct.ActorRegistry.html" text="ActorRegistry" /> }} · {{ <rustdoc path="engine/actors/components/struct.ActorCameraComponent.html" text="ActorCameraComponent" /> }} · {{ <rustdoc path="engine/games/trait.Game.html" text="Game" /> }}
