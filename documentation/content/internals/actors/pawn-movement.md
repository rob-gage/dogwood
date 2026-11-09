+++
title = "Pawn movement"
description = "How pawn input, acceleration, jumping, gravity, and dynamic-cell contacts move kinematic actors."
weight = 30
[extra]
sources = ["engine_world/src/scenes/collisions/collision.rs", "engine_world/src/actors/components/actor_pawn_movement.rs", "engine_world/src/actors/components/actor_pawn_control.rs", "engine/src/games/game_instance.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/cellular_tiles/cellular_tiles.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_pass.rs", "engine_world/src/scenes/mod.rs", "engine_world/src/actors/components/actor_collision.rs", "engine_world/src/actors/components/actor_identity.rs", "engine_world/src/actors/components/actor_kinematics_position.rs"]
+++

This page is for engine contributors. It describes the character controller that collision runs each fixed step for pawns: an actor with `ActorPawnMovementComponent` that moves under keyboard or game-written control.

## Eligibility

Collision applies pawn movement only to actors that have:

- identity and position components,
- an `ActorPawnMovementComponent`, and
- an `ActorCollisionComponent` with body type `Kinematic`.

`Scene::pawn_velocity` and `Scene::pawn_is_grounded` expose the stored state for eligible pawns. Tethers set with `Scene::set_pawn_tether` are stored per pawn in `Collision::pawn_tethers` and consumed by the next movement step.

When an actor has one or more colliders, every present collider's bounds must fit inside the active region; the actor's position is not checked separately. The `ActorPawnControlComponent` is optional; when absent, movement and jump or crouch intent are zero.

## Tuning values

`ActorPawnMovementComponent` defaults:

| Field | Default | Unit |
|---|---|---|
| `speed` | `10.0` | tiles per second |
| `crouching_speed` | `4.0` | tiles per second |
| `acceleration` | `80.0` | tiles per second squared |
| `jump_speed` | `18.0` | tiles per second |
| `maximum_slope_angle` | `50°` | radians |

> **Limitation:** `acceleration` must be nonnegative. A negative value currently makes collision's velocity clamp panic.

## Input

Before each game update, `GameInstance` writes controls to every actor with both `ActorIdentityComponent` and `ActorPawnPossessionComponent`:

- A or left, and D or right, set horizontal movement.
- W or up holds jump.
- S or down holds crouch.

AI or other game code can write the same control component directly.

## Per-step movement

While walking without a tether, each fixed collision step does the following:

1. Clamps horizontal intent to `[-1, 1]` and multiplies it by normal or crouching speed to get the target velocity.
2. Moves horizontal velocity toward the target by at most `acceleration * dt`. Acceleration therefore also governs deceleration and direction changes.
3. Applies vertical velocity, described below.
4. Resolves the movement with the character controller.

Crouching only changes the target speed. It does not change the collider shape or crouch the actor physically.

### Jumping and gravity

- A jump triggers only on a newly pressed jump control while grounded. Collision remembers whether jump was held last step, so holding the button does not retrigger it.
- Grounding comes from Rapier's character movement result, or from a dynamic-cell contact whose upward normal meets the pawn's `maximum_slope_angle` threshold. The same angle limits the slopes the pawn climbs and sets where it starts to slide, and collision clamps it between zero and a right angle each step.
- A standing pawn with nonpositive vertical velocity is held at zero vertical speed. Otherwise gravity is subtracted each step.
- Scene gravity defaults to `50.0` tiles per second squared and can be changed with `Scene::set_gravity`.

### Character controller

