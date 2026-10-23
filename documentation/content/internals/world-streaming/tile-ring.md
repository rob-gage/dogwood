+++
title = "The cellular tile ring"
description = "Documents the GPU buffers, tile coordinates, ring mapping, and awake flags."
weight = 40
[extra]
sources = ["engine_world/src/scenes/cellular_tiles/cellular_tile_buffers.rs", "engine_world/src/scenes/gases/mod.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_index.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_region.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_coordinates.rs", "engine_world/src/scenes/cellular_tiles/cell_coordinates.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_streaming_state.rs", "engine_world/src/scenes/cellular_tiles/mod.rs", "engine_world/src/scenes/chunks/mod.rs", "engine_world/src/scenes/kinematics/kinematics_cellular_tile.wgsl"]
+++

This page is for contributors working on the GPU side of world streaming. It covers tile and region coordinates, the GPU buffers of the tile ring, how logical tiles map to physical slots, and the awake flags. The tile and chunk definitions are in the [glossary](@/architecture/glossary.md).

## Tile and region coordinates

`CellCoordinates::cellular_tile_coordinates` divides each cell coordinate by 8 with Euclidean division, so negative cell positions map to the tile containing them. Tile coordinates use `i64`. `chunk_coordinates` divides them by 64 with Euclidean division and casts the result to `i32`.

`CellularTileRegion` uses inclusive bottom-left and top-right tile coordinates. Its width and height include both ends, and its iterator yields rows in row-major order (`y` outer, `x` inner). A `CellularTileIndex` wraps the `u32` index for a tile's contiguous cells in the GPU buffers.

## GPU buffers

### Ring size

{{ <rustdoc path="engine/scenes/struct.Scene.html#method.stream" text="Scene::stream" /> }} requests a rectangle of `2 × camera.width` by `2 × camera.height` tiles, using the camera fields at zoom 1.0. Zoom does not change it. With `camera` as the actor's `ActorCameraComponent` and `camera_position` as the `ScenePosition` from its `ActorKinematicsPositionComponent`, the inclusive bounds run from `(camera_position.tile_x - camera.width, camera_position.tile_y - camera.height)` through `(camera_position.tile_x + camera.width - 1, camera_position.tile_y + camera.height - 1)`.

On its first stream, `CellularTileBuffers::new(accelerator, width, height)` allocates a ring at those dimensions. Later region requests must keep the same dimensions.

### Buffers

Each tile contains 64 row-major cells. The buffers are:

| Buffer | Contents | Bytes per tile |
| --- | --- | ---: |
| `material_identifier_buffer` | one `u32` per cell | 256 |
| `color_buffer` | packed RGBA `u32` per cell | 256 |
| `kinematics_buffer` | two `u32` per cell | 512 |
| `awake_buffer` | one `u32` per tile | 4 |
| `rigid_body_cell_buffer` | encoded rigid-body-cell value plus one; zero means none | 256 |
| `gas_buffer` | eight `u32` per cell: four species slots, phantom air, two face flows, padding | 2048 |
| `gas_awake_buffer` | one `u32` per tile | 4 |

For a ring of `width × height` tiles, the material, color, and rigid-body buffers each allocate `width × height × 256` bytes, kinematics allocates twice that, and gas eight times that. Awake and gas awake storage each allocate `max(width × height, 1) × 4` bytes.

Each species slot holds a gas index in its top eight bits and an amount below; an amount of zero marks a free slot. Amounts use 65,536 units per atmosphere (`GAS_AMOUNT_PER_ATMOSPHERE`). Phantom air is the ambient gas filling empty cells, stored as a separate amount. The face flows are `f32` bits for the cell's right and top faces.

Outgoing readbacks and chunk payloads contain material identifiers, colors, kinematics, and gas. On tile entry, streaming also sets the awake and gas awake values and clears rigid-body cell data. See [tile streaming](@/internals/world-streaming/tile-streaming.md) for upload and readback behavior.

## Ring mapping

The tile ring has the dimensions of its initial region. For an active region with bottom-left `(left, bottom)`, width `W`, height `H`, and ring offset `(ox, oy)`, a logical tile `(x, y)` maps as:

```text
logical_x  = x - left              logical_y  = y - bottom
physical_x = (logical_x + ox) % W  physical_y = (logical_y + oy) % H
index      = physical_y * W + physical_x
```

The index addresses a contiguous block of 64 cells in each per-cell buffer, in row-major order within the block.

As the active region moves, offsets rotate modulo the ring dimensions so overlapping logical tiles keep their physical slots. Outgoing data is copied to staging before a slot is reused, then applied to chunks later. See [tile streaming](@/internals/world-streaming/tile-streaming.md) for that transfer path.

## Awake flags

`CELLULAR_TILE_AWAKE` is `0b11`: it sets both `TILE_AWAKE` (`1`) and `TILE_DISTURBED` (`2`). The kinematics collect pass schedules tiles with either bit and their neighboring tiles. It also activates tiles near actors. Newly entering tiles are initialized with this value.

For the complete scene stream and simulation ordering, see the [scene update pipeline](@/architecture/scene-pipeline.md).
