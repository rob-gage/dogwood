+++
title = "Collision shaders"
description = "Documents GPU occupancy extraction and rigid-body rasterization bindings, records, dispatches, and readbacks."
weight = 40
[extra]
sources = ["engine_world/src/scenes/collisions/collision_cellular_tile.wgsl", "engine_world/src/scenes/collisions/collision_rigid_body.wgsl", "engine_world/src/scenes/collisions/collision_cellular_tile_pass.rs", "engine_world/src/scenes/collisions/collision_rigid_body_pass.rs", "engine_world/src/scenes/collisions/collision_cellular_tile.rs", "engine_world/src/scenes/collisions/collision_rigid_body.rs"]
+++

This page is the reference for the two collision compute shaders, for engine contributors. `CollisionCellularTilePass` extracts occupancy bitsets for voxel colliders; `CollisionRigidBodyPass` releases, rasterizes, reads, and writes the cells of cellular rigid bodies. Collider construction is covered by [static cell colliders](@/internals/simulation/static-cell-colliders.md) and [cellular rigid bodies](@/internals/simulation/rigid-bodies.md).

## Occupancy extraction

`collision_cellular_tile.wgsl` has one compute entry point, `compute`, with a 64-thread workgroup.

### Bindings

Group 0 holds these bindings:

| Binding | Resource | Access |
|---:|---|---|
| 0 | 32-byte `CellularTileUniforms`: `region_width`, `region_height`, ring x/y, dynamic selector, padding | uniform |
| 1 | Material IDs | read-only |
| 2 | Occupancy output, `array<vec2<u32>>` | writable |
| 3 | Rigid-body cell IDs | read-only |
| 4 | Packed kinematics | read-only |

### Dispatch and output

Rust dispatches `ceil(region width * region height / 64)` workgroups, one invocation per tile of the active region. Each invocation maps its logical tile through the ring offset and scans the tile's 64 cells.

- Static mode marks nonempty static material IDs.
- Dynamic mode marks dynamic cells whose resting bit is set.
- Cells claimed by a rigid body are excluded.

