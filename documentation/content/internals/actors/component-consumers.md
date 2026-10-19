+++
title = "Who reads each component"
description = "A map of actor components to the engine systems that currently read or write them."
weight = 20
[extra]
sources = ["engine_world/src/actors/components", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/kinematics/kinematics.rs", "engine_world/src/scenes/edits/edit_streaming_state.rs", "engine/src/games/game_instance.rs", "editor/src/editor.rs"]
+++

This page is for engine contributors. The registry only stores components; scene, graphics, collision, and kinematics code give them behavior. The tables show which system reads or writes each component.

For component fields and defaults, see the [actor components API](@/external-api/actor-components.md).

## Components with consumers

| Component | Current consumer and effect |
|---|---|
| `ActorIdentityComponent` | Registry hooks maintain the actor-to-entity map. Scene collision and kinematics queries pair it with position to recover the `Actor` handle. `GameInstance` pairs it with possession to select keyboard-controlled actors. |
| `ActorKinematicsPositionComponent` | Read by scene streaming, graphics, collision, kinematics, and the editor (see below). Collision writes back positions changed by dynamic bodies or pawn movement. |
| `ActorCameraComponent` | `Scene::stream` and `Scene::graphics` read it (see below). The editor copies it when creating its free camera. |
| `ActorSpritesComponent` | `Scene::graphics` converts sprite records to renderer-ready sprites, skipping frames outside the sheet. See [scene rendering](@/internals/rendering/scene-renderer.md). |
| `ActorCollisionComponent` | Collision creates and updates Rapier bodies and colliders and checks region exit. Kinematics packs eligible actors' collider shape, offsets, rotation, and body-dependent mass for dynamic-cell simulation, but the GPU pass includes at most 32 actors per tick. Edit collision queries also use actor colliders. |
| `ActorContactComponent` | Collision creates and updates a Rapier sensor collider and checks its bounds for region exit. Engine code does not query or expose the sensor's overlap pairs. |
| `ActorPawnPossessionComponent` | `GameInstance` selects actors with this marker and writes keyboard-derived `ActorPawnControlComponent` values to them. |
| `ActorPawnControlComponent` | `GameInstance` writes it to possessed actors. Collision reads movement, jump, and crouch intent. |
| `ActorPawnMovementComponent` | Collision reads tuning values for kinematic actors with a physical collider. See [pawn movement](@/internals/actors/pawn-movement.md). |
| `ActorPersistenceComponent` | Collision checks for this marker before despawning an actor that exits the active region. The editor attaches it to its free camera. See [actor lifetime](@/internals/actors/actor-lifetime.md). |

### Position consumers

- **Scene:** streams around the selected camera position.
- **Graphics:** positions the camera and sprites.
- **Collision:** reads actor positions.
- **Kinematics:** reads positions when packing actors for dynamic-cell simulation.
- **Editor:** copies the game camera position when creating its free camera, reads the selected camera position for the viewport, and updates the free camera position from keyboard input.

### Camera consumers

- **Streaming:** `Scene::stream` reads camera width and height and requests a region twice that unzoomed size.
- **Graphics:** `Scene::graphics` uses the visible dimensions, which are width and height multiplied by zoom.
- **Editor:** scales keyboard movement by the free camera's zoom-adjusted visible width and updates its zoom from the mouse wheel.

## Components without consumers

These components are empty markers that no system reads:

- `ActorAttachmentComponent`

> **Not implemented:** Actor-parent attachment is not implemented.
- `ActorKinematicsVelocityComponent`: movement does not read it as velocity.
- `ActorKinematicsRotationComponent`: collision locks all actor body rotations regardless of this component.
- `ActorKinematicsAngularVelocityComponent`
- `ActorSamplerMaterialsComponent`

## Update order

For the host-level fixed-update and rendering sequence, see [frame and update lifecycle](@/architecture/frame-lifecycle.md). Within scene simulation, `Scene::simulate` calls kinematics before collision. Kinematics returns asynchronous contact data; see [cell and body contact exchange](@/internals/simulation/contact-exchange.md) for readback timing and cached values.

For the simulations themselves, see [cellular kinematics](@/internals/simulation/cellular-kinematics.md) and the [Rapier collision world](@/internals/simulation/collision-world.md).
