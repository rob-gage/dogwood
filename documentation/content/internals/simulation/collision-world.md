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

1. Reads collision-owned changed-cell data copied during the preceding tick and applies changed tile masks and rigid-body cell removals, including thermal transitions. Detached group material reads are verified asynchronously; groups with changed occupancy are discarded before resulting bodies are created on a later tick.
2. Refreshes static occupancy when region, ring, or edit state requires it. Edit changed-cell reads can block. Applies changed occupancy to the static voxel collider and checks both added and removed occupancy cells for floating groups; tiles entering the region are not checked.
3. Consumes the newest completed settled dynamic-cell occupancy snapshot, falling back to a blocking read only when no matching region/ring snapshot exists; requests another snapshot.
4. Releases bodies outside the active region and applies cellular kinematics impulses and fluid moments from prior GPU contact feedback.
5. Synchronizes actors and collider components, then moves pawns through the character controller using movement, gravity, and dynamic-cell contacts.
6. Steps Rapier and writes body positions back to actors.
7. Rasterizes changed cellular rigid bodies into tile buffers and requests detached-cell copies for later processing.

Changed-cell and contact results are one tick late. The [scene pipeline](@/architecture/scene-pipeline.md#simulation-order) gives the complete scene order; [GPU synchronization](@/architecture/concurrency-and-gpu-sync.md) describes waits.
A release during this step requests a static occupancy reread; its newly rasterized cells enter the static collider on a later collision call.

`Collision` creates actor Rapier bodies with rotations locked. `ActorKinematicsRotationComponent` is currently a marker with no rotation value. An actor with only `ActorContactComponent` gets a position-based kinematic body, and its contact collider is a sensor. The collision and occupancy phases, including collider delta updates, are detailed on [static cell colliders](@/internals/simulation/static-cell-colliders.md); the kinematics-to-collision contact formats are on [contact exchange](@/internals/simulation/contact-exchange.md).

Collision also consumes asynchronous fluid actor samples for impulses and swimming, and rigid-body fluid samples for buoyancy and drag. See [fluid coupling](@/internals/simulation/fluid-coupling.md).