The controller is Rapier's `KinematicCharacterController`. For its tuning constants, see [Rapier tuning and origin](@/internals/simulation/collision-world.md#rapier-tuning-and-origin). It queries the actor's collision groups, excludes its own rigid body and sensor colliders, and removes velocity into each contacted surface. It runs once per [movement substep](#movement-substeps), and the summed movement sets the kinematic body's next translation.

## Dynamic-cell contact

`CellularTiles` passes actor-contact data to collision when readbacks are available. See [cell and body contact exchange](@/internals/simulation/contact-exchange.md) for latency and caching.

The pawn first receives the velocity change, then loses any velocity directed into the cellular contact normal. To let dynamic cells overlap a pawn slightly, collision allows one cell of depth and applies half of any excess depth as a correction along the contact normal before calling `move_shape`.

## Edge cases

- Pawns start at zero velocity and ungrounded.
- If gameplay changes an actor's position between steps, collision resets its stored velocity and grounded state.

## Swimming

Collision retains a swimming flag per eligible pawn. It enters when sampled immersion reaches `swimming_enter_immersion` and stays swimming until immersion drops below `swimming_exit_immersion`. Both thresholds are clamped to `[0, 1]`, and exit is clamped no higher than enter. Field units/defaults are on [actor components](@/external-api/actor-components.md#swimming-fields).

The swimming branch applies, in order:

1. Add cellular-contact velocity changes, as with walking.
2. Apply effective gravity: `velocity.y -= gravity * (1 - immersion * fluid_density / pawn_density) * dt`, with density floored at `0.000001`.
3. Express velocity relative to sampled current and multiply by `exp(-material_drag * swimming_drag * immersion * dt)`.
4. Add thrust from horizontal intent and jump/up or crouch/down. Jump takes precedence if both vertical controls are held. Normalize diagonal thrust before applying swimming acceleration.
5. Cap relative speed to `swimming_maximum_speed`, then cap world speed to the larger of that speed and current magnitude.
6. Resolve the resulting movement with the same Rapier character controller and solid collision filters as walking.

Swimming does not require a new jump press or grounding. Crouch supplies downward thrust rather than selecting walking speed. Walking rules resume below the immersion thresholds, with remembered jump-held state still updated. A gameplay position change resets swimming along with velocity and grounding.

The controller reads immersion, current, density, and drag rather than directly applying fluid projection impulses to kinematic pawns. Only idle swimmers set the GPU current-coupling flag; see [pawn/current feedback](@/internals/simulation/fluid-coupling.md#pawns-and-currents). Samples come from earlier GPU submissions, so transitions can lag the visible fluid raster.

## Tethers

A tether is a one-sided length limit along a rope from the pawn through rigid body points. After control and gravity have produced the pawn's velocity, `simulate_collisions_solve_pawn_tether` computes the rope's lengthening rate from the pawn's velocity along `-direction` and each held point's velocity along its gradient. Its stiffness sums the pawn's inverse collision mass and, per distinct dynamic body, the accumulated linear gradient over the body's mass plus the accumulated angular arm over its inertia. Slack (negative stretch) may be taken up within the step; positive overstretch uses a bias of `0.2 * stretch / dt`. A slack tether uses `stretch / dt`. The resulting one-sided impulse changes the pawn's velocity and is applied to each held point with `apply_impulse_at_point`, before `physics_pipeline.step`, so bodies respond in the same tick. The character controller then resolves the pawn's motion as usual. The tether lapses after one step.

While a tether is set and the pawn is neither standing nor swimming, horizontal control only accelerates the pawn toward its walking speed and never brakes it, so swings keep their momentum. Standing pawns walk and jump normally; their walking acceleration is what pulls against the tether.
## Pawn impact speed

`Scene::pawn_impact_speed(actor)` reports the largest speed at which a pawn struck something during
its last movement step, in tiles per second, measured before the hit stopped it. Three hits count:

- **Character-controller hits** on static cells and rigid bodies: the pawn's velocity relative to
  the surface along its normal. A moving rigid body's velocity at the hit point is subtracted, so a
  body striking a still pawn is not the pawn's impact.
- **Ground snaps**: when the controller snaps a falling pawn onto the ground without reporting a
  hit, which is how most landings end, the falling speed the snap stops counts. A standing pawn
  only gains one step of gravity, so standing and walking report about `gravity * dt`.
- **Dynamic-cell contacts**: dynamic cells stop a pawn by their sampled contact rather than in the
  controller, so the speed the pawn carried into the contact normal at the start of the step
  counts. Because contacts arrive a step late, this measures the speed of the first step inside
  the cells.

Tether pulls, fluid forces, the cells' pushes, and control changes never count. Walking into a wall
reports roughly walking speed; a fall reports its landing speed. The value is zero without a hit,
and resets at each movement step and when gameplay teleports the pawn.

### Movement substeps

A pawn's movement step is split into substeps, each no longer than half the narrowest half extent
of its collider, up to `PAWN_MOVEMENT_SUBSTEP_COUNT_MAXIMUM` (8). Each substep moves by the velocity
left after earlier substeps' hits. Without substeps, a pawn moving most of its own size in one step
could pass through the static-cell voxel collider between casts.
