+++
title = "Graphics"
description = "Sprite sheets, borrowed scene graphics, and the engine_graphics scene renderer API."
weight = 90
[extra]
sources = ["engine_graphics/src/lib.rs", "engine_graphics/src/sprites", "engine_graphics/src/renders/mod.rs", "engine_graphics/src/renders/scene_renderer.rs", "engine_graphics/src/world/scenes", "engine/src/lib.rs"]
+++

Game code generally needs only `SpriteSheet`. Integrators providing their own render loop can use `SceneRenderer` and `SceneGraphics`; the engine also uses them internally. These types live in the `engine_graphics` crate and are not re-exported from the `engine` facade.

## SpriteSheet

`SpriteSheet` is re-exported from the facade as `engine::SpriteSheet` (defined in `engine_graphics::sprites`). Constructors upload straight-alpha sRGB RGBA8 pixels into a GPU texture divided into equal frames, numbered row-major from the top-left.

| Constructor | Input |
|---|---|
| `from_png(accelerator, bytes, columns, rows)` | Encoded PNG bytes. |
| `from_tga(accelerator, bytes, columns, rows)` | Encoded TGA bytes. |
| `from_rgba(accelerator, pixels, width, height, columns, rows)` | Raw RGBA8 bytes and texture/frame grid dimensions. |
| `from_color(accelerator, red, green, blue, alpha)` | One RGBA color as a single frame. |

### Errors

`from_rgba` fails with `SpriteSheetError::InvalidDimensions` when:

- any dimension or grid count is zero;
- the texture exceeds the device limit;
- the dimensions do not divide evenly into the frame grid;
- the byte count is not exactly `width * height * 4`.

`from_png` and `from_tga` return `SpriteSheetError::Decode(image::ImageError)` for decode failures, or `InvalidDimensions` for an invalid decoded frame grid or device dimensions.

### Accessors

- `width()` and `height()` return per-frame pixel dimensions.
- `frame_count()` returns columns times rows.
- `frame_texture_region(frame)` returns normalized left, top, right, and bottom coordinates, or `None` when the index is out of range.
- `texture_view()` borrows the GPU texture view.

## Scene graphics and renderer

`SceneGraphics`, `SceneGraphicsCellular`, `SceneGraphicsFluid`, `SceneGraphicsGas`, and `SceneGraphicsSprites` (in `engine_graphics::world::scenes`) are borrowed descriptions of one frame:

- the visible tile dimensions;
- optional resident cellular buffers and ring placement;
- optional fluid raster and material-property buffers with ring placement;
- optional gas cell, material identifier, and gas material-property buffers with ring placement;
- sprite texture views with position, size, rotation, depth, texture region, and linear color.

The actor sprite tuple that feeds the sprite data is documented on [actor components](@/external-api/actor-components.md).

`SceneRenderer` is in `engine_graphics::renders`. Create it with `SceneRenderer::new(accelerator)`, then call `render(accelerator, texture_view, &graphics)` on a mutable renderer. `render` submits its command buffer to `accelerator.queue()`.

The target must be single-sample.

Each frame is drawn in this order, into a centered letterboxed viewport:

```text
[negative-depth sprites] -> [fluid] -> [gas] -> [cellular content] -> [nonnegative-depth sprites]
```

Cellular rendering is skipped when there is no resident region or the viewport is empty. See [scene renderer](@/internals/rendering/scene-renderer.md) for the render passes.

{{ <rustdoc path="engine_graphics/sprites/struct.SpriteSheet.html" text="SpriteSheet" /> }} ·
{{ <rustdoc path="engine_graphics/sprites/enum.SpriteSheetError.html" text="SpriteSheetError" /> }} ·
{{ <rustdoc path="engine_graphics/world/scenes/struct.SceneGraphics.html" text="SceneGraphics" /> }} ·
{{ <rustdoc path="engine_graphics/renders/struct.SceneRenderer.html" text="SceneRenderer" /> }}
