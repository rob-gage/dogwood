# Saving, loading, and streaming

`SceneData` is the filesystem-backed source of materials and chunks. Load an
existing directory, then create a scene:

```rust
let data = SceneData::load("save/world".into())?;
let scene = Scene::load_with_generator(
    &accelerator, configuration, data, generator,
)?;
```

Use `SceneData::new_temporary(registry)` for a disposable scene, as the template
does. `read_chunk` and `write_chunk` are the explicit chunk persistence
boundary.

Scenes stream a resident tile buffer around the requested area (normally the
possessed pawn). `area_buffered`, `is_position_resident`, `tiles_download`, and
`tiles_upload` are the useful game-facing controls. Tile, fluid, gas, and
rigid-body state can cross the resident boundary; gameplay should treat
non-resident world state as unavailable until streamed.

For ordinary games, let `Scene::update` manage streaming. Only use the
upload/download futures for tools, custom persistence, or deliberate world
editing.
