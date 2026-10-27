+++
title = "Material table compilation"
description = "How MaterialRegistry::compile builds material, thermal property, and cellular albedo data."
weight = 20
[extra]
sources = ["engine_world/src/materials", "engine_graphics/src/sprites", "engine_graphics/src/world/materials/material_graphics_gas.rs", "engine_graphics/src/world/materials/material_graphics_gas.rs", "engine_graphics/src/world/materials/material_graphics_cellular_static.rs", "engine_graphics/src/world/materials/material_graphics_cellular_dynamic.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile.wgsl", "engine_world/src/scenes/collisions/collision_rigid_body.rs", "engine_world/src/scenes/collisions/collision.rs"]
+++

This page is for engine contributors. It explains how `MaterialRegistry::compile` turns registered materials into the GPU property buffer and CPU albedo lookup data that the simulation uses.

`MaterialRegistry` stores named definitions with unresolved dependency names. `MaterialRegistry::compile` assigns identifiers in registration order within each form, resolves dependencies, and builds an immutable `MaterialTable` holding:

- the named material definitions, for CPU lookups;
- compiled name lookups and resolved debris identifiers;
- GPU buffers of static-cell, dynamic-cell, fluid, gas, and thermal properties;
- transition colors and properties;
- CPU pixel arrays that assign colors to generated or edited cells.

Static and dynamic cellular materials use separate identifier-indexed albedo arrays. Fluid and gas definitions remain available through table lookups but do not participate in this cellular path. See the [materials API](@/external-api/materials.md) for registration and property types.

## Identifier indexing

`MaterialIdentifier` stores a two-bit form tag in its top bits and a 30-bit form-specific index below it. The tags are static `00`, dynamic `01`, fluid `10`, and gas `11`. `EMPTY` is static index zero; registered static materials begin at index one, while the other forms begin at zero. Games look up handles by name from the compiled table. Simulation shaders test the tag; the cellular tile shader masks the low 30 bits to index the dynamic property array. The encoding is defined in {{ <rustdoc path="engine/materials/struct.MaterialIdentifier.html" text="MaterialIdentifier" /> }}.

## Compilation path

Compilation collects static albedo frames starting with an empty slot, then appends dynamic frames. The albedo array is split after the reserved empty slot and all registered static entries.

### Property validation

Static and dynamic materials are validated against the same physical ranges:

- mass is positive and finite;
- friction is nonnegative and finite;
- restitution is in `[0, 1]`;
- static strength is positive and not NaN (positive infinity is allowed), and absorption is finite in `[0, 1]`;
- for a static material, a debris name, if present, resolves to a registered dynamic material.

### Dynamic property buffer

Each dynamic record is serialized as four little-endian `f32` values: mass, friction, restitution, and zero padding. The records form a `STORAGE | COPY_DST` buffer with a 16-byte stride. If no dynamic material exists, a zeroed 16-byte record keeps the binding nonempty.

### Static property buffer

Each static record is four little-endian `f32` values: mass, absorption, strength, and the debris material identifier stored in the fourth word's bits (zero when absent). The `STORAGE | COPY_DST` buffer has a 16-byte stride. Index zero is the empty material and contains zeros.

### Fluid property buffer

Each fluid record contains seven little-endian `f32` values (density, viscosity, cohesion, artificial pressure, drag, friction, restitution), followed by a packed RGBA `u32` color. The `STORAGE | COPY_DST` buffer has a 32-byte stride. With no fluid materials, a zeroed 32-byte record keeps the binding nonempty.

Compilation validates positive finite density, viscosity and restitution in `[0, 1]`, and finite nonnegative cohesion, artificial pressure, drag, and friction.

### Gas property buffer

Each gas record contains three little-endian `f32` values (mass, diffusivity, cohesion), followed by a packed RGBA `u32` color. The `STORAGE | COPY_DST` buffer has a 16-byte stride. With no gas materials, a zeroed 16-byte record keeps the binding nonempty.

Compilation validates positive finite mass, diffusivity in `[0, 0.4]`, and cohesion in `[0, 1]`. It allows at most 255 gas materials: gas indices are stored in eight bits, and the pressure simulation also stores each index plus one in eight bits.

### Thermal properties and transitions

Thermal records use sixteen little-endian `u32` words per thermal index, with indices assigned in
four consecutive groups ordered static, dynamic, fluid, and gas. Each record stores heat capacity,
conduction, hot and cold transition temperatures and latent heat, resolved product identifiers
and amounts, and transition flags. Static index zero is reserved for empty. `thermal_offsets()` returns the first
thermal index for each form. A parallel buffer stores seven `vec4<f32>` values for each hot or cold
transition's color transform.

The compiled table retains transition temperatures, latent heat, product identifiers and amounts,
and color transforms. These properties are not yet used to simulate transitions.

> **Not implemented:** Material transitions are not yet simulated.

### Albedo readback

For every cellular material frame, compilation validates that the repetition scales (cells per pixel) are finite and positive and that the sprite frame exists. It then:

1. Copies the frame texture to a readable buffer, padding each row to `wgpu::COPY_BYTES_PER_ROW_ALIGNMENT`.
2. Submits the copies, maps the buffers, and waits for the device.
3. Packs the unpadded RGBA bytes into `u32` pixels.

Repetition width and height are the frame's pixel dimensions multiplied by the configured cells-per-pixel scales.

Compilation reports unknown dependency names as `MaterialError::UnknownMaterial`, non-dynamic debris as `InvalidDebris`, and invalid transitions as `InvalidTransition`. Other property or frame validation and albedo readback failures return `MaterialError::Compilation(String)`.

## Color lookup

The color lookup samples by global cell coordinates. It offsets each cell center by half a cell and repeats from the scene origin with Euclidean modulo. It flips the vertical coordinate because sprite rows start at the top while cell rows increase upward.

Packed colors are little-endian RGBA. White returns the sampled albedo unchanged; other tints multiply RGB in linear light and alpha with integer normalized multiplication. If there is no albedo for the identifier, the supplied color is returned.

## GPU property consumers

The cellular tile pass binds the dynamic property buffer as `array<vec4<f32>>` and indexes it with the dynamic identifier's low 30 bits. It uses mass, friction, and restitution in cell movement and collisions; see [cellular kinematics](@/internals/simulation/cellular-kinematics.md) and the [cellular tile shader](@/internals/shaders/cellular-tile-shader.md). Static-cell mass, friction, and restitution are instead looked up on the CPU when calculating a detached [cellular rigid body's properties](@/internals/simulation/rigid-bodies.md).

`MaterialTable` creates static cellular, dynamic cellular, fluid, and gas property buffers. The fluid solver and cellular kinematics consume the fluid buffer for particle constraints and fluid/body coupling. The fluid renderer reads density and packed color from the same buffer. The [pressure simulation](@/internals/simulation/pressure.md) reads mass, diffusivity, and cohesion from the gas buffer, and [gas rendering](@/internals/rendering/gas-pass.md) reads its packed color.

The albedo readback and dynamic buffer setup are implemented in {{ <rustdoc path="engine/materials/struct.MaterialTable.html" text="MaterialTable" /> }}.
