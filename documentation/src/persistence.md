# Saving, Loading, And Streaming

`SceneData` is the filesystem-backed source of materials and chunks. Load an
existing directory, then create a scene:

```rust
let data: SceneData = SceneData::load("save/world".into())?;
let scene: Scene = Scene::load_with_generator(
    &accelerator, configuration, data, generator,
)?;
```

Use `SceneData::new_temporary(registry)` for a disposable scene, as the template
does. Its temporary directory is removed after the last `SceneData` owner is
dropped. `read_chunk` and `write_chunk` are the explicit chunk persistence
boundary.

Scenes stream a resident tile buffer around the requested area (normally the
possessed pawn). `area_buffered`, `is_position_resident`, `tiles_download`, and
`tiles_upload` are the useful game-facing controls. Tile, fluid, gas, and
rigid-body state can cross the resident boundary; gameplay should treat
non-resident world state as unavailable until streamed.

Stable actor identifiers and persisted rigid-body identities survive ordinary
streaming. Fluid particles and gas cells are serialized as dormant records;
cellular tile state is downloaded into chunk data; rigid bodies are gathered
from their body-local authority into owner files. Render views, collision
snapshots, GPU buckets, and other derived buffers do not survive as authorities
and are reconstructed on restoration.

Scene data is world persistence, not a player-profile database. Save scores,
inventory, and other game-owned progress in a separate game save format.

For ordinary games, let `Scene::update` manage streaming. Only use the
upload/download futures for tools, custom persistence, or deliberate world
editing.
