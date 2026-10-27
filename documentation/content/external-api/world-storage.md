+++
title = "Coordinates, tiles, and chunks"
description = "World coordinate types, cellular tile and chunk storage, and their public APIs."
weight = 40
[extra]
sources = ["engine_world/src/scenes/cellular_tiles", "engine_world/src/scenes/chunks", "engine_world/src/materials/material_table.rs", "engine/src/lib.rs"]
+++

This page covers the public coordinate types, size constants, and chunk storage API of the world. It is for game programmers who generate, inspect, or persist world data; most games only need the `Scene` methods in [Scenes](@/external-api/scenes.md).

## Units and constants

The world uses three nested units. A cell is one grid square; a cellular tile (short: tile) is a group of cells; a chunk is a group of tiles.

| Unit | Size in cells | Constant |
| --- | --- | --- |
| Cell | 1x1 | None |
| Cellular tile | 8x8 | {{ <rustdoc path="engine/scenes/cellular_tiles/constant.CELLULAR_TILE_SIZE.html" text="CELLULAR_TILE_SIZE" /> }} (`8`) |
| Chunk | 512x512 (64x64 tiles) | {{ <rustdoc path="engine/scenes/chunks/constant.CHUNK_SIZE.html" text="CHUNK_SIZE" /> }} (`512`) |

The types below are under `engine::scenes::cellular_tiles` or `engine::scenes::chunks`. Coordinates increase rightward on X and upward on Y, as reflected by region fields named `bottom_left` and `top_right`.

## Coordinates and regions

| Type | Representation and behavior |
| --- | --- |
| {{ <rustdoc path="engine/scenes/cellular_tiles/struct.CellCoordinates.html" text="CellCoordinates" /> }} | Public `x`, `y` fields of `i64`; `cellular_tile_coordinates()` finds its parent tile using Euclidean division. |
| {{ <rustdoc path="engine/scenes/cellular_tiles/struct.CellularTileCoordinates.html" text="CellularTileCoordinates" /> }} | Public `x`, `y` fields of `i64`; `chunk_coordinates()` uses Euclidean division, then narrows each quotient to `i32`, which wraps outside the representable chunk-coordinate range. |
| {{ <rustdoc path="engine/scenes/cellular_tiles/struct.CellularTileIndex.html" text="CellularTileIndex" /> }} | Opaque `u32` index with `From<u32>` and `Into<u32>`. |
| {{ <rustdoc path="engine/scenes/cellular_tiles/struct.CellularTileRegion.html" text="CellularTileRegion" /> }} | Inclusive tile bounds `bottom_left` and `top_right`; exposes `width`, `height`, `contains`, `chunk_region`, and row-major `cellular_tile_coordinates()`. |
| {{ <rustdoc path="engine/scenes/chunks/struct.ChunkCoordinates.html" text="ChunkCoordinates" /> }} | Public `x`, `y` fields of `i32`. |
| {{ <rustdoc path="engine/scenes/chunks/struct.ChunkRegion.html" text="ChunkRegion" /> }} | Inclusive chunk bounds `bottom_left` and `top_right`, with `width()` and `height()`. |
| {{ <rustdoc path="engine/scenes/chunks/struct.ChunkRegionIndex.html" text="ChunkRegionIndex" /> }} | Opaque `u64` retained-region identifier with `From<u64>` and `Into<u64>`. |

Euclidean division means negative cell and tile coordinates map to the expected negative parent coordinate rather than truncating toward zero.

## Chunk cell access

### `Chunk`

`Chunk` has public `coordinates`; its cell arrays are private. `Chunk::new(coordinates)` creates an empty chunk, and `fill(material_identifier, color, radiance)` fills cells and resets kinematics to zero. `u32::MAX` selects the material's default radiance.

Tile access uses absolute `CellularTileCoordinates` and returns `Result`:

