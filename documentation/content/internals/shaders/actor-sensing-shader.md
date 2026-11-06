+++
title = "Actor sensing shader"
description = "GPU bindings and result layout for actor cell sensing."
weight = 65
[extra]
sources = ["engine_world/src/scenes/actor_sensing/actor_sensing.wgsl", "engine_world/src/scenes/actor_sensing/actor_sensing_pass.rs", "engine_world/src/scenes/actor_sensing/mod.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_pass.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_buffers.rs", "engine_world/src/scenes/fluids/fluid_particle.wgsl", "engine_world/src/scenes/thermal/thermal_cellular_tile.wgsl", "engine_world/src/scenes/pressure/mod.rs"]
+++

`ActorSensingPass` runs `sense` once per cellular tile actor record at the end of `Scene::simulate`. Each workgroup scans cell centers near one actor, using the active region and toroidal ring offset to address cellular and fluid buffers. The [actor sensing page](@/internals/simulation/actor-sensing.md) describes the public counts and configuration.

## Bindings

All resources are in group 0.

| Binding | Resource | Access | Contents |
|---:|---|---|---|
| 0 | `ActorSensingUniforms` | Uniform | Region/ring dimensions, actor/tag counts, four form counts, raster scale, margins, gas thresholds, and fluid thresholds. Twenty 32-bit words (80 bytes), including padding. |
| 1 | `ActorSensingActor[]` | Read-only storage | Cellular tile actor records; collider center, velocity, rotation, dimensions, bounds, shape, mass, and swimming flag. |
| 2 | `u32[]` | Read-only storage | Material identifiers by physical cell. |
| 3 | `u32[]` | Read-only storage | Rigid-body cell identifiers; zero means no rigid-body cell. |
| 4 | `u32[]` | Read-only storage | Fluid raster records, four words per cell: mass, momentum x/y, and selected fluid identifier. |
| 5 | `vec4<f32>[]` | Read-only storage | Fluid material properties, two records per fluid index; rest density is the first record's x component. |
| 6 | `vec4<u32>[]` | Read-only storage | Gas records, two `vec4` values per cell. The first record's four 24-bit amounts are summed as species gas. |
| 7 | `u32[]` | Read-only storage | Material tag masks, laid out by cellular static, cellular dynamic, fluid, then gas indices. |
| 8 | `vec2<u32>[]` | Read-only storage | Shared per-cell temperature and matter amount; temperature is `x` as `f32` kelvin. |
| 9 | `f32[]` | Read-only storage | Published per-cell gauge pressure in atmospheres. |
| 10 | `atomic<u32>[]` | Read-write storage | Sixteen words per actor: eight tag counts, breathable count, breathing count, maximum temperature and absolute pressure as `f32` bit patterns, then four padding words. |

## Dispatch and counting

`sense` uses 64 invocations per workgroup and dispatches one workgroup per actor, up to 32. Each invocation strides over the actor's collider bounds expanded by its maximum configured margin and one cell. Signed distance at the cell center decides contact and breathing-band membership. Atomic increments combine the per-cell counts.

For touching cells, cellular and fluid masks retain their existing selection and rigid-body exclusions. Eligible gas species masks are ORed into that cell's mask when their individual amount reaches `gas_tag_amount`; gas tags are counted even in rigid-body cells and cells inside an actor collider. For touching or breathing-band cells, valid shared temperatures and published pressures update the two maxima. The shared temperature already includes the thermal pass's exchange with rasterized fluid heat. For breathing-band cells, the pass counts breathability only when there is no matter or rigid-body cell, fluid coverage is below `DROWNING_COVERAGE` (0.5), and the sum of species amounts is within the configured gas limit.

The Rust pass clears the result words, dispatches the compute work, and copies the active actor records into one of three asynchronous readback slots. A full set of slots skips the copy without blocking. Result words are decoded as little-endian `u32` values on a later nonblocking poll. See [GPU synchronization](@/architecture/concurrency-and-gpu-sync.md#gpu-readbacks).
