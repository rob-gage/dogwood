+++
title = "Generating worlds"
description = "Implement SceneGenerator to create CPU chunks when no saved chunk exists."
weight = 20
[extra]
sources = ["engine_world/src/scenes/scene_generator.rs", "engine_world/src/scenes/chunks/chunk.rs", "engine_world/src/scenes/chunks/chunk_data.rs", "engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/chunks/mod.rs", "engine_world/src/scenes/chunks/chunk_coordinates.rs", "engine_world/src/scenes/mod.rs", "template/src/template_scene_generator.rs", "engine/src/lib.rs", "engine_world/src/materials/material_identifier.rs", "engine_world/src/materials/material_table.rs"]
+++

## Implement `SceneGenerator`

A `SceneGenerator` creates a chunk when storage has no saved file for its coordinates. A chunk is 512x512 cells; see [coordinates and chunks](@/external-api/world-storage.md) for the coordinate types. The [scene API](@/external-api/scenes.md#generator-contract) documents the generator's runtime contract.

This template generator fills every chunk below chunk row zero with stone and leaves chunks at or above zero empty:

```rust
use engine::{
    materials::MaterialIdentifier,
    scenes::{chunks::{Chunk, ChunkCoordinates}, SceneGenerator},
};
use std::error::Error;

struct WorldGenerator { stone: MaterialIdentifier }

impl SceneGenerator for WorldGenerator {
    fn generate_chunk(
        &self,
        coordinates: ChunkCoordinates,
        _world_seed: u64,
    ) -> Result<Chunk, Box<dyn Error + Send + Sync>> {
        let mut chunk = Chunk::new(coordinates);
        if coordinates.y < 0 {
            chunk.fill(self.stone, u32::from_le_bytes([255, 255, 255, 255]), u32::MAX);
        }
        Ok(chunk)
    }
}
```

`WorldGenerator::stone` is the identifier returned by material registration. Pass the generator to `Scene::new` together with the compiled `MaterialTable` from [Defining materials](@/examples/materials-and-sprites.md).

### Build a chunk

`Chunk::new` creates empty cell storage at the supplied coordinates. `fill` assigns one material identifier, packed color, and packed radiance to every cell and clears kinematics. Pass `u32::MAX` to use each material's default radiance.

To vary a chunk, replace one cellular tile's arrays with `set_cellular_tile_material_identifiers`, `set_cellular_tile_colors`, `set_cellular_tile_radiances`, or `set_cellular_tile_kinematics`. There is no public single-cell setter; scene edits can fill one cell with `Edit::fill_cell`.

### Colors

A color is a `u32` built with `u32::from_le_bytes([r, g, b, a])`. After generation, Dogwood runs each nonempty cell through `MaterialTable::cellular_color`. For materials with albedo, the color tints that albedo, so white (255, 255, 255, 255) leaves it unchanged.

## Seed, errors, and saved chunks

The seed is stable input to each generation call, but Dogwood does not enforce deterministic output. Use the seed and coordinates in your algorithm if repeated generation should produce the same world.

`ChunkData` stores the root path for saved chunk files; `save` creates the directory if needed. On load:

- Streaming loads a matching saved chunk when one exists, and invokes the generator when the file is absent.
- If a saved file exists but cannot be read or decoded, streaming reports a chunk load error; it does not fall back to generation.
- Generated chunks start clean. Scene saving writes only dirty chunks, so an unchanged generated chunk may be unloaded and generated again later.
- A saved chunk takes precedence on later loads; deleting its file lets the generator supply it again.

See [coordinates and chunks](@/external-api/world-storage.md) for storage types and [the scene API](@/external-api/scenes.md) for the generator contract and scene lifecycle.

{{ <rustdoc path="engine/scenes/trait.SceneGenerator.html" text="SceneGenerator" /> }} · {{ <rustdoc path="engine/scenes/chunks/struct.Chunk.html" text="Chunk" /> }} · {{ <rustdoc path="engine/scenes/chunks/struct.ChunkData.html" text="ChunkData" /> }}
