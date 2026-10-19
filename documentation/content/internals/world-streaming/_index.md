+++
title = "World streaming and persistence"
description = "Follows chunks between generation, storage, residency, and the GPU tile ring."
weight = 20
sort_by = "weight"
template = "section.html"
[extra]
sources = ["engine_world/src/scenes/chunks", "engine_world/src/scenes/cellular_tiles", "engine_world/src/scenes/scene.rs"]
+++

This subsection is for engine contributors tracing how world data moves between disk, CPU chunks, and the GPU. Chunk streaming keeps CPU chunks available for the scene’s retained chunk regions, while the cellular tile ring keeps the currently simulated cells in GPU buffers. Before their ring slots are reused, outgoing tiles are copied to staging buffers; their contents reach the chunks later, when the asynchronous readback completes. Dirty chunks are saved asynchronously when a save is requested or when they are no longer retained.

```text
disk files <----> CPU chunks <----> GPU tile ring
                   ^    |          active region
                   |    +-- load / generate / save jobs
                   +------- tile readback and upload
```

The pages in this subsection follow that path:

- [Chunk storage format](@/internals/world-streaming/chunk-format.md) describes the CPU arrays and persisted file representation.
- [Chunk residency](@/internals/world-streaming/chunk-residency.md) covers retained regions, resident states, and dirty data.
- [Chunk I/O scheduling](@/internals/world-streaming/chunk-io.md) follows background loads, generation, and saves.
- [The cellular tile ring](@/internals/world-streaming/tile-ring.md) describes GPU buffers and logical-to-physical tile indexing.
- [Tile streaming](@/internals/world-streaming/tile-streaming.md) traces transfers as the active region moves.

Terms such as active region, retained, and resident chunk are defined in the [glossary](@/architecture/glossary.md). Scene-level ordering is covered in the [scene update pipeline](@/architecture/scene-pipeline.md).
