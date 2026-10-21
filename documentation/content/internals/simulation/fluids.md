+++
title = "Fluid simulation"
description = "GPU fluid particles, body feedback, edits, streaming, and persistence."
weight = 75
[extra]
sources = ["engine_world/src/scenes/fluids", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/cellular_tiles", "engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/kinematics", "engine_world/src/scenes/collisions/collision.rs"]
+++

Fluids use a fixed-capacity GPU particle pool separate from cellular materials. The first stream allocates it using the scene's [capacity factor](@/external-api/scenes.md#fluid-capacity). `Fluids` owns queued edits and imports; `FluidParticlePass` encodes compute work.

## Solver

`fluid_particle.wgsl` compacts and rebases particles, applies erasures and spawns, prepares solid and moving boundaries, then runs position-based fluid (PBF) substeps. Dense bins use count, prefix-scan, and scatter passes for neighbor lookup. Each substep predicts motion, solves density constraints with Jacobi position corrections, commits velocity, and applies XSPH smoothing. Material cohesion and artificial pressure contribute to correction. Actor projection and solid boundaries constrain particle motion.

| Constant | Value |
| --- | --- |
| `FLUID_SUBSTEP_COUNT` | 4 |
| `FLUID_ITERATION_COUNT` | 2 per substep |
| `FLUID_KERNEL_RADIUS` | 2.5 cells |
| `FLUID_BIN_SIZE` | 3 cells |
| `FLUID_RELAXATION` | 0.01 |
| `FLUID_COHESION_SCALE` | 0.5 |
| `FLUID_COHESION_NORMALIZATION_2D` | 54.584815 |
| `FLUID_VELOCITY_MAXIMUM` | 4 cells/tick per axis |
| `FLUID_RASTER_SCALE` | 4096 |
| `FLUID_WORKGROUP_SIZE` | 128 |
| `FLUID_SCAN_WORKGROUP_SIZE` | 256 |
| `FLUID_TIME_STEP` | 1/60 second |

## GPU records and feedback

Particle state and exact-position import commands each occupy 32 bytes; edit commands occupy 16 bytes. Export records occupy 40 bytes after a leading count. Dynamic uniforms use 152 bytes within a 256-byte stride. Actor records are shared with cellular kinematics at a 64-byte stride. Actor feedback records occupy 32 bytes and encode fixed-point impulse and immersion/current/material samples (`FLUID_ACTOR_CONTACT_SCALE = 1024`).

The cellular-side coupling helpers and rigid-body fluid kernels live in `engine_world/src/scenes/kinematics/kinematics_fluids.wgsl`, composed into the same cellular shader module; see [source composition](@/internals/shaders/kinematics-shader.md#source-composition).

A physical cell raster stores mass, momentum, and material identifiers, with sparse tile tracking and contact data. Cellular kinematics consumes it for cell/body interactions. Collision consumes completed fluid actor feedback and rigid-body fluid samples for buoyancy, drag, and [swimming](@/internals/actors/pawn-movement.md#swimming). Counts and actor samples use three asynchronous readback slots; feedback can lag the current GPU state.

## Streaming and saving

Fluid edits queue erasures and spawns; conservative reservations prevent exceeding live capacity. Dormant chunk particles import at their exact positions and velocities. Imports are bounded in whole-tile groups, and rejected records return to their source chunks. Compaction exports particles leaving the active region into outgoing tile requests.

`Scene::save` requests a resident particle snapshot alongside cellular saving. Chunks are retained until the mapped snapshot is applied; `is_saved` includes pending particle saves. Drop drains unspawned imports and synchronously reads live particles into chunk storage. See [chunk format](@/internals/world-streaming/chunk-format.md#fluid-particle-section).

> **Not implemented:** Fluid particle rendering. The particle raster supports simulation feedback but is not connected to the scene renderer in this change.
