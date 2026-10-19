+++
title = "CPU worker pools"
description = "How named Rayon pools are validated, sized, and made available to engine systems."
weight = 40
[extra]
sources = ["engine_host/src/processors", "engine_world/src/scenes/scene.rs"]
+++

This page describes how `Processor` validates, sizes, and names its CPU worker pools. It is for engine contributors; for configuring pools from a game, see [host types](@/external-api/host.md).

`Processor` owns named Rayon thread pools. `thread_pool(name)` returns `Option<Arc<rayon::ThreadPool>>`, cloning a handle only when the name exists. The configured worker count limits threads in a pool, not queued jobs. The {{ <rustdoc path="engine/struct.Processor.html" text="Processor" /> }} and its configuration and error types are re-exported by the `engine` facade.

## Configuration and validation

`ProcessorConfiguration` has these fields:

- `processor_capacity`: CPU worker budget for weighted pools; dedicated pool workers are added separately.
- `processor_capacity_reserved`: capacity set aside from CPU-bound allocation.
- `pool_weights`: weighted CPU-bound pools.
- `dedicated_worker_pool_counts`: fixed-count pools.

Its default uses `thread::available_parallelism()`, falling back to one. It sets `processor_capacity_reserved` to `1` when capacity is greater than one; that capacity is excluded from CPU-bound pool allocation for the main, window, or tick thread.

### Validation

`Processor::new` rejects each of these:

- Zero total capacity.
- A reservation above the total.
- Empty or duplicate names across both pool lists.
- Zero CPU-pool weights or zero dedicated counts.
- More CPU-bound pools than unreserved workers.
- A relative-weight sum that overflows the supported `u128` range.

The CPU-bound budget is `processor_capacity - processor_capacity_reserved`. Dedicated pools are created with their configured counts outside that allocation, so total workers can exceed `processor_capacity`. If Rayon cannot build a pool, `ProcessorError::RayonThreadPoolBuild` includes its name and source error.

## Weighted allocation

The allocator works in these steps:

1. Assign one worker to every CPU-bound pool.
2. Distribute the remaining budget proportionally to each pool's relative weight, keeping each integer quotient and fractional remainder.
3. Give any workers left after rounding to the largest remainders; equal remainders preserve input order.

With no CPU-bound pools, allocation returns an empty list. Each Rayon pool names its threads `dogwood-{pool_name}-{thread_index}`.

## Defaults

`Processor::default()` creates these pools:

| Pool | Kind | Size |
| --- | --- | --- |
| `simulation` | CPU-bound | Weight 3 |
| `world_generation` | CPU-bound | Weight 1, only when at least two unreserved workers are available |
| `chunk_io` | Dedicated | Two workers |

With one available logical processor, the default reserves zero workers and configures a one-worker `simulation` pool plus the two-worker `chunk_io` pool.

`Processor::default()` panics if configuration validation or Rayon pool construction fails. If `chunk_io` is missing when `Scene::stream` initializes chunk streaming, it returns `SceneError::ChunkThreadPoolUnavailable`. See [host internals](@/internal-api/host.md) for pool construction and configuration, and [chunk I/O scheduling](@/internals/world-streaming/chunk-io.md) for how `Scene::stream` supplies the pool.
