+++
title = "Fluid streaming and persistence"
description = "Particle capacity, edits, exact-position imports, export ownership, snapshots, and teardown."
weight = 35
[extra]
sources = ["engine_world/src/scenes/fluids", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/cellular_tiles", "engine_world/src/scenes/chunks", "engine_world/src/scenes/edits"]
+++

Fluid particles have three distinct storage roles: live GPU state, dormant chunk records awaiting import, and resident-save copies of still-live particles. Treating the last two as the same would duplicate live particles. [Chunk format](@/internals/world-streaming/chunk-format.md#fluid-particle-section) defines persisted records; this page describes ownership transitions.

## Capacity and edit reservations

The first stream fixes pool capacity from the scene's [fluid capacity factor](@/external-api/scenes.md#fluid-capacity). Dense scratch arrays and region maps are also allocated at that point. The default pool holds one particle per four active-region cells; filling every cell with fluid needs a larger factor.

Admission reserves the newest known GPU count plus spawn attempts not represented by that readback, plus queued imports. Submission ticks and spawn revisions associate delayed count results with the work they include. The revision counts attempted spawns, including overwritten edits, not just successful particles. No immediate capacity credit is taken for pending erasures.

> **Limitation:** Reservations are conservative. An edit can return `CapacityReached` although the GPU would reject some Insert spawns or pending erasures would free space. Capacity becomes available after a suitable count readback, not necessarily on the next CPU call.

## Editing fluid

`Scene::edit` first checks particle capacity, then normal edit validation, then queues both cellular and particle effects. Particle actions keep the latest spawn per world cell and a separate erasure set. Non-fluid edits remove queued spawns and request particle erasure; fluid Replace also erases prior particles. The actual effects occur in streaming and the following particle simulation.

The cellular edit path substitutes empty material/zero color for written fluid cells. Fluid is not left as an identifier in the cellular array. The particle pass compacts erased particles before spawning, so Replace can exchange material without overlapping the old particles. Spawn commands create a particle at the cell center with repeatable ±0.01-cell jitter and zero velocity.

The cellular part of fluid Insert uses ordinary edit collision rules. Its independently queued particle spawn checks nonempty cellular matter and an occupancy-generation stamp for existing particles on the GPU; it does not receive the cellular collider mask. Actor overlap is resolved later by particle projection. It creates at most one new particle per occupied cell stamp. GPU capacity is checked again atomically. Imported records may share the same cell to preserve compressed saves; they do not use Insert's one-per-cell rejection.

> **Limitation:** Particle erasures are queued separately from the cellular Insert acceptance test. A non-fluid Insert's particle erasure is not conditioned on the eventual cellular write succeeding. Per-edit particle colors are retained for persistence; rendering uses the fluid material's color.

## Dormant imports

After streaming, resident tiles are scanned for dormant particles when no save or import handoff blocks admission. Imports take whole tile groups up to the available reservation and the per-tick target; a first tile larger than that target can be selected whole. A GPU import submission is capped to pool capacity; queued excess waits.

Selecting particles removes them from their active CPU chunk and marks it dirty. Integer world/chunk offsets are subtracted before converting to local floats, preserving fractions at large coordinates. The import retains exact position, velocity, and material. The GPU rejects unregistered/non-fluid IDs, out-of-region positions, nonempty cellular cells, or a full pool. An asynchronous flag per import distinguishes acceptance from rejection; rejected original records return to their source chunks and can be retried. If chunk application fails, the rejected list is restored for retry.

Ring movement waits while imports remain queued, mapped results remain pending, or rejected records await return. This keeps their local coordinates valid. Rejected records in blocked cells can remain dormant rather than vanish.

## Region movement and exports

A ring shift copies outgoing cellular tiles before reusing their slots and allocates a particle export buffer on that request. It refuses another shift while any earlier particle export has not been submitted. During the following simulation, compaction rebases positions by the old-minus-new region origin, erases selected cells, and exports particles outside the new region. Survivors are packed into the other state buffer.

Exports encode signed 64-bit whole world-cell coordinates as low/high words, plus fractional position, velocity, and material. They avoid representing a large absolute world position as one float. The scene marks the oldest export request submitted only after the particle simulation succeeds; mapping starts afterward.

A request is applied only after both cellular and particle buffers are ready. Old chunk retention remains attached until the data is applied in request order. Re-entry into an outgoing tile waits for its previous return, preventing stale CPU data from being imported over newer exported state.

## Resident saves

`save()` requests cellular returns and a separate asynchronous particle snapshot. Snapshot origin comes from the last simulated pool origin, which can differ from the latest streamed region. The scene retains that region's chunks until snapshot application. Imports and saves gate one another so a snapshot does not treat an in-flight import as either missing or duplicated state.

Snapshot application removes the prior resident-save copies by exact record matching, preserves dormant records, and appends the new live snapshot. The recorded copy list is removed before later imports or exports treat those chunk records as dormant. Dirty chunk saves begin only after the requested cellular return and particle work have completed. `is_saved()` includes those pending phases.

The snapshot copies live state without unloading it. Save progress needs subsequent `stream` calls; queued particle edits and imports also need `simulate` to advance. A save is a queued snapshot, not a promise that continuously evolving simulation has stopped changing.

> **Limitation:** `Scene::stream` can request the resident snapshot before applying that call's edits, and particle edits are not materialized until simulation. If a save must contain newly queued fluid edits, run their stream/simulate step before calling `save()`. Waiting for `is_saved()` alone does not replay pending particle edits into an already captured snapshot.

## Teardown

Scene drop first applies cellular edits and returns tiles. It then waits for import statuses, returns unspawned/rejected imports, synchronously reads live particles using the pool's last origin, and replaces the resident snapshot. Chunk-state drop persists the resulting dirty chunks. See [drop failure policy](@/architecture/resources-and-lifetimes.md#drop).

> **Limitation:** Teardown does not execute a final fluid simulation to materialize queued fluid spawn/erasure edits. Drive the normal stream/simulate lifecycle before saving or closing when those edits must be reflected in particle state.
