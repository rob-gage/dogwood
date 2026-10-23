+++
title = "Editing and saving the world"
description = "Build cell edits, submit them to a scene, and track save completion."
weight = 50
[extra]
sources = [
    "engine_world/src/scenes/edits",
    "engine_world/src/scenes/scene.rs",
    "engine_world/src/scenes/chunks/chunk_streaming_state.rs",
    "engine_world/src/materials/material_table.rs",
    "engine/src/lib.rs",
    "engine_world/src/materials/material_identifier.rs",
    "engine_world/src/scenes/cellular_tiles/cell_coordinates.rs",
    "engine_world/src/scenes/scene_position.rs",
]
+++

## Build an edit

An `Edit` groups cells by cellular tile (an 8x8 block of cells). Each filled cell carries a material identifier and a packed color. Create one with `Edit::new`, then add cells:

- `fill_cell`: sets one cell, replacing an earlier fill of the same cell in the same edit.
- `fill_segment`: takes two scene positions and a radius in tiles, and fills cells whose centers are within the radius of the segment. A negative or non-finite radius produces no fill and returns `Ok(())`.

The color tints the albedo; materials without an albedo use it directly.

```rust
use engine::materials::MaterialIdentifier;
use engine::scenes::{
    cellular_tiles::CellCoordinates,
    edits::{Edit, EditError, EditMode},
    ScenePosition,
};

fn build_edit(material_identifier: MaterialIdentifier) -> Result<Edit, EditError> {
    let mut edit = Edit::new(EditMode::Replace);
    let color = u32::from_le_bytes([180, 120, 60, 255]);
    edit.fill_cell(CellCoordinates { x: 12, y: 7 }, material_identifier, color);
    edit.fill_segment(
        ScenePosition { tile_x: 2, tile_y: 1, offset_x: 0, offset_y: 0 },
        ScenePosition { tile_x: 3, tile_y: 1, offset_x: 0, offset_y: 0 },
        0.25,
        material_identifier,
        color,
    )?;
    Ok(edit)
}
```

The `EditMode` chosen in `Edit::new` controls what an edit overwrites:

- `EditMode::Insert`: fills only empty cells and skips cells overlapped by colliders in the edit collision group.
- `EditMode::Replace`: fills every targeted cell and ignores colliders.

To erase, use `Replace` with `MaterialIdentifier::EMPTY` and color `0`.

## Submit edits to a scene

`Scene::edit(edit)` accepts an edit only when every targeted tile is in the active region. Before the first region becomes resident, the active region is `None`, so edits are rejected. On the next `Scene::stream`, Dogwood applies queued edits in request order before moving targeted tiles: a later `Replace` overwrites earlier fills, while `Insert` fills only cells left writable by earlier requests.

The active region is the rectangle of tiles currently resident on the GPU and simulated (see [core concepts](@/getting-started/core-concepts.md)).

See [actors, cameras, and pawns](@/examples/actors-and-pawns.md) for how a game supplies the scene and camera used for streaming.

### Validation

Validation is all-or-nothing: if any check fails, the entire edit is rejected and none of it is queued. Failures are:

- `CellularTileInactive`: a targeted tile is outside the active region.
- `MaterialUnregistered`: a filled material is not registered.
- `CapacityReached`: the request exceeds capacity. `fill_segment` also returns it, before filling, when its bounding rectangle spans more than 4096 tiles.

The complete API is in [collisions and edits](@/external-api/collisions-and-edits.md).

## Save and check completion

Call `Scene::save()` to request persistence of the pending edits and resident tiles without unloading them. Save requests progress during later `Scene::stream()` calls; keep streaming while polling `is_saved()`. A `false` result means at least one requested save has not completed successfully; it can stay `false` after a chunk write fails. Request a save explicitly when the game needs to report completion before the scene is dropped.

On drop, Dogwood applies queued edits, returns active tiles, and saves dirty chunks. Drop panics if applying edits or returning tiles fails, or if a dirty chunk still cannot be saved after three attempts.

The [editor](@/examples/using-the-editor.md) uses this same API for brush strokes.

{{ <rustdoc path="engine/scenes/edits/struct.Edit.html" text="Edit" /> }} · {{ <rustdoc path="engine/scenes/edits/enum.EditMode.html" text="EditMode" /> }} · {{ <rustdoc path="engine/scenes/struct.Scene.html" text="Scene" /> }}
