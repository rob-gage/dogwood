# Rendering

The runtime renders a scene and then UI into the current WGPU surface texture.
`SceneRenderer` owns a format-dependent pipeline, bind-group layout, uniform
buffer, and bind group. It asks `Scene::graphics()` for borrowed scene resources
and encodes one fullscreen-triangle render pass whose fragment shader samples
resident simulation buffers.

The scene render view includes persistent cellular material IDs and appearances,
rasterized rigid cells, derived fluid material/coverage/velocity, gas
concentrations, retained cellular pressure, ring origin/offsets, and actor
graphics. Material appearance and solver/render properties come from immutable
GPU tables built from the CPU `MaterialRegistry`.

Rendering consumes simulation state; it does not make simulation decisions. The
scene buffers remain owned by `Scene`, and `SceneGraphics` borrows them only for
command encoding. Ring origin and offsets are passed to the shader so logical
world coordinates map to the current physical buffer slots.

The application computes camera position and viewport in tile units. It can
also convert surface pixels back to world positions for editor pointer tools.
`UserInterfaceRenderer` loads the latest egui output and draws it with load
operation over the scene target. Actor graphics are a lightweight CPU-to-GPU
render list, interpolated between fixed ticks.

### Relevant implementation

- `engine/src/renders/scene_renderer.rs` — scene pipeline, bindings, uniforms,
  and render pass.
- `engine/src/renders/scene.wgsl` — scene sampling and fragment rendering.
- `engine_graphics/src/scene_graphics.rs` — borrowed scene resource contract.
- `engine_graphics/src/material_graphics.rs` — packed material appearance and
  physical render properties.
- `engine_graphics/src/camera.rs` — camera view configuration.
- `engine_physics/src/scenes/scene_graphics.rs` — builds the scene render view.
- `engine/src/games/game_application.rs` — viewport, camera follow, and frame
  presentation.