The output is two `u32` words per tile, where bit `y * 8 + x` marks occupancy. The pass decodes the output into row-major `CollisionCellularTile` values. Static reads and the settled-dynamic fallback wait for a staging-buffer mapping; ordinary settled-dynamic reads use three `AcceleratorReadback` slots. See [occupancy readback](@/internals/simulation/static-cell-colliders.md#occupancy-readback).

## Rigid-body cell pass

`collision_rigid_body.wgsl` exposes five compute entry points, all with 64-thread workgroups: `release`, `rasterize`, `write_cells`, `read_cells`, and `copy_states`.

### Rigid-body pass bindings

| Binding | Resource |
|---:|---|
| 0 | 48-byte `CollisionRigidBodyUniforms`: region dimensions and ring offset (0-15), region origin (16-23), item offset/count (24/28), mode (32), padding (36-47) |
| 1-5 | Mutable scene buffers: material IDs, paired color/radiance, kinematics, awake flags, atomic rigid-body cell IDs |
| 6 | Rigid-body tile cell data |
| 7 | Mutable `CollisionRigidBodyCellularTile` records |
| 8 | Read-only `CollisionRigidBody` records |
| 9 | Item indices |
| 10 | Cell write records |
| 11 | Cell read results, four words per cell |
| 12 | Atomic changed-cell results |
| 13 | Stored pressure bits and integrity of every rigid-body tile cell |
| 14 | Gas cells, read as eight words per cell |

### Records

Rust serializes both records little-endian.

- `CollisionRigidBodyCellularTile` is 32 bytes: body index and padding (0-7), tile coordinate (8-15), current 64-cell mask (16-23), previous mask (24-31).
- `CollisionRigidBody` is 64 bytes: six `vec2<f32>` fields (0-47) holding position, rotation cosine/sine, previous position, previous rotation, velocity, and center; angular velocity at 48; mass at 52; 8 bytes of padding.

### Dispatch

The Rust `dispatch` helper chooses the workgroup count per entry point:

- `release` and `rasterize`: one workgroup per listed rigid-body tile. The 64 local invocations share the bounded cell scan.
- `write_cells`, `read_cells`, and `copy_states`: `ceil(item_count / 64)` workgroups, one invocation per cell or copy.
- Empty dispatches are skipped.

Uniforms occupy six 256-byte slots, for write, return, clear, rasterize, read, and changed-cell operations; `copy_states` uses the read slot. Writes are chunked at 65,536 cells per dispatch, and one tick reads at most 65,536 cells.

### Modes

`release` takes its behaviour from the uniform `mode`:

- `MODE_CLEAR` saves each vacated static cell's stored pressure and integrity in the body's cell state, then empties the cell and its stored pressure. If the stored body material is dynamic, the cell becomes moving dynamic debris; if it is empty, the cell becomes empty.
- `MODE_RETURN` saves each static cell's stored pressure and integrity in the body's cell state and leaves the cell in place at rest with its integrity; covered dynamic cells remain dynamic.
- `MODE_CHANGE` reports and detaches a listed body cell only when its current material differs from the stored material and is not dynamic. Unchanged or dynamic cells remain attached.

`MODE_CLEAR` and `MODE_RETURN` are queued through `rasterize_rigid_bodies`, which submits without waiting. Pending cell writes are submitted first. In the rigid-body submission, `release` runs for returned and cleared tiles before `rasterize`, so moving bodies do not block themselves.

### Entry point behaviour

- `rasterize` skips cells holding other static matter. It claims `rigid_body_cells` with compare-exchange, so the first invocation to successfully claim a contested cell wins; the shader does not impose item-order priority. Rigid-body tile cells use a 16-byte record of material, color, radiance, and padding. Rasterization writes those values, velocity, and the cell state's stored pressure and integrity only into empty cells; dynamic cells stay covered. It then saves the tile's mask as `previous_cells`.
- `write_cells` takes `[x, y, new_material, expected_material]` records. It writes only if the cell still holds the expected material and has no rigid-body cell. It zeroes kinematics and stored pressure, zeroes color and radiance when the cell is emptied, and wakes the tile.
- `read_cells` returns each cell in two `vec4<u32>` values; the second value carries radiance in `.z`, alongside stored pressure bits and integrity.
- `copy_states` takes `[old_cell, new_cell]` rigid-body tile-cell index pairs and copies the cell state, so a split body's new tiles keep their stored pressure and integrity.
- `read_changed_cells` runs `release` in `MODE_CHANGE` and returns rigid-body tile-cell indices (`tile_index * 64 + bit`), not coordinates. It returns at most 65,536 cells with an incomplete flag; excess cells remain attached for a later read.

Rigid-body cells keep their stored pressure and integrity in the cell state buffer while they are not rasterized. Once rasterized, they hold it in the scene's gas and kinematics buffers like static cells, where the pressure pass updates it; see [pressure simulation](@/internals/simulation/pressure.md#solid-integrity-and-damage).

## Readback

`request_cell_reads` dispatches `read_cells` for the detached groups that fit in one 65,536-cell read and copies the results into a three-slot `AcceleratorReadback` without waiting; groups that do not fit wait for a later tick. `read_cell_reads` returns them during the next collision tick, waiting only for their own submission. `copy_cell_states` submits `copy_states` without a readback. `read_changed_cells` copies its results to a staging buffer and waits for the mapping. `read_changed_cells` resets the changed-cell count, submits a copy, and waits for the mapping, even for an empty tile list; only its `release` dispatch is skipped. Rasterization and ordinary return/clear releases update the scene buffers without readback. `read_changed_cells` uses `release` in `MODE_CHANGE` and reads back the changed-cell list.
