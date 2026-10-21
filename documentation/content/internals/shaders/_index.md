+++
title = "Shaders and GPU data"
description = "Reference for Dogwood's six WGSL shaders, their owning passes, GPU bindings, and dispatch sites."
weight = 60
sort_by = "weight"
template = "section.html"
[extra]
sources = ["engine_world/src/scenes/kinematics/kinematics_fluids.wgsl", "engine_graphics/src/renders", "engine_world/src/scenes/kinematics", "engine_world/src/scenes/collisions", "engine_world/src/scenes/edits"]
+++

This section is the reference for Dogwood's six embedded WGSL shaders, for engine contributors. Each shader's Rust pass creates the matching pipeline and bind groups, packs CPU-side data where needed, and submits work. Details live in the [render shader reference](@/internals/shaders/render-shaders.md), [kinematics shader reference](@/internals/shaders/kinematics-shader.md), [collision shader reference](@/internals/shaders/collision-shaders.md), and [edit shader reference](@/internals/shaders/edit-shader.md).

## Shader table

| Shader | Stage and entry points | Owner pass | Binding summary | Dispatch site |
|---|---|---|---|---|
| `engine_graphics/src/renders/scene_renderer_cellular.wgsl` | Render: `vertex`, `fragment` | `SceneRendererCellularPass` | Group 0: uniforms, material identifiers, packed colors | Fullscreen triangle draw in `SceneRendererCellularPass::draw` |
| `engine_graphics/src/renders/scene_renderer_sprites.wgsl` | Render: `vertex`, `fragment` | `SceneRendererSpritesPass` | Group 0: uniforms and sampler; group 1: one sprite texture | Four-vertex instanced draw for each texture batch in `SceneRendererSpritesPass::draw` |
| `engine_world/src/scenes/kinematics/kinematics_cellular_tile.wgsl` | Compute: `collect`, `clear`, `prepare`, `integrate`, `request`, `pick`, `resolve_begin`, `resolve_even`, `resolve_odd`, `stage`, `commit`, `contact` | `KinematicsPass` | Group 0: uniforms and cell, material, actor, rigid-body, control and contact buffers; group 1: indirect dispatch records | Sequential compute dispatches in one compute pass; `prepare` writes indirect workgroup counts used by later stages |
| `engine_world/src/scenes/collisions/collision_cellular_tile.wgsl` | Compute: `compute` | `CollisionCellularTilePass` | Uniforms, material IDs, occupancy output, rigid-body cell IDs and kinematics | One invocation per tile in the active region, dispatched in 64-thread workgroups |
| `engine_world/src/scenes/collisions/collision_rigid_body.wgsl` | Compute: `release`, `rasterize`, `write_cells`, `read_cells` | `CollisionRigidBodyPass` | Uniforms, cellular buffers, rigid-body records, item lists, cell writes/read results and changed-cell atomics | Per-tile release/rasterize and per-cell read/write dispatches; shared helper selects workgroup count |
| `engine_world/src/scenes/edits/edit_cellular_tile.wgsl` | Compute: `compute` | `EditCellularTilePass` | Edited tile records, material IDs, colors, kinematics and atomic awake flags | One 64-thread workgroup per edited tile |

## Common conventions

- Storage order follows the active tile ring where applicable. Logical tile coordinates add the ring offset and wrap modulo region dimensions.
- Cells within a tile use row-major order.
- Several CPU/WGSL records are packed manually, little-endian, so their padding and offsets are part of the pass contract. Each reference page documents its layouts and Rust writer.

## Related pages

Rendering and pass scheduling are covered in [cellular rendering](@/internals/rendering/cellular-pass.md), [sprite rendering](@/internals/rendering/sprite-pass.md), [cellular kinematics](@/internals/simulation/cellular-kinematics.md), [static cell colliders](@/internals/simulation/static-cell-colliders.md), [rigid bodies](@/internals/simulation/rigid-bodies.md), and the [edit pipeline](@/internals/simulation/edit-pipeline.md).

The [fluid solver](@/internals/simulation/fluids.md) adds `fluid_particle.wgsl`, driven by `FluidParticlePass`, for particle compaction, neighbor search, pressure correction, and fluid raster/feedback generation.

`scene_renderer_fluid.wgsl` classifies and shades raster cells with compute, then draws the shaded colors with a fullscreen triangle. See [fluid rendering](@/internals/rendering/fluid-pass.md).
