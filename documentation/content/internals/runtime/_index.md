+++
title = "Runtime and host"
description = "The native event loop, window surfaces, GPU accelerator, and CPU worker pools."
weight = 10
sort_by = "weight"
template = "section.html"
[extra]
sources = [
    "engine/src/runtime/engine.rs",
    "engine_host/src/windows/window.rs",
    "engine_host/src/acceleration/accelerator.rs",
    "engine_host/src/processors/processor.rs",
    "engine_host/src/processors/processor_configuration.rs",
]
+++

The host connects Dogwood's game callbacks to a native window, schedules fixed updates and redraws, and provides the GPU and worker-pool resources used by the engine. These pages follow that path from the event loop through surface presentation, then describe accelerator setup and CPU pool allocation.

- [Event loop and timing](@/internals/runtime/event-loop.md) — event translation, fixed-step updates, and redraw pacing.
- [Windows and surfaces](@/internals/runtime/surfaces.md) — window lifecycle, WGPU surface configuration, resize, and presentation recovery.
- [GPU accelerator](@/internals/runtime/accelerator.md) — backend selection, adapter and device requests, and initialization errors.
- [CPU worker pools](@/internals/runtime/processor-pools.md) — pool validation, weighted worker allocation, and defaults.

For the broader update-to-present sequence, see [frame and update lifecycle](@/architecture/frame-lifecycle.md). The [host internals API page](@/internal-api/host.md) summarizes the host types and their relationship to the engine facade.
