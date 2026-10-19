+++
title = "Material table compilation"
description = "How MaterialRegistry::compile builds dynamic-cell properties and cellular albedo frames from registered materials."
weight = 20
[extra]
sources = ["engine_world/src/materials", "engine_graphics/src/sprites", "engine_graphics/src/world/materials/material_graphics_cellular_static.rs", "engine_graphics/src/world/materials/material_graphics_cellular_dynamic.rs", "engine_world/src/scenes/kinematics/kinematics_cellular_tile.wgsl", "engine_world/src/scenes/collisions/collision_rigid_body.rs"]
+++

This page is for engine contributors. It explains how `MaterialRegistry::compile` turns registered materials into the GPU property buffer and CPU albedo lookup data that the simulation uses.

`MaterialRegistry::register` assigns each material identifier. `MaterialRegistry::compile` consumes the populated registry and builds an immutable `MaterialTable` holding:

- the material definitions, for CPU lookups;
- a GPU buffer of dynamic-cell physics properties;
- CPU pixel arrays that assign colors to generated or edited cells.

Static and dynamic cellular materials use separate identifier-indexed albedo arrays. Fluid and gas definitions remain available through table lookups but do not participate in this cellular path. See the [materials API](@/external-api/materials.md) for registration and property types.

## Identifier indexing

`MaterialIdentifier` stores a two-bit form tag in its top bits and a 30-bit form-specific index below it. The tags are static `00`, dynamic `01`, fluid `10`, and gas `11`. `EMPTY` is static index zero; registered static materials begin at index one, while the other forms begin at zero. Simulation shaders test the tag; the kinematics shader masks the low 30 bits to index the dynamic property array. The encoding is defined in {{ <rustdoc path="engine/materials/struct.MaterialIdentifier.html" text="MaterialIdentifier" /> }}.

## Compilation path

Compilation collects static albedo frames starting with an empty slot, then appends dynamic frames. The albedo array is split after the reserved empty slot and all registered static entries.

### Property validation

Static and dynamic materials are validated against the same physical ranges:

- mass is positive and finite;
- friction is nonnegative and finite;
- restitution is in `[0, 1]`;
- for a static material, a debris identifier, if present, names a registered dynamic material.

### Dynamic property buffer

Each dynamic record is serialized as four little-endian `f32` values: mass, friction, restitution, and zero padding. The records form a `STORAGE | COPY_DST` buffer with a 16-byte stride. If no dynamic material exists, a zeroed 16-byte record keeps the binding nonempty.

### Albedo readback

For every cellular material frame, compilation validates that the repetition scales (cells per pixel) are finite and positive and that the sprite frame exists. It then:

1. Copies the frame texture to a readable buffer, padding each row to `wgpu::COPY_BYTES_PER_ROW_ALIGNMENT`.
2. Submits the copies, maps the buffers, and waits for the device.
3. Packs the unpadded RGBA bytes into `u32` pixels.

Repetition width and height are the frame's pixel dimensions multiplied by the configured cells-per-pixel scales.

If property or frame validation, or albedo readback, fails, compilation returns `MaterialError::Compilation(String)`.

## Color lookup

The color lookup samples by global cell coordinates. It offsets each cell center by half a cell and repeats from the scene origin with Euclidean modulo. It flips the vertical coordinate because sprite rows start at the top while cell rows increase upward.

Packed colors are little-endian RGBA. White returns the sampled albedo unchanged; other tints multiply RGB in linear light and alpha with integer normalized multiplication. If there is no albedo for the identifier, the supplied color is returned.

## GPU property consumers

The kinematics pass binds the dynamic property buffer as `array<vec4<f32>>` and indexes it with the dynamic identifier's low 30 bits. It uses mass, friction, and restitution in cell movement and collisions; see [cellular kinematics](@/internals/simulation/cellular-kinematics.md) and the [kinematics shader](@/internals/shaders/kinematics-shader.md). Static-cell mass, friction, and restitution are instead looked up on the CPU when calculating a detached [cellular rigid body's properties](@/internals/simulation/rigid-bodies.md).

> **Limitation:** `MaterialTable` currently creates only the dynamic cellular property buffer. Static, fluid, and gas property-buffer accessors return `None`; fluid and gas are not compiled into simulation records.

The albedo readback and dynamic buffer setup are implemented in {{ <rustdoc path="engine/materials/struct.MaterialTable.html" text="MaterialTable" /> }}.
