+++
title = "Deep internals"
description = "Implementation details of the runtime, streaming, actors, simulation, rendering, and shaders."
weight = 6
sort_by = "weight"
template = "section.html"
+++

These pages describe how Dogwood’s runtime, world, simulation, and rendering systems are implemented. Start with the subsystem that matches the code you are tracing; the [architecture pages](@/architecture/_index.md) describe how those parts fit together.

## Runtime and host

[Runtime and host](@/internals/runtime/_index.md) covers event dispatch and fixed-step timing, window surfaces, GPU accelerator setup, and CPU worker pools.

## World streaming and persistence

[World streaming and persistence](@/internals/world-streaming/_index.md) follows chunks between generation, storage, residency, and the GPU tile ring.

## Actors and systems

[Actors and systems](@/internals/actors/_index.md) explains actor identity and registry behavior, component consumers, pawn movement, and actor lifetime in the active region.

## Simulation

[Simulation](@/internals/simulation/_index.md) describes material-table compilation, cellular kinematics, Rapier collision, static-cell colliders, rigid bodies, contact exchange, and edits.

## Rendering and UI

[Rendering and UI](@/internals/rendering/_index.md) covers scene and sprite passes, cellular rendering, UI rendering and layout, and the editor viewport.

## Shaders and GPU data

[Shaders and GPU data](@/internals/shaders/_index.md) documents the WGSL stages alongside their Rust bindings, data layouts, and dispatch sites.
