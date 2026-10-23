+++
title = "Gas simulation"
description = "How gas is stored, moved, displaced, diffused, and kept active in a scene's cellular tiles."
weight = 78
[extra]
sources = ["engine_world/src/scenes/gases/mod.rs", "engine_world/src/scenes/gases/gases.rs", "engine_world/src/scenes/gases/gas_cellular_tile_pass.rs", "engine_world/src/scenes/gases/gas_cellular_tile.wgsl", "engine_world/src/scenes/scene.rs", "engine_world/src/scenes/cellular_tiles/cellular_tile_buffers.rs", "engine_world/src/scenes/edits/edit_cellular_tile.wgsl", "engine_world/src/materials/material_table.rs"]
+++

Gas occupies open cells in the cellular tile ring. This page describes the simulation model; [gas shader](@/internals/shaders/gas-shader.md) documents GPU bindings and packed records. Tile storage and ring mapping are in [the tile ring](@/internals/world-streaming/tile-ring.md), and tile upload and return are in [tile streaming](@/internals/world-streaming/tile-streaming.md).

## Cell state and units

Each cell stores four 32-bit species slots, phantom air, flow through its right face, flow through its top face, and padding. A slot packs an 8-bit gas index in the high bits and a 24-bit amount in the low bits; zero amount means free. Gas indices are limited to 255 registered species. The largest slot amount is `GAS_AMOUNT_MAXIMUM` (`0xff_ffff`).

`GAS_AMOUNT_PER_ATMOSPHERE` is 65,536 amount units. A slot can therefore hold just under 256 atmospheres. Phantom air stores ambient gas separately from registered species, with its own amount and the scene's configured ambient mass per atmosphere. Scene ambient pressure and mass, including defaults and validation, are described in [scene settings](@/external-api/scenes.md).

Phantom air contributes to cell pressure and density alongside registered gases. The flow solver compares density with ambient density for buoyancy. When an edit places gas in an empty cell, it replaces phantom air with at least one atmosphere; emptying a cell leaves the configured ambient amount. A cellular edit removes phantom air but retains registered gas so it can be displaced. See the [edit shader](@/internals/shaders/edit-shader.md) for edit writes.

Each gas material contributes its registered mass, diffusivity, cohesion, and color to a GPU property record. Mass determines the species contribution to density. Diffusivity sets exchange with open neighbors, and cohesion sets the minimum amount in an open source before it can transfer to a cell that does not already hold that species. The gas renderer uses the color; see [gas rendering](@/internals/rendering/gas-pass.md).

## Tick order

`Scene::simulate` runs gas last, after fluid simulation, using the fluid raster buffer, the cellular tile actor buffer, and `cellular_tiles.fluid_actors().len()` as the actor count. `Gases` is created lazily when gas simulation first runs.

Each gas tick proceeds in this order:

1. `volume` visits every physical tile in the ring, measures open volume and actor drag for its cells, and wakes tiles whose measured values changed.
2. `collect` marks tiles active when they or one of their neighbors is awake. `clear` consumes the awake flags, and `prepare` writes an indirect dispatch with one workgroup per active tile.
3. Four `step` substeps update gas, alternating between the cellular tile gas buffer and a scratch buffer. The even substep count leaves the result in the cellular tile buffer.

The pass uses ring offsets to address physical storage while collecting active tiles in logical region order. See [ring mapping](@/internals/world-streaming/tile-ring.md#ring-mapping).

## Face flows and transfers

Each cell stores the flow through its right and top faces; neighboring cells share a face's signed flow. For active faces, each substep carries the previous flow along itself, adds a pressure response and bulk-viscosity response, applies vertical buoyancy, blends actor drag where present, then damps the result. Buoyancy uses the difference between gas density and ambient density, with scene gravity scaled by `GAS_BUOYANCY_SCALE`.

The stored flow is not a velocity: it is the signed amount flow through the cell face. The shader derives an approximate face velocity by dividing by mean pressure before sampling the advected flow field. Horizontal and vertical faces use cross-flow samples to carry flow along its direction.

The flow advection speed is bounded. Pressure response moves gas from high to low pressure; bulk viscosity damps differences in net outflow to reduce pressure waves. Flows below `FLOW_MINIMUM` are set to zero so settled gas can sleep. Flow toward closed cells is suppressed.

Outgoing flow fractions are computed per cell and scaled down together if their sum exceeds `BULK_FRACTION_MAXIMUM`. Transfers use integer fractions and `transfer()` rounds down with a split multiply that avoids overflowing a 32-bit product. The sender computes each face transfer from its own amount; the receiver recomputes the same value from the same prior state, so both sides apply identical integer amounts. This keeps transfers sender-limited and exactly balanced across each face.

Species diffuse between open neighboring cells at the diffusivity registered for that gas. Phantom air also diffuses, using `AMBIENT_DIFFUSIVITY`. A destination accepts a species it already holds or has admitted during the substep. Open-cell transfers also require the source cloud to meet that gas's cohesion threshold, unless the destination already holds it. A closed source can expel its gas without that threshold.

Advection and diffusion are separate contributions to a transfer fraction. Both use the same sender amount and face limit, so neither can withdraw more than the sender's stored amount. Diffusion applies only across open cells; it does not cross cellular matter or a closed fluid or actor cell.

Cells have four species slots. When a free slot is needed, admission selects distinct candidate gases offered by neighboring flow or diffusion, ordered by largest offered amount and then lower gas index. If all four slots are occupied and the cell does not hold a fifth species, that species cannot enter; the simulation does not delete it to make room.

## Open volume and actor drag

Cellular material closes a cell to gas. Fluid closes it when the renderer-style smoothed occupancy exceeds `0.5`; occupancy is estimated from the fluid raster and the density of the largest material identifier sampled in the cell's neighborhood. Gas also treats actor shapes as closed where the cell center lies inside them. Closed cells use a minimum pressure volume during flow evaluation, which pushes their registered gas outward.

An actor's motion creates a stream function for flow around a moving rounded cylinder, with a pair of wake eddies behind it. Differences of the stream function at cell corners produce right- and top-face flows, so this added field is divergence free. Nearby cells blend their face flow toward the actor's field. Actor drag uses the actor records passed to the gas pass.

Tiles wake when their measured open volume or drag changes, when an edit wakes them, or when simulation changes gas or leaves a nonzero flow. `collect` schedules awake tiles and their 3×3 tile neighborhoods. A flow that would affect a sleeping neighbor wakes it for a later tick rather than exchanging gas immediately.

> **Limitation:** Coupling is one way. Gas does not push fluid particles, actors, or cellular matter; fluid and actors only displace gas. There is no momentum exchange back to those systems.

> **Limitation:** Gas in a fully enclosed closed region stays trapped. Closed cells expel gas through open faces, but the solver has no route through a fully closed boundary.
