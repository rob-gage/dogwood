+++
title = "Rapier collision world"
description = "How Collision maintains Rapier state, synchronizes actors and voxel colliders, and advances one fixed step."
weight = 40
[extra]
sources = ["engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/collisions/collision_rigid_body_pass.rs", "engine_world/src/scenes/collisions/collision_rigid_body.wgsl", "engine_world/src/scenes/collisions/mod.rs", "engine_world/src/actors/components/actor_kinematics_rotation.rs", "engine_world/src/scenes/mod.rs", "engine_world/src/scenes/scene.rs", "engine/src/lib.rs", "engine_world/src/actors/components/actor_identity.rs", "engine_world/src/actors/components/actor_kinematics_position.rs", "engine_world/src/actors/components/actor_collision.rs", "engine_world/src/actors/components/actor_contact.rs", "engine_world/src/scenes/scene_position.rs"]
+++

This page is for engine contributors. It describes the `Collision` struct, its Rapier configuration, and the order of work in one fixed step. See {{ <rustdoc path="engine/scenes/collisions/struct.Collision.html" text="Collision" /> }}.

`Collision` owns the Rapier 2D pipeline and its world state: rigid bodies and colliders, island and broad/narrow phase managers, joints, the CCD solver, and fixed-step integration parameters. It also owns:

- the actor-to-Rapier handles;
- the static-cell collider and a separate settled-dynamic-cell collider;
- the GPU passes that read tile occupancy and rasterize cellular rigid bodies.

`Scene::simulate` creates `Collision` during simulation when an active region is ready.

## Rapier tuning and origin

The Rapier step uses gravity `(0, -gravity)`, with `gravity` in tiles per second squared. Integration uses four internal PGS iterations, and static contacts use a spring natural frequency of 600 Hz. Pawns use a `KinematicCharacterController` with a `0.01`-tile absolute offset, 50-degree climb and slide limits, and a `0.2`-tile absolute ground snap. Cellular rigid bodies use a soft CCD prediction distance of 1 tile and 8 additional solver iterations; their lifecycle is described with [body lifecycle](@/internals/simulation/rigid-bodies.md).

Rapier positions use a local tile origin to retain sub-cell precision in `f32`. The origin is the active region's center tile; it is moved when the center is over 1,024 tiles away on either axis. On a shift, the system translates existing cellular rigid bodies by the inverse origin change, adjusts their rasterization positions in cells, removes the old tile colliders, and recomputes actor translations from exact `ScenePosition` values.

## Step order

Each call to `simulate_collisions` runs these steps in order:

1. Processes any pending changed rigid-body cells, reads and applies static occupancy when needed (see [refresh triggers](@/internals/simulation/static-cell-colliders.md#refresh-triggers)), then checks newly changed cells for detached groups.
2. If cellular rigid bodies exist, reads settled dynamic-cell occupancy and refreshes that separate collider.
3. Releases rigid bodies that no longer fit in the active region: static cells remain at rest, while covered dynamic cells stay dynamic; the release clears rigid-body markers from covered cells.
4. Applies the latest kinematics impulses to rigid bodies and stores the latest holding contacts.
5. Synchronizes actors with identity and position components (see [actor lifetime](@/internals/actors/actor-lifetime.md) for region exit). Collision checks region membership for each such actor. It creates or updates a fixed, dynamic, or position-based kinematic Rapier body only when the actor has an `ActorCollisionComponent` or `ActorContactComponent`, and replaces the relevant collider when its component changes. Actors that leave the active region lose their Rapier bodies.
6. Moves pawns through the character controller using movement input, gravity, and dynamic-cell contacts, then runs the four-pass holding response for rigid bodies supported by dynamic cells.
7. Steps Rapier and writes changed body positions back to actors’ `ActorKinematicsPositionComponent` values.
8. Queues changed cellular rigid bodies for rasterization into the tile buffers.

A release during this step requests a static occupancy reread; its newly rasterized cells enter the static collider on a later collision call.

`Collision` creates actor Rapier bodies with rotations locked. `ActorKinematicsRotationComponent` is currently a marker with no rotation value. An actor with only `ActorContactComponent` gets a position-based kinematic body, and its contact collider is a sensor. The collision and occupancy phases, including collider delta updates, are detailed on [static cell colliders](@/internals/simulation/static-cell-colliders.md); the kinematics-to-collision contact formats are on [contact exchange](@/internals/simulation/contact-exchange.md).
