+++
title = "Actor components"
description = "Fields, units, and current engine use of the public actor component types."
weight = 60
[extra]
sources = ["engine_world/src/scenes/pressure", "engine_world/src/scenes/fluids", "engine_world/src/actors/components", "engine_world/src/actors/actor_registry.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/cellular_tiles/cellular_tiles.rs", "engine/src/games/game_instance.rs", "engine/src/lib.rs"]
+++

Components are Bevy data attached to an `Actor` through `ActorRegistry`. This page is the reference for every public component, for game programmers. All are re-exported from `engine::actors::components`.

See [actors](@/external-api/actors.md) for registry operations and [collisions and edits](@/external-api/collisions-and-edits.md) for collider shapes and group masks.

## Common combinations

| Goal | Components |
|---|---|
| Camera | `ActorCameraComponent` + `ActorKinematicsPositionComponent` |
| Parallax layer | `ActorKinematicsPositionComponent` + `ActorSpritesComponent` + `ActorParallaxComponent` |
| Visible sprite | `ActorKinematicsPositionComponent` + `ActorSpritesComponent` |
| Controllable pawn | `ActorKinematicsPositionComponent` + `ActorCollisionComponent` (kinematic body) + `ActorPawnMovementComponent` + `ActorPawnPossessionComponent` (`ActorPawnControlComponent` is optional; the game instance supplies it for possessed pawns) |

