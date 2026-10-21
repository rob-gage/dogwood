+++
title = "Scene renderer"
description = "Scene graphics data, letterboxed viewport calculation, and render-pass ordering."
weight = 20
[extra]
sources = ["engine_graphics/src/renders/scene_renderer_fluid.wgsl", "engine_graphics/src/renders/scene_renderer_fluid_pass.rs", "engine_graphics/src/renders/scene_renderer.rs", "engine_graphics/src/renders/scene_renderer_sprites_pass.rs", "engine_graphics/src/renders/scene_renderer_cellular_pass.rs", "engine_graphics/src/world/scenes", "engine_world/src/scenes/scene.rs", "engine/src/lib.rs", "engine_world/src/scenes/cellular_tiles/mod.rs"]
+++

## Scene graphics input

This page is for engine contributors. It covers how a scene becomes graphics data, how the renderer computes its viewport, and the order of the render passes.

`Scene::graphics` returns a borrowed `SceneGraphics` for a camera actor, or an `ActorError` if the camera components cannot be read. It does the following:

- Derives the visible width and height from the camera.
- Collects actor sprites relative to the camera position.
- Includes cellular and fluid buffers only when a streamed active tile region is available.

Sprite frames outside a sheet are skipped. Sheet pixel dimensions are converted to tile units using `CELLULAR_TILE_SIZE`. Each valid sprite record applies the component's offsets and scales and carries its rotation, depth, and color.

`SceneGraphics` contains the visible dimensions in tiles, optional `SceneGraphicsCellular` and `SceneGraphicsFluid` records, and a `SceneGraphicsSprites` list. The cellular data holds:

- the GPU material-ID and packed-color buffers;
- the active region's camera-relative origin and dimensions in tiles;
- the ring offsets and cells per tile.

The fluid record borrows the mass/momentum raster and material properties with the same region/ring placement; see [fluid rendering](@/internals/rendering/fluid-pass.md).

See {{ <rustdoc path="engine_graphics/world/scenes/struct.SceneGraphics.html" text="the scene graphics API" /> }} and the {{ <rustdoc path="engine/scenes/struct.Scene.html" text="Scene API" /> }}.

## Viewport

{{ <rustdoc path="engine_graphics/renders/struct.SceneRenderer.html" text="SceneRenderer" /> }} reads the target texture's dimensions and format in `render`. When both visible dimensions are positive, it computes `pixels_per_tile` as the smaller of target width / visible width and target height / visible height.

It rounds the resulting viewport dimensions, caps them at the texture dimensions, and centers the viewport. The unused bars remain black because the render pass clears the full target to opaque black.

## Pass order

The renderer prepares passes only when the viewport has nonzero dimensions. Cellular rendering also requires a present cellular record with nonzero region width, region height, and tile size. Fluid rendering requires the same nonzero viewport and region dimensions. It classifies and shades fluid cells in two compute dispatches before opening one render pass with no depth attachment and draws in this order:

1. Sprites with depth `i32::MIN..=-1`.
2. Fluid raster, when resident fluid data is available.
3. Cellular tiles, when resident cellular data is available.
4. Sprites with depth `0..=i32::MAX`.

Each pass uses premultiplied-alpha blending. `SceneRenderer` finishes its command encoder and submits it to the accelerator queue itself. This is a scene-only render: the UI overlay is a separate stage described in [User interface rendering](@/internals/rendering/user-interface.md).

## Related pages

- [Cellular render pass](@/internals/rendering/cellular-pass.md): cellular buffer layout and draw setup.
- [Render shaders](@/internals/shaders/render-shaders.md): pixel-to-cell mapping and sprite vertex math.
- [Sprite render pass](@/internals/rendering/sprite-pass.md): depth sorting, instance packing, and texture batching.

- [Fluid render pass](@/internals/rendering/fluid-pass.md): cell classification, shading, and blending.
