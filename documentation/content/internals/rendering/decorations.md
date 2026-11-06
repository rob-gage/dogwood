+++
title = "Chunk decorations"
description = "Chunk art pieces, atlas ramps, and their renderer motion field."
weight = 55
[extra]
sources = ["engine_world/src/scenes/chunks/chunk_decoration.rs", "engine_world/src/scenes/chunks/chunk_background.rs", "engine_world/src/scenes/chunks/chunk_streaming_state.rs", "engine_world/src/scenes/chunks/mod.rs", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/scene_backgrounds.rs", "engine_graphics/src/world/scenes/scene_graphics_decorations.rs", "engine_graphics/src/renders/scene_renderer.rs", "engine_graphics/src/renders/scene_renderer_decorations_pass.rs", "engine_graphics/src/renders/scene_renderer_decoration_field.wgsl", "engine_graphics/src/renders/scene_renderer_decorations.wgsl", "engine_graphics/src/world/scenes/scene_graphics_decorations.rs"]
+++
Chunk backgrounds may carry `ChunkDecoration` pieces in addition to background sprites. A piece locates a grayscale art region in the decoration atlas, selects a color ramp row, and stores its placement and motion settings. `ChunkBackground.decorations` carries the generated pieces, and `Scene::set_decoration_atlas` supplies their atlas. The atlas includes ramps; art channels select ramp colors, and the green channel can select the following ramp row. Decoration data is generated for chunks and uploaded while they are resident; see [chunk residency](@/internals/world-streaming/chunk-residency.md).

One art pixel maps to one world cell. The renderer computes a motion field in `scene_renderer_decoration_field.wgsl`: a spring at two texels per tile over the visible part of the active region, pushed by gas flow and by the bounds and velocities of nearby actors. Each piece moves rigidly, following where the field carries its pivot and far end, more strongly the farther they are from its root. The support cell only decides whether the piece is drawn. The `SceneRendererDecorationsPass` dispatches this compute work before the surface field and before opening the scene render pass.

## Frame order

Within scene rendering, lighting is computed first, then the decoration motion field, then the optional material surface field. Fluid shading follows those compute passes. The render pass draws background and parallax batches before negative-depth sprites, then fluid, gas, and cellular tiles, followed by foreground sprites and overlays. See [scene renderer pass order](@/internals/rendering/scene-renderer.md#pass-order).

Chunk background generation runs on the chunk streaming worker after chunk generation has produced a new chunk, or after a stored chunk has loaded. It prepares transient background images and decoration pieces for resident chunks; rendering consumes the uploaded results on a subsequent redraw.
