+++
title = "Pawn movement"
description = "How pawn input, acceleration, jumping, gravity, and dynamic-cell contacts move kinematic actors."
weight = 30
[extra]
sources = ["engine_world/src/scenes/collisions/collision.rs", "engine_world/src/actors/components/actor_pawn_movement.rs", "engine_world/src/actors/components/actor_pawn_control.rs", "engine/src/games/game_instance.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/kinematics/kinematics.rs", "engine_world/src/scenes/kinematics/kinematics_pass.rs", "engine_world/src/scenes/mod.rs", "engine_world/src/actors/components/actor_collision.rs", "engine_world/src/actors/components/actor_identity.rs", "engine_world/src/actors/components/actor_kinematics_position.rs"]
+++

This page is for engine contributors. It describes the character controller that collision runs each fixed step for pawns: an actor with `ActorPawnMovementComponent` that moves under keyboard or game-written control.

## Eligibility

Collision applies pawn movement only to actors that have:

- identity and position components,
- an `ActorPawnMovementComponent`, and
- an `ActorCollisionComponent` with body type `Kinematic`.

When an actor has one or more colliders, every present collider's bounds must fit inside the active region; the actor's position is not checked separately. The `ActorPawnControlComponent` is optional; when absent, movement and jump or crouch intent are zero.

## Tuning values

`ActorPawnMovementComponent` defaults:

| Field | Default | Unit |
|---|---|---|
| `speed` | `10.0` | tiles per second |
| `crouching_speed` | `4.0` | tiles per second |
| `acceleration` | `80.0` | tiles per second squared |
| `jump_speed` | `18.0` | tiles per second |

> **Limitation:** `acceleration` must be nonnegative. A negative value currently makes collision's velocity clamp panic.

## Input

Before each game update, `GameInstance` writes controls to every actor with both `ActorIdentityComponent` and `ActorPawnPossessionComponent`:

- A or left, and D or right, set horizontal movement.
- W or up holds jump.
- S or down holds crouch.

AI or other game code can write the same control component directly.

## Per-step movement

While walking, each fixed collision step does the following:

1. Clamps horizontal intent to `[-1, 1]` and multiplies it by normal or crouching speed to get the target velocity.
2. Moves horizontal velocity toward the target by at most `acceleration * dt`. Acceleration therefore also governs deceleration and direction changes.
3. Applies vertical velocity, described below.
4. Resolves the movement with the character controller.

Crouching only changes the target speed. It does not change the collider shape or crouch the actor physically.

### Jumping and gravity

- A jump triggers only on a newly pressed jump control while grounded. Collision remembers whether jump was held last step, so holding the button does not retrigger it.
- Grounding comes from Rapier's character movement result, or from a dynamic-cell contact whose upward normal meets the maximum-slope threshold.
- A standing pawn with nonpositive vertical velocity is held at zero vertical speed. Otherwise gravity is subtracted each step.
- Scene gravity defaults to `50.0` tiles per second squared and can be changed with `Scene::set_gravity`.

### Character controller

The controller is Rapier's `KinematicCharacterController`. For its tuning constants, see [Rapier tuning and origin](@/internals/simulation/collision-world.md#rapier-tuning-and-origin). It queries the actor's collision groups, excludes its own rigid body and sensor colliders, and removes velocity into each contacted surface. The resolved movement sets the kinematic body's next translation.

## Dynamic-cell contact

Kinematics passes actor-contact data to collision when readbacks are available. See [cell and body contact exchange](@/internals/simulation/contact-exchange.md) for latency and caching.

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
