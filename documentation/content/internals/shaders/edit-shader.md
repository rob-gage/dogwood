+++
title = "Edit shader"
description = "Documents the edit compute record, bindings, per-tile dispatch, masks, and cell writes."
weight = 50
[extra]
sources = ["engine_world/src/scenes/edits/edit_cellular_tile.wgsl", "engine_world/src/scenes/edits/edit_cellular_tile_pass.rs", "engine_world/src/scenes/edits/mod.rs"]
+++

This page is the reference for `edit_cellular_tile.wgsl`, which applies already-validated edit data to the active cellular buffers, for engine contributors. Request validation, edit merging, and Insert collision masks belong to the [edit pipeline](@/internals/simulation/edit-pipeline.md).

## Entry point and dispatch

The single compute entry point, `compute`, runs with 64 threads per workgroup. One workgroup handles one edited cellular tile (8x8 cells), and each local invocation handles one cell.

`EditCellularTilePass::write_cellular_tiles` uploads the records, dispatches one workgroup per record in `edit_cellular_tiles` along X, and submits the compute command. An empty slice returns immediately. The pass allocates room for `EDIT_CELLULAR_TILE_COUNT_MAXIMUM` records.

## Bindings and record layout

Group 0 bindings:

| Binding | Resource | Access |
|---:|---|---|
| 0 | `EditCellularTile` records | read-only |
| 1-2 | Material identifiers, colors | read/write |
| 3 | Packed kinematics (`array<vec2<u32>>`) | read/write |
| 4 | Awake flags (atomic) | read/write |
| 5 | Gas (`array<vec4<u32>>`, two per cell) | read/write |
| 6 | Gas awake flags (atomic) | read/write |

Each tile record occupies 536 bytes, serialized little-endian by Rust:

| Offset | Field | Size |
|---:|---|---:|
| 0 | `cellular_tile_index: u32` | 4 B |
| 4 | `ambient_gas_amount: u32` | 4 B |
| 8 | `inserted_cells: vec2<u32>` | 8 B |
| 16 | `replaced_cells: vec2<u32>` | 8 B |
| 24 | `material_identifiers: array<u32, 64>` | 256 B |
| 280 | `colors: array<u32, 64>` | 256 B |

### Masks and addressing

- The two-word masks cover the tile's 64 cells in row-major order. The shader selects word `cell / 32` and bit `cell % 32`.
- Cell buffers are addressed at `cellular_tile_index * 64 + cell`.
- Gas is addressed at `(cellular_tile_index * 64 + cell) * 2` for the species slots and one more for phantom air and flows.
- The awake and gas awake buffers are addressed at `cellular_tile_index`.

## Cell write behavior

Each invocation reads its cell's inserted and replaced mask bits:

- A replacement always writes.
- An insertion writes only when the current material identifier is zero.

On a write, the shader zeros the cell's kinematics, atomically ORs `3` into the tile's awake value (awake and disturbed flags), and ORs `1` into its pressure awake value. The rest depends on the material:

- A gas material leaves the cell empty with zero color. Its gas becomes one species slot of that gas holding the ambient amount, or one atmosphere when the ambient amount is lower, with no phantom air or flow.
- `EMPTY` is copied with its color, and the gas becomes phantom air at the ambient amount alone.
- Any other material is copied with its color. The cell keeps its species slots, which the [pressure simulation](@/internals/simulation/pressure.md) later pushes out, but loses its phantom air and flows.

Queued pressure edits use atmospheres. Static cells receive signed stored pressure; other cells add to phantom air or proportionally reduce their gas.

The shader does not evaluate actor colliders or material registration; those decisions are made before the pass receives the records.
