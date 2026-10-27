+++
title = "Who reads each component"
description = "A map of actor components to the engine systems that currently read or write them."
weight = 20
[extra]
sources = ["engine_world/src/scenes/pressure", "engine_world/src/scenes/fluids", "engine_world/src/actors/components", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/cellular_tiles/cellular_tiles.rs", "engine_world/src/scenes/edits/edit_streaming_state.rs", "engine/src/games/game_instance.rs", "editor/src/editor.rs", "engine_world/src/actors/components/actor_light.rs", "engine_graphics/src/renders/scene_renderer_lighting_pass.rs", "engine_graphics/src/renders/scene_renderer_lighting_clear.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cache.wgsl"]
+++

This page is for engine contributors. The registry only stores components; scene, graphics, collision, and kinematics code give them behavior. The tables show which system reads or writes each component.

For component fields and defaults, see the [actor components API](@/external-api/actor-components.md).

## Components with consumers

| Component | Current consumer and effect |
|---|---|
| `ActorIdentityComponent` | Registry hooks maintain the actor-to-entity map. Scene collision and cellular tile simulation queries pair it with position to recover the `Actor` handle. `GameInstance` pairs it with possession to select keyboard-controlled actors. |
| `ActorKinematicsPositionComponent` | Read by scene streaming, graphics, collision, kinematics, and the editor (see below). Collision writes back positions changed by dynamic bodies or pawn movement. |
| `ActorCameraComponent` | `Scene::stream` and `Scene::graphics` read it (see below). The editor copies it when creating its free camera. |
| `ActorParallaxComponent` | `Scene::graphics` reads it optionally alongside sprite and position components, projecting sprites into background records instead of ordinary gameplay records. Its depth is physical distance in tiles. Bevy required-component insertion adds `ActorPersistenceComponent`, preventing region-exit despawn of background anchors. |
| `ActorSpritesComponent` | `Scene::graphics` converts sprite records to renderer-ready sprites, skipping frames outside the sheet. See [scene rendering](@/internals/rendering/scene-renderer.md). |
| `ActorLightComponent` | `Scene::graphics` collects positioned lights into scene graphics; the lighting pass rasterizes them as disc emission. |
| `ActorCollisionComponent` | Collision creates and updates Rapier bodies and colliders and checks region exit. `CellularTiles` packs eligible actors' collider shape, offsets, rotation, and body-dependent mass for dynamic-cell simulation, shared fluid actor projection/sampling, pressure-volume measurement, and pressure flow drag; all use at most 32 actors per tick. Pressure contacts also push actors using prior published pressure. Edit collision queries also use actor colliders. |
| `ActorContactComponent` | Collision creates and updates a Rapier sensor collider and checks its bounds for region exit. Engine code does not query or expose the sensor's overlap pairs. |
| `ActorPawnPossessionComponent` | `GameInstance` selects actors with this marker and writes keyboard-derived `ActorPawnControlComponent` values to them. |
| `ActorPawnControlComponent` | `GameInstance` writes it to possessed actors. Collision reads movement, jump, and crouch intent. |
| `ActorPawnMovementComponent` | Collision reads tuning values for kinematic actors with a physical collider. See [pawn movement](@/internals/actors/pawn-movement.md). |
| `ActorPersistenceComponent` | Collision checks for this marker before despawning an actor that exits the active region. The editor attaches it to its free camera. See [actor lifetime](@/internals/actors/actor-lifetime.md). |

### Position consumers

- **Scene:** streams around the selected camera position.
- **Graphics:** positions the camera and sprites.
- **Collision:** reads actor positions.
- **`CellularTiles`:** reads positions when packing actors for dynamic-cell simulation.
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
- `ActorSamplerTemperaturesComponent`
- `ActorSamplerPressuresComponent`

## Update order

For the host-level fixed-update and rendering sequence, see [frame and update lifecycle](@/architecture/frame-lifecycle.md). Within scene simulation, `Scene::simulate` runs cellular tiles, collision, fluid particles, then pressure; pressure consumers read the previous tick's publication. `CellularTiles` returns asynchronous contact data; see [cell and body contact exchange](@/internals/simulation/contact-exchange.md) for readback timing and cached values.

For the simulations themselves, see [cellular kinematics](@/internals/simulation/cellular-kinematics.md) and the [Rapier collision world](@/internals/simulation/collision-world.md).

## Fluid consumers

Cellular kinematics uploads collider records for fluid projection and sampling; scene control intent also selects which idle swimmers set the shared current-coupling flag. Collision consumes completed fluid actor samples and the swimming fields of `ActorPawnMovementComponent`; see [swimming](@/internals/actors/pawn-movement.md#swimming).

## Pressure and overlay consumers

Pressure simulation binds cellular tile actor records directly. Its volume pass measures actor-covered cells and flow drag. `ActorOverlayComponent` becomes cell-aligned graphics data; hosts append cursor overlays with `GameInstance::set_overlays`. See [pressure simulation](@/internals/simulation/pressure.md) and [scene rendering](@/internals/rendering/scene-renderer.md).
