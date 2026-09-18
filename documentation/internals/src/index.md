# Dogwood Engine Internals

This book describes how the current engine is assembled. It is an architecture
map, not a Rustdoc mirror: each page explains ownership, representations,
ordering, and the first source files to open.

The public game-facing manual is the right starting point for gameplay API
usage. Use this book when a change crosses a subsystem boundary or when a
simulation result needs to be traced from authored data to GPU buffers and
back again.

## Reading Paths

- Start with [Architecture](architecture/overview.md) for crate boundaries,
  coordinate systems, and the CPU/GPU split.
- Follow [Runtime](runtime/application.md), [Scene model](scenes/scene.md), and
  the [Simulation pipeline](simulation/pipeline.md) to understand one frame.
- Read [Rigid bodies](simulation/rigid-bodies.md) for the most involved
  representation handoff.
- Use [Cross-system flows](flows.md) to trace common data journeys.

The source tree and tests are authoritative. Names in this book describe the
implementation that exists today, including bounded asynchronous readbacks and
resident-region limitations.
