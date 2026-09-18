# World cells and edits

One tile contains 8×8 cells. Use `CellCoordinates::from_world_position` to
convert tile-space positions and `tile_coordinates`/`local_tile_coordinates` to
navigate.

Queue gameplay edits on a mutable scene:

```rust
let mut edits = SceneEditBatch::new();
edits.erase(vec![CellCoordinates { x: 10, y: 4 }]);
edits.thermal(vec![CellCoordinates { x: 11, y: 4 }], 25.0);
scene.queue_edits(edits);
```

`place_material`/`place_cells` author cells, `erase` removes their material,
`destroy_cells` removes every representation at those cells, and
`place_rigid_body` creates body-local cellular matter. Edits are applied by the
next scene update, so batch them rather than mutating one cell at a time.

`Scene::tile_at` is a lightweight resident-tile query. It returns `None` for
non-resident coordinates. For a CPU snapshot of resident tiles, use
`Scene::tiles_download(area).await`; upload modified tile data with
`tiles_upload(area)` when you intentionally operate at that lower level.
