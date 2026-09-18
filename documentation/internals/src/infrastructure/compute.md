# Compute Infrastructure

`Accelerator` owns the WGPU instance, adapter, device, queue, and debug timing
collector. It is shared by the application renderer and every physics resource;
there is one queue ordering all submitted compute and render work.

`AcceleratorBuffer` wraps storage buffers allocated with storage/copy usage.
Subsystems own their buffers and expose borrowed views to neighboring systems.
The accelerator provides polling for device progress and mapped timing
readbacks. It does not provide a scheduler or a CPU-side world model.

Physics shaders are composed from common WGSL utilities for cell coordinates,
material identifiers, thermal properties, fluid spatial helpers, actor shapes,
and ring mapping. A subsystem supplies its root shader and the shared
composition path creates the device module. Render shaders use the same
composition bridge for compatible utilities.

Ordering is explicit: Rust code writes uniform/input buffers, encodes passes,
submits a command buffer, and only later polls/maps readbacks. A WGPU queue
submission is the boundary between command encoding and execution; a buffer
copy plus map callback is the boundary between GPU result and CPU ownership.
Debug timestamp queries are optional and compiled into the application timing
sample only when the adapter supports them.

### Relevant Implementation

- `engine_compute/src/accelerator.rs` — device/queue ownership, polling, pass
  labels, and timing integration.
- `engine_compute/src/accelerator_buffer.rs` — storage buffer ownership.
- `engine_compute/src/shader_composition.rs` — Naga/WGSL module composition.
- `engine_compute/src/accelerator_timing.rs` — timestamp sample lifecycle.
- `engine_physics/src/simulation/mod.rs` — shared physics shader utilities and
  bind-group helpers.
- `engine_physics/src/simulation_utility/` — shared WGSL coordinate and data
  layout helpers.