See [parallax backgrounds](@/external-api/scenes.md#parallax-backgrounds) for physical depth, sprite scale, and gameplay changes.

## Component reference

| Component | Fields | Defaults | Current engine use |
|---|---|---|---|
| `ActorIdentityComponent` | Private `Actor`; `new(actor)`, `actor()` | None | Registry identity, inserted by `spawn`. Do not insert or remove it manually. |
| `ActorParallaxComponent` | `depth: f32` in tiles, `scroll_factor: [f32; 2]`, `scroll_origin: ScenePosition`, `offset: [f32; 2]`, `repeat: [ParallaxRepeat; 2]` | Depth `0.0`, factors `[1.0, 1.0]`, origin/offset zero, no repetition | Routes existing actor sprites into background rendering and requires `ActorPersistenceComponent`; invalid depth skips sprites. |
| `ActorCameraComponent` | `width`, `height: u32` in cellular tiles at zoom 1; `zoom: f32` | None | Scene camera size; `visible_width()` and `visible_height()` multiply `width` and `height` by `zoom`. |
| `ActorKinematicsPositionComponent` | `position: ScenePosition` | Zero position | Position in tiles with fixed-point offsets, used for camera, rendering, collision, and kinematics. See [coordinate spaces](@/architecture/glossary.md). |
| `ActorCollisionComponent` | `body: CollisionBody`; `collider: Collider`; `memberships`, `filter: u32`; `mass: f32` | None | Read by collision and dynamic-cell kinematics; pressure simulation also reads its shape from the cellular tile actor records to close cells and drag gas. `mass` uses dynamic cellular material mass units; it decides how far dynamic cells push the actor and how much they slow it. |
| `ActorContactComponent` | `collider: Collider`; `memberships`, `filter: u32` | None | Sensor collider configuration; see [collision setup](@/examples/collision-setup.md). |
| `ActorPawnControlComponent` | `movement: f32`; `is_jumping`, `is_crouching: bool` | `0.0`, `false`, `false` | Control intent for pawn movement. `movement` runs from -1 (left) to 1 (right). |
| `ActorPawnMovementComponent` | `speed`, `crouching_speed`, `acceleration`, `jump_speed: f32`; swimming fields below | `10.0`, `4.0`, `80.0`, `18.0` | Pawn tuning in tiles per second (speeds) and tiles per second squared (`acceleration`). Acceleration must be nonnegative. The engine does not validate it; a negative value causes the pawn update to panic. |
| `ActorPawnPossessionComponent` | Empty marker | Empty | Marks the pawn that receives keyboard intent. |
| `ActorPersistenceComponent` | Empty marker | Empty | Keeps the actor from being despawned on region exit. |
| `ActorSpritesComponent` | `sprites`: vector of sprite tuples | Empty vector | Read by scene graphics; see the tuple key below. |
| `ActorOverlayComponent` | `shape: ActorOverlayShape`; `color: u32` packed RGBA8 | None | Draws a cell-aligned circle or rectangle at the actor position. |
| `ActorAttachmentComponent` | Empty marker | `Default::default()` | No runtime consumer. |
| `ActorKinematicsVelocityComponent` | Empty marker | `Default::default()` | No runtime consumer. |
| `ActorKinematicsRotationComponent` | Empty marker | `Default::default()` | No runtime consumer. |
| `ActorKinematicsAngularVelocityComponent` | Empty marker | `Default::default()` | No runtime consumer. |
| `ActorSamplerMaterialsComponent` | Empty marker | `Default::default()` | No runtime consumer. |
| `ActorSamplerTemperaturesComponent` | Empty marker | `Default::default()` | No runtime consumer. |
| `ActorSamplerPressuresComponent` | Empty marker | `Default::default()` | No runtime consumer. |

### Control and possession

- While walking, `is_jumping` triggers one jump per press while the pawn is grounded. While swimming, jump and crouch control upward and downward thrust.
- `is_crouching` selects `crouching_speed` instead of `speed`.
- Before each `Game::update`, the game instance writes the same keyboard intent to every actor with `ActorPawnPossessionComponent`; game code can override it during the update.
- See [pawn movement mechanics](@/internals/actors/pawn-movement.md) for the full movement rules.

### Persistence

A persistent actor is not despawned by region-exit handling. Its collision body is removed while it is outside the active region.

### Sprite tuple

Each `sprites` entry is `(Arc<SpriteSheet>, u32, f32, f32, f32, f32, f32, [f32; 4], i32)`:

| Position | Meaning | Type and units |
|---:|---|---|
| 1 | Sprite sheet | `Arc<SpriteSheet>` |
| 2 | Frame | `u32` frame index |
| 3–4 | X/Y offset | `f32`, tiles |
| 5–6 | X/Y scale | `f32`, dimension multipliers |
| 7 | Rotation | `f32`, radians |
| 8 | Color | `[f32; 4]`, linear RGBA |
| 9 | Depth | `i32` sort key |

> **Limitation:** `ActorContactComponent` creates a sensor collider, but the public API does not expose its overlap results.

## Requirements

- The camera actor needs a position for streaming and drawing. Streaming requests a region twice the camera's unzoomed width and height; zoom changes only the displayed view.
- Sprites are read only from positioned actors. Ordinary sprite negative integer depths draw behind cellular content, nonnegative integer depths above it. Actors with `ActorParallaxComponent` route sprites into background rendering using physical depth; integer depth orders coplanar sprites. Out-of-range frames are skipped; see [graphics](@/external-api/graphics.md).
- Pawn movement needs a positioned actor with a kinematic collision body and a physical collider.
- Kinematic and dynamic actors in cellular kinematics need finite, positive `mass`, or simulation returns `SceneError::InvalidCollider`. Fixed bodies use an engine-assigned kinematics mass.

{{ <rustdoc path="engine/actors/components/struct.ActorCameraComponent.html" text="ActorCameraComponent" /> }} ·
{{ <rustdoc path="engine/actors/components/struct.ActorCollisionComponent.html" text="ActorCollisionComponent" /> }} ·
{{ <rustdoc path="engine/actors/components/struct.ActorSpritesComponent.html" text="ActorSpritesComponent" /> }} ·
{{ <rustdoc path="engine/actors/components/struct.ActorOverlayComponent.html" text="ActorOverlayComponent" /> }}

## Swimming fields

`ActorPawnMovementComponent` also defines `swimming_maximum_speed` (default `3.5` tiles/s), `swimming_acceleration` (`30.0` tiles/s²), `swimming_density` (`0.57` mass/cell area), `swimming_drag` (`4.0`), `swimming_enter_immersion` (`0.55`), and `swimming_exit_immersion` (`0.35`). See [pawn movement](@/internals/actors/pawn-movement.md#swimming).
