+++
title = "Chunk storage format"
description = "Describes chunk cell arrays, sparse versioned files, naming, and validation."
weight = 10
[extra]
sources = ["engine_world/src/scenes/chunks/chunk.rs", "engine_world/src/scenes/chunks/chunk_data.rs", "engine_world/src/scenes/chunks/chunk_error.rs", "engine_world/src/scenes/chunks/chunk_coordinates.rs"]
+++

This page is for contributors who need the exact in-memory layout and on-disk format of a chunk, including how saves stay atomic.

## In-memory layout

{{ <rustdoc path="engine/scenes/chunks/struct.Chunk.html" text="Chunk" /> }} is a 512 × 512 cell CPU store. It holds four parallel row-major vectors with one entry per cell:

- a `MaterialIdentifier`
- a packed RGBA `u32` color
- a packed `u32` radiance
- a packed `u64` kinematics value, the packed per-cell motion state (see [cellular kinematics](@/internals/simulation/cellular-kinematics.md))

A chunk also owns a sparse list of `ChunkFluidParticle` records and sparse maps of gas cells and matter records for cellular tiles that differ from their defaults.

A newly created chunk fills these with `MaterialIdentifier::EMPTY`, zero color and radiance, and zero kinematics. `fill` replaces the cell arrays, sets kinematics to zero, and clears the particle list, gas map, and matter map.

For a cell at local coordinates `(x, y)`, the row-major cell index is `y * 512 + x`. Tile accessors copy or replace eight rows of eight entries at a time. They first check that the tile's chunk coordinates match the chunk. They then use Euclidean remainder to find the tile's local position within the chunk's 64 × 64 tiles. Sizes are summarized in [coordinates, tiles, and chunks](@/external-api/world-storage.md).

## Sparse file bytes

`Chunk::write` emits a 22-byte little-endian header followed by zero or more 24-byte cell entries, a particle section, and a gas tile section:

| Header offset | Size | Value |
| --- | ---: | --- |
| 0 | 8 | ASCII `DOGWOOD` followed by NUL |
| 8 | 2 | Format version `9` |
| 10 | 4 | Chunk x coordinate as `i32` |
| 14 | 4 | Chunk y coordinate as `i32` |
| 18 | 4 | Number of stored entries as `u32` |

Each entry contains the row-major cell index (`u32`), material identifier (`u32`), packed color (`u32`), packed radiance (`u32`), and kinematics (`u64`), all little-endian. The writer omits a cell only when its material is `EMPTY` and both color and radiance are zero; therefore its kinematics is omitted too. Matter is stored separately as sparse 8 × 8 tile records, each with a tile index and 128 words (temperature bits and amount per cell). Other entries are written in ascending cell-index order.

### Reading and validation

The reader checks the complete header before allocating the entry buffer. Each failure maps to a `ChunkError` variant:

| Condition | Error |
| --- | --- |
| Bad magic | `InvalidHeader` |
| Unsupported version | `InvalidVersion` |
| Entry count above the chunk cell capacity | `CellCapacityExceeded` |
| Truncated or trailing bytes | `InvalidLength` |
| Non-EOF I/O failure | `Io` |
| Cell index out of range | `CellIndexOutOfRange` |
| Duplicate cell index | `CellIndexIsDuplicate` |
| Empty material with zero color and radiance | `CellIsEmpty` |
| Embedded coordinates differ from the requested coordinates (`ChunkData::load`) | `CoordinateMismatch` |

A missing file yields `Ok(None)`; other I/O errors propagate. Omitted indices load as `EMPTY` cells with zero color and radiance and zero kinematics.

## Files and saves

`ChunkData` stores chunks beneath its configured directory. A filename is `{x:08x}{y:08x}.chunk`, with each signed coordinate cast to `u32` before fixed-width lowercase hexadecimal formatting. Saving creates the directory, writes to a sibling file ending in `.chunk.temporary`, calls `sync_all`, then renames the temporary file to the destination. If opening, writing, syncing, or renaming the temporary file fails, it attempts to remove the temporary path.

### Temporary file names

The temporary sequence is shared by cloned `ChunkData` values. Separately constructed values have independent sequences and can choose the same temporary path. Temporary files are opened with `create_new`; if the chosen path already exists, opening it fails and the error cleanup attempts to remove that path.

Chunk loading and save scheduling are covered in [chunk I/O scheduling](@/internals/world-streaming/chunk-io.md); the resident state machine is in [chunk residency](@/internals/world-streaming/chunk-residency.md).

## Fluid particle section

Version 9 stores 36 bytes per particle: position and velocity, material identifier, packed color, temperature, amount, and packed radiance, each as a little-endian `u32` (float values preserve their bits). Float bits are preserved exactly. The count cannot exceed `512 * 512`; positions must be finite and in `[0, 512)`, velocities finite, and identifiers fluid-form. Failures return `FluidParticleCapacityExceeded`, `FluidParticlePositionInvalid`, `FluidParticleVelocityInvalid`, `FluidParticleTemperatureInvalid`, or `FluidParticleMaterialInvalid`.

`Chunk::fluid_particles` reads the list; `set_fluid_particles` validates and replaces it. `ChunkFluidParticle::new` validates the record and preserves supplied float bits; storage validates records again.

## Matter tile section

Version 9 stores sparse matter tiles after gas tiles. Each record is a tile index (`u32`) followed by 128 `u32` words, two per row-major cell: temperature bits and amount. Missing tiles use the scene ambient temperature and default amount.

## Gas tile section

Version 7 appended a little-endian `u32` gas tile count after the particle records. Each tile record contains its row-major tile index (`u32`) followed by 512 little-endian `u32` gas words in row-major cell order. The count cannot exceed 4096; tile indices must be below 4096 and unique. Failures return `GasTileCapacityExceeded`, `GasTileIndexOutOfRange`, or `GasTileIndexIsDuplicate`. Truncated sections return `InvalidLength`.

`Chunk::cellular_tile_gas` returns `None` for default gas. `set_cellular_tile_gas` stores or removes a tile entry.

> **Limitation:** The reader accepts only version 9; older files return `InvalidVersion`.

The format validates that a particle identifier is fluid-form, but does not check that it is registered in a particular scene material table. GPU admission checks registration later. Saved records do not include particle sleep state, actor samples, raster data, or material definitions; reconstructed simulation uses the scene's registered materials.