- `cellular_tile_material_identifiers`, `cellular_tile_colors`, `cellular_tile_radiances`, and `cellular_tile_kinematics` return arrays of 64 entries (`MaterialIdentifier`, packed `u32` RGBA color, packed `u32` radiance, or packed `u64` kinematics).
- `set_cellular_tile_material_identifiers`, `set_cellular_tile_colors`, `set_cellular_tile_radiances`, and `set_cellular_tile_kinematics` replace one tile's matching array.
- Each getter or setter returns `ChunkError::CoordinateMismatch` if the tile belongs to another chunk.
- Row order is row-major within the tile and chunk.

`Chunk::read(reader)` reads a chunk from an `io::Read` and returns `Result<Chunk, ChunkError>`. `Chunk::write(writer)` writes one to an `io::Write` and returns `Result<(), ChunkError>`. See [Chunk storage format](@/internals/world-streaming/chunk-format.md) for serialization behavior.

### `ChunkData` (persistence)

`ChunkData::new(root)` stores chunk files under the given directory, and `path()` returns that path.

- `load(coordinates)` returns `Ok(None)` for a missing file, `Ok(Some(chunk))` for a valid one, or a `ChunkError`.
- `save(&chunk)` writes the chunk and returns `Result<(), ChunkError>`.

### `ChunkError`

| Group | Variants |
| --- | --- |
| Cell data | `CellCapacityExceeded`, `CellIndexOutOfRange`, `CellIndexIsDuplicate`, `CellIsEmpty` |
| Fluid particles | `FluidParticleCapacityExceeded`, `FluidParticleMaterialInvalid`, `FluidParticlePositionInvalid`, `FluidParticleVelocityInvalid` |
| Gas tiles | `GasTileCapacityExceeded`, `GasTileIndexOutOfRange`, `GasTileIndexIsDuplicate` |
| Coordinates | `CoordinateMismatch` |
| I/O and format | `Io`, `InvalidHeader`, `InvalidLength`, `InvalidVersion` |

## Public streaming implementation types

{{ <rustdoc path="engine/scenes/chunks/struct.ChunkStreamingState.html" text="ChunkStreamingState" /> }}, {{ <rustdoc path="engine/scenes/cellular_tiles/struct.CellularTileStreamingState.html" text="CellularTileStreamingState" /> }}, and {{ <rustdoc path="engine/scenes/cellular_tiles/struct.CellularTileBuffers.html" text="CellularTileBuffers" /> }} are public through the `chunks` and `cellular_tiles` module re-exports, but their intended use is scene-internal: `Scene` owns and advances them. Request, response, and resident implementation types are not re-exported from those modules. Games normally use [`Scene`](@/external-api/scenes.md) methods rather than constructing streaming state or GPU buffers directly.

{{ <rustdoc path="engine/scenes/cellular_tiles/constant.CELLULAR_TILE_AWAKE.html" text="CELLULAR_TILE_AWAKE" /> }} (`0b11`) is the awake buffer value for a tile whose dynamic cells and neighbors must be simulated. {{ <rustdoc path="engine/scenes/cellular_tiles/constant.CELLULAR_TILE_GAS_AWAKE.html" text="CELLULAR_TILE_GAS_AWAKE" /> }} (`0b1`) is the gas awake buffer value for a tile whose pressure and neighbors must be simulated. Both belong to the streaming implementation; see the [glossary](@/architecture/glossary.md).

{{ <rustdoc path="engine/scenes/chunks/struct.Chunk.html" text="Chunk" /> }} · {{ <rustdoc path="engine/scenes/chunks/struct.ChunkData.html" text="ChunkData" /> }} · {{ <rustdoc path="engine/scenes/chunks/enum.ChunkError.html" text="ChunkError" /> }}

`ChunkFluidParticle` stores a fluid identifier and exact position/velocity bits. `Chunk::fluid_particles` and `set_fluid_particles` expose dormant particles; see [chunk format](@/internals/world-streaming/chunk-format.md#fluid-particle-section). `Chunk::cellular_tile_gas` and `set_cellular_tile_gas` expose a tile's stored gas, or `None` for default gas; see [chunk format](@/internals/world-streaming/chunk-format.md#gas-tile-section).
