+++
title = "Concurrency and GPU synchronization"
description = "How chunk workers and GPU submissions exchange data without stale or overwritten tiles."
weight = 40
[extra]
sources = ["engine_host/src/acceleration/accelerator_readback.rs", "engine_host/src/acceleration/accelerator_readback_error.rs", "engine_host/src/processors", "engine_world/src/scenes", "engine/src/games/game_instance.rs", "engine_graphics/src/renders/scene_renderer_lighting_pass.rs", "engine_graphics/src/renders/scene_renderer_lighting_clear.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cache.wgsl", "engine_graphics/src/renders/scene_renderer_lighting.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_cascades.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_mips.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_optical.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_spread.wgsl", "engine_graphics/src/renders/scene_renderer_lighting_sprites.wgsl"]
+++

This page explains which work runs on which thread, how GPU results come back, and where the runtime blocks. It is for contributors changing streaming, kinematics, or collision code.

Scene state is advanced by the game update on the host event-loop thread. Background chunk load, generation, and save requests run on the named Rayon `chunk_io` pool. GPU work is submitted through the accelerator queue, and asynchronous buffer-map callbacks report completion through channels.

```text
event-loop thread             chunk_io workers              GPU queue
 Scene::stream ──requests──> load / generate / save
       ^                     └──responses channel──┐
       └──────────────────── stream_chunks polls <──┘
       |
       +── submit tile copies / kinematics ──────────────────>
       |         map_async callback ──channel──> later poll
       +── blocking occupancy submit ───────────────────────>
                 map_async + wait_indefinitely <──────────────
```

## CPU work and channels

### Chunk workers

`Processor` owns named Rayon pools. Its default configuration adds a two-worker dedicated `chunk_io` pool, which `Scene::stream` retrieves lazily. The pool's worker count bounds chunk requests in flight in `ChunkStreamingState`. Jobs return typed load, generation, or save responses over a multi-producer, single-consumer (MPSC) channel.

A call to `stream_chunks` drains only the responses currently available and schedules more work, so file operations do not block the scene update. See [processor pools](@/internals/runtime/processor-pools.md) and [chunk I/O scheduling](@/internals/world-streaming/chunk-io.md).

### GPU readbacks

Tile streaming and kinematics do not use a worker pool. They submit GPU work and call `map_async`; callbacks send map results over MPSC channels. `Scene::stream` polls the device with `Poll` and applies tile responses later.

`CellularTiles` likewise polls and reads available contact and control results before submitting the current tick. Tick N actor and rigid-body contact copies are read from tick N+1, waiting only for the submission containing the copy when contact response needs it. Completed control readbacks set `substep_count` and `resolve_count` for later submissions and never wait.

`CellularTiles` has three readback slots. If all actor-contact slots are busy, that tick's actor contacts are skipped; rigid-body contacts instead accumulate until a slot is available.

`AcceleratorReadback::map` transitions only slots with newly recorded copies into mapping. Slots already mapping retain their metadata and ownership across subsequent submissions, including submissions with no new copies. A slot becomes reusable only after its result is consumed and its buffer is unmapped.

## Queue ordering and readback

When shifting the tile ring, the state encodes a gather and copy of outgoing tiles, submits them, then queues one packed `write_buffer` upload and submits a scatter that writes incoming tiles into the slots they reuse. Queue order makes each departing tile's readback capture the old slot contents before the incoming tile replaces them.

The map callback sends its result over a channel. A later stream call consumes it, marks successful requests ready, and applies ready responses in request-ID order. Retained chunk regions keep destinations available until their responses are applied.

## Wait behavior

The synchronization strategy depends on the operation:

| Operation | Wait behavior | Why |
| --- | --- | --- |
| Static occupancy and settled-dynamic fallback reads | Block with `wait_indefinitely` after mapping. | Static refreshes need current occupancy; the dynamic fallback supplies a snapshot matching the region and ring offset. |
| Ordinary settled dynamic occupancy reads | Poll without waiting and retain the held snapshot if no matching result is ready. | See [occupancy readback](@/internals/simulation/static-cell-colliders.md#occupancy-readback) for slot handling and fallback conditions. |
| Rigid-body changed-cell reads, `read_changed_cells` ([collision world](@/internals/simulation/collision-world.md)) | Block when changed-cell data is needed. | Cells under bodies must detach before the step. |
| Detached-group cell reads, `request_cell_reads` and `read_cell_reads` | Copied at tick N end and read at tick N+1 start; waits only for that copy submission. | Cells no longer static are filtered out using current occupancy before the remaining group becomes a body or debris. |
| Collision changed-cell feedback, including thermal changes | Copied at tick N end and read at tick N+1 start; waits only for that copy submission. | Topology updates are applied on the following tick. |
| Cellular actor and rigid-body contact feedback | Copied during tick N and consumed starting tick N+1; contact reads wait only for their copied submission. Control sizing never waits. | Feedback is exactly one tick late, never delayed by later queue work. |
| Fluid actor feedback | Copied during tick N and read starting tick N+1; waits only for that copy submission when actor response needs it. | Samples match the actor records used by that tick. |
| Normal tile-streaming and kinematics readbacks ([tile streaming](@/internals/world-streaming/tile-streaming.md)) | Map asynchronously; later stream and update calls poll and consume available responses. | These results can be applied after their GPU work completes. |
| Active tile return during `Scene` drop | On success, polls with `wait_indefinitely` until every response has been applied. A polling, mapping, or inactive-chunk error aborts the return and makes `Scene::drop` panic. | Teardown must return all active tile data to chunks. See [resources and lifetimes](@/architecture/resources-and-lifetimes.md#saving-and-drop-behavior). |
| Dirty chunk saves during drop | Attempts to save each remaining dirty chunk synchronously up to three times; if any save still fails, drop panics. | See [chunk I/O scheduling](@/internals/world-streaming/chunk-io.md). |

## Locks and ownership

A failed map is re-requested, but the mapping error also propagates from `Scene::stream`; the native `GameInstance::update` path converts that error to a panic. Streaming and kinematics state is owned through exclusive scene borrows and receives completions through channels, so no locks guard it. For subsystem details, see [tile streaming](@/internals/world-streaming/tile-streaming.md), [kinematics](@/internals/simulation/cellular-kinematics.md), and [collision world](@/internals/simulation/collision-world.md).

## Fluid readbacks

Fluid count, actor, and import status each have three asynchronous staging slots. A full count/actor ring skips that copy; a full import ring defers import submission. One independent resident-save snapshot is allowed at a time. Streaming retains chunks until particle exports or snapshots are applied; teardown waits for imports and reads the live pool synchronously. See [GPU readback policy](@/internals/shaders/fluid-shader.md#readback-policy).

Queue order makes the previous fluid raster available to cellular kinematics and current cellular/rigid-body writes available to the following particle solver. Pressure runs after fluid simulation and consumes current geometry and the new fluid raster. Cellular, actor, rigid-body, and fluid pressure consumers read the buffer published by the preceding tick. Collision-owned changed-cell copies are requested after thermal and fluid heat application, then read the following tick. CPU body responses still use asynchronous older samples; [fluid coupling](@/internals/simulation/fluid-coupling.md) describes the resulting cache behavior.

Lighting preparation, cascade dispatches, spread, and mip generation are encoded on the renderer command encoder before fluid shading and scene drawing; see [lighting pass](@/internals/rendering/lighting-pass.md).
