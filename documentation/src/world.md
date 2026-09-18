# World Cells And Edits

One tile contains 8 by 8 cells. Convert a tile-space position with
`CellCoordinates::from_world_position`; use `tile_coordinates` and
`local_tile_coordinates` to navigate the result. Scene cell queries and edits
only operate on resident state unless an API explicitly downloads a region.

## Querying Resident Cells

`Scene::tile_at` is a lightweight resident-tile query and returns `None` for a
non-resident coordinate. Use `is_position_resident` before gameplay logic that
depends on a moving target. For a CPU snapshot of a resident `TileArea`, use
`tiles_download`; this is an asynchronous lower-level tool and should not be
needed for ordinary per-frame gameplay.

## Queued Edits

Gameplay should build one `SceneEditBatch` and queue it on the scene. The scene
applies batches at an update boundary, preserving ordering relative to the
simulation stages. Batch related changes rather than submitting one request
per cell.

```rust
use dogwood_engine::physics::scenes::SceneEditBatch;
use dogwood_engine::physics::tiles::CellCoordinates;

let mut scene_edit_batch: SceneEditBatch = SceneEditBatch::new();
let thermal_cells: Vec<CellCoordinates> = vec![
    CellCoordinates { x: 10, y: 4 },
    CellCoordinates { x: 11, y: 4 },
];
scene_edit_batch.erase(vec![CellCoordinates { x: 10, y: 4 }]);
scene_edit_batch.thermal(thermal_cells, 25.0);
scene.queue_edits(scene_edit_batch);
```

## Placement And Explicit State

`place_material` places one material and appearance into a list of cells.
`place_cells` accepts explicit `SceneEditCellPlacement` records when each cell
needs different material or appearance state. `place_rigid_body` creates one
atomic body-local cellular object instead of terrain cells.

## Erase Versus Destroy

`erase` removes the ordinary material representation at each target cell. It
is useful for gameplay editing where other material-form state should not be
treated as a universal destruction event. `destroy_cells` removes every
material representation occupying those world cells, including representations
that would otherwise be routed through fluid, gas, or rigid-body handling. Use
it for an authoritative blast or cleanup effect. Both operations are queued;
their result becomes visible when the next scene update applies the batch.

## Thermal Edits And Batching

`thermal` applies a signed temperature delta through the thermal authority. It
does not directly mutate a CPU `TileData` copy. Combine thermal, placement,
erase, and destruction requests in a batch when the gameplay event is one
logical action. The engine routes the request to the appropriate resident
representation and later persists it when the region leaves residency.

For simulation ownership and the GPU/CPU boundary, see the internals pages on
[Scene Model](../internals/src/scenes/scene.md) and [Cellular
Simulation](../internals/src/simulation/cellulars.md).
