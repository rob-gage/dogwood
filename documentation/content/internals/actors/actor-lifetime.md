+++
title = "Actor lifetime in the active region"
description = "How collision simulation removes actors that leave the active region and how persistence affects despawning."
weight = 40
[extra]
sources = ["engine_world/src/scenes/collisions/collision.rs", "engine_world/src/actors/components/actor_persistence.rs", "engine_world/src/scenes/scene.rs", "engine/src/games/game_instance.rs", "engine_world/src/actors/components/actor_identity.rs", "engine_world/src/actors/actor_registry.rs", "engine_world/src/scenes/chunks", "engine_world/src/actors/components/actor_camera.rs", "engine_world/src/scenes/cellular_tiles/cellular_tiles.rs", "editor/src/editor.rs"]
+++

This page is for engine contributors. It explains when an actor is despawned because it left the [active region](@/architecture/glossary.md), and how `ActorPersistenceComponent` and camera actors affect that.

## Region-exit check

During collision simulation, the engine checks each actor that has both identity and position components against the active region. What is tested depends on the actor's colliders:

- With no collision or contact collider, the actor position must remain inside the region.
- With either collider, the bounds of each present collider must fit entirely inside. The position itself is not tested.

Actors without a position component are not part of this pass.

## What happens on exit

Actors with a collision or contact collider that exit are removed from the collision world, which removes their Rapier body. Actors with no collision or contact collider have no active collider to remove. If their collider components were removed since the previous step, collision simulation also removes the previously created Rapier body.

- Without `ActorPersistenceComponent`, the registry then despawns the actor, and its identity hook removes the actor-to-entity mapping.
- With it, the actor is not despawned and stays registered. The component is an empty marker.

If a persistent actor later re-enters the active region, collision simulation adds it back and recreates its Rapier body if it has a collision or contact collider.

`CellularTiles` can still include eligible actors up to four tiles beyond the region for [dynamic-cell interactions](@/internals/simulation/contact-exchange.md).

> **Not implemented:** Actor components are not serialized with chunk data. `ActorPersistenceComponent` keeps an actor registered in memory after region exit; it does not persist components to disk.

## Cameras and region selection

A camera component does not exempt an actor from region-exit handling. A camera actor that is actually outside the active region is despawned unless it carries the persistence marker.

The editor's free camera is spawned with `ActorPersistenceComponent`, so it remains registered when it leaves the active region.

- The game instance streams around the selected camera actor. That actor can differ from the game's default camera.
- Scene streaming sizes the region to `2 * camera.width` by `2 * camera.height` tiles, using the camera position's tile coordinates. `zoom` does not affect this size.

See [actor registry and identity](@/internals/actors/registry.md) for handle behavior after despawn and [world streaming](@/internals/world-streaming/_index.md) for active-region residency.
