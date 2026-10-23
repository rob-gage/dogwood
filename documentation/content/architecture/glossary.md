+++
title = "Glossary and coordinate spaces"
description = "Definitions and conversions for Dogwood's world, tile-ring, physics, and camera coordinates."
weight = 70
[extra]
sources = ["engine_world/src/scenes/fluids", "engine_world/src/scenes/scene_position.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/cellular_tiles", "engine_world/src/scenes/chunks/chunk_coordinates.rs", "engine_world/src/scenes/chunks/chunk_data.rs", "engine_world/src/scenes/chunks/chunk_region.rs", "engine_world/src/scenes/chunks/chunk_resident.rs", "engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/chunks/mod.rs", "engine_world/src/scenes/collisions/collision.rs", "engine_world/src/scenes/collisions/mod.rs", "engine_world/src/scenes/kinematics/kinematics_cellular_tile.wgsl", "engine_graphics/src/world/scenes", "engine_graphics/src/renders/scene_renderer.rs", "engine_graphics/src/renders/scene_renderer_sprites.wgsl"]
+++

This page defines Dogwood's world terms and coordinate spaces, with the conversion formulas between them. It is a reference for engine contributors.

## World terms

- **Cell:** one grid square, the smallest simulated unit.
- **Fluid particle:** a continuous-position sample stored separately from cellular cells; see [fluid simulation](@/internals/simulation/fluids.md).
- **Phantom air:** the ambient gas held as a separate amount in each cell's gas, filling empty cells at the scene's ambient gas pressure; see [tile ring](@/internals/world-streaming/tile-ring.md).
- **Fluid raster:** per-physical-ring-cell mass, momentum, and selected material derived from particles, shared by simulation and rendering.
- **Dormant particle:** a chunk-owned record awaiting admission to the live GPU pool; a resident-save copy is distinct from dormant state.
- **Cellular tile (tile):** an 8 × 8 group of cells (`CELLULAR_TILE_SIZE = 8`).
- **Chunk:** 512 × 512 cells, or 64 × 64 tiles (`CHUNK_SIZE = 512`). Chunks are CPU-side storage and persistence units.
- **Region:** an inclusive rectangle of tile or chunk coordinates. Width and height are `top_right - bottom_left + 1`, zero for reversed bounds, and saturate at `usize::MAX`.
- **Requested region:** the tile rectangle requested for streaming.
- **Active region:** the rectangle of tiles currently resident in the GPU tile ring and simulated. The kinematics pass selects which tiles to process based on awake or disturbed neighbors and nearby actors.
- **Tile ring:** a toroidal layout shared by the cellular tile GPU buffers for the active region. A ring offset maps logical tile coordinates to physical slots.
- **Resident chunk:** a chunk tracked by chunk streaming in any state. A retained chunk is required by a region; a dirty chunk has unsaved changes.

The [world storage API](@/external-api/world-storage.md) lists coordinate types and constants. The [scene pipeline](@/architecture/scene-pipeline.md) describes requested and active regions in operation.

## Coordinate spaces

{{ <rustdoc path="engine/scenes/struct.ScenePosition.html" text="ScenePosition" /> }} stores an integer tile coordinate and a fixed-point offset within that tile. Each offset is an unsigned 32-bit fraction, with `2^32` units per tile. Given position `p` and origin `o`, the camera-relative position in tiles is:

```text
relative_x = (p.tile_x - o.tile_x) + (p.offset_x - o.offset_x) / 2^32
relative_y = (p.tile_y - o.tile_y) + (p.offset_y - o.offset_y) / 2^32
```

The containing cell is `tile * 8 + (offset >> 29)` per axis. The implementation subtracts tile coordinates in `i128` before converting to `f64`.

**Edge cases**

- `translate` floors the finite tile offset plus distance into an integer-tile carry and stores the fractional remainder in the offset. A non-finite distance leaves that axis unchanged.
- The float-to-`i64` cast saturates, so an out-of-range carry can give a wrong result even when the final coordinate fits in `i64`, and adding the carry can overflow.
- `cell_coordinates()` uses `i64` arithmetic.

Integer coordinate conversion uses Euclidean division, so negative coordinates map to the containing lower tile or chunk rather than rounding toward zero:

```text
cell (cx, cy)  -> tile  (cx.div_euclid(8),  cy.div_euclid(8))
tile (tx, ty)  -> chunk (tx.div_euclid(64), ty.div_euclid(64))
```

`CellCoordinates` and `CellularTileCoordinates` use `i64` axes; `ChunkCoordinates` uses `i32`. Tile-to-chunk conversion casts the divided `i64` values to `i32`, wrapping out-of-range values.

```text
ScenePosition (tile + 2^32 fractional units)
        │ containing cell: tile * 8 + (offset >> 29)
        ▼
Cell (i64, i64) ── div_euclid(8) ──> Tile (i64, i64)
                                           │ div_euclid(64), cast
                                           ▼
                                      Chunk (i32, i32)

Logical active tile ── ring offset modulo dimensions ──> Physical tile slot
```

For ring mapping, each logical coordinate is made relative to the active region origin, the ring offset is added, then the result is reduced modulo ring width or height. The mapping changes physical slots while preserving logical world coordinates.

## Physics and drawing coordinates

These are distinct spaces. Chunk conversion depends only on integer cell or tile coordinates.

| Space | Unit | Origin |
| --- | --- | --- |
| World tile | tiles | Tile (0, 0) |
| Rapier local tile | tiles | Collision origin (recentered) |
| Camera-relative tile | tiles | Camera `ScenePosition` |
| Pixel | pixels | Camera-relative, scaled by `pixels_per_tile` |
| Clip | normalized | Viewport center (camera maps to clip `(0, 0)`) |

### Rapier origin

Rapier uses a local tile-space origin instead of large absolute world coordinates. `Collision::new` initializes its origin at tile `(0, 0)`.

When `simulate_collisions` refreshes static active-region occupancies, it moves the origin to the region center on the first refresh. It recenters if either axis differs from the center by more than 1024 tiles. Scene simulation passes the active region's bottom-left relative to that origin, multiplied by 8 to express it in cells.

### Drawing

Actor positions are converted to tile offsets from the camera `ScenePosition`. Scene graphics pass the active region's camera-relative offset to the renderer. For positive visible dimensions:

```text
pixels_per_tile = min(texture_width / visible_width, texture_height / visible_height)
clip = 2 * camera_relative_position * pixels_per_tile / vec2(viewport_width, viewport_height)
```

The `clip` expression is the sprite vertex shader's mapping.
