+++
title = "Sprite render pass"
description = "Sprite instance packing, depth ordering, texture batches, and sprite sheet uploads."
weight = 40
[extra]
sources = ["engine_graphics/src/renders/scene_renderer_sprites_pass.rs", "engine_graphics/src/sprites", "engine_graphics/src/renders/scene_renderer_sprites.wgsl"]
+++

## Instance data and ordering

This page is for engine contributors. It covers how sprites are sorted, packed into instances, batched by texture, and drawn, plus the sprite sheet texture format.

### Ordering

The scene renderer creates two `SceneRendererSpritesPass` instances: one selects depths below zero and the other selects zero or above. Each pass filters the scene sprite list, then sorts selected indices by ascending depth. Rust's stable `sort_by_key` retains source order for sprites at the same depth.

### Instance layout

Each sprite becomes one 56-byte instance record with these fields, in order:

- position and size (`x`, `y`, `width`, `height`);
- rotation and a padding float;
- normalized texture edges (left, top, right, bottom);
- linear tint color (RGBA).

The pipeline reads four attributes at byte offsets 0, 16, 24, and 40. It draws four vertices per instance as a triangle strip, with no depth attachment; the pass order determines composition. The fragment shader premultiplies tinted RGB by alpha, and the pipeline uses premultiplied-alpha blending.

### Buffer growth

The instance buffer starts at `256 * 56` bytes. When the packed data exceeds that capacity, the pass replaces it with a buffer sized to the next power of two that can hold the frame's instances. The enlarged capacity is retained for later preparations and does not shrink.

The per-view uniform contains viewport width and height in pixels and `pixels_per_tile`.

## Texture binding and batching

The first bind group holds the uniform and a nearest-filter sampler clamped to texture edges. A second layout binds one 2D filterable float texture view.

- Preparation groups only consecutive sorted instances that use the same texture view, producing a draw range per contiguous batch.
- Texture bind groups are cached by view for the current set of batches; unused entries are dropped during preparation.
- The draw loop sets each batch's texture group and issues an instanced draw.

## Sprite sheets

`SpriteSheet` stores a single GPU view divided into equal frames, numbered row-major from the top-left. Uploads create a one-mip, single-sample `Rgba8UnormSrgb` 2D texture with `TEXTURE_BINDING`, `COPY_DST`, and `COPY_SRC` usage from straight-alpha sRGB RGBA8 pixels. Constructors and their errors are documented in [Graphics](@/external-api/graphics.md#spritesheet).

The sheet exposes frame dimensions, count, texture view, and normalized frame region. The scene adapter uses these values to make render records; invalid frame indices are omitted. See [Scene renderer](@/internals/rendering/scene-renderer.md) for the adapter and pass order, and [Render shaders](@/internals/shaders/render-shaders.md) for vertex transformation and texture sampling.

Parallax uses a separate `SceneRendererSpritesPass` instance before gameplay. The renderer first sorts planes by descending physical depth and ascending sprite integer order, then submits their projected quads with equal batching depth so stable sprite batching preserves that order. See [parallax backgrounds](@/external-api/scenes.md#parallax-backgrounds).
