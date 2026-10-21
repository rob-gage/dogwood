+++
title = "Host types"
description = "GPU and CPU resources, normalized host events, and native window configuration."
weight = 20
[extra]
sources = ["engine_host/src/acceleration", "engine_host/src/processors", "engine_host/src/system_events", "engine_host/src/windows/window.rs", "engine_host/src/windows/window_configuration.rs", "engine_host/src/windows/window_error.rs", "engine/src/lib.rs", "engine_host/src/lib.rs", "engine_host/src/windows/mod.rs"]
+++

Host types are the GPU, CPU, input, and window primitives under the engine. This page is for game programmers who configure resources or handle input. Games reach `Accelerator`, `Processor`, `ProcessorConfiguration`, and their errors at the `engine` root, and input types at `engine::system_events`.

## GPU context

`Accelerator` owns a WGPU instance, adapter, device, and queue. Its `instance()`, `adapter()`, `device()`, and `queue()` accessors return shared references.

`Accelerator::default()` requests a high-performance adapter without a compatible surface and requests 20 storage buffers per shader stage. It panics if backend, adapter, or device initialization fails.

`AcceleratorError` variants:

- `RequestAdapter(wgpu::RequestAdapterError)`
- `RequestDevice(wgpu::RequestDeviceError)`
- `UnsupportedPlatform`

## CPU pools

`Processor` owns named Rayon thread pools. Build one from a `ProcessorConfiguration`.

### Configuration fields

| Field | Type | Meaning |
| --- | --- | --- |
| `processor_capacity` | `usize` | Total CPU workers to budget |
| `processor_capacity_reserved` | `usize` | Workers kept out of the pools |
| `pool_weights` | `Vec<(String, usize)>` | CPU-bound pool names and relative weights |
| `dedicated_worker_pool_counts` | `Vec<(String, usize)>` | Fixed-size pool names and worker counts |

`ProcessorConfiguration::default()` uses available parallelism (one worker if unknown) and reserves one worker when capacity exceeds one.

### Methods

- `ProcessorConfiguration::new(capacity, reserved)` starts with no pools.
- `add_cpu_bound_pool(name, relative_weight)` adds a weighted pool.
- `add_dedicated_worker_pool(name, worker_count)` adds a fixed-size pool.
- `Processor::new(configuration) -> Result<Self, ProcessorError>` validates the budget, names, weights, and worker counts, then builds the pools.
- `thread_pool(name)` returns an optional shared `Arc<rayon::ThreadPool>`.

### Worker allocation

The processor assigns one worker to each CPU-bound pool, then distributes the remaining unreserved workers by relative weight. Dedicated pool workers are added separately and can raise the total above `processor_capacity`. Worker counts limit workers, not queued jobs.

### Default pools

`Processor::default()` panics if configuration or pool creation fails. It creates:

| Pool | Kind | Size |
| --- | --- | --- |
| `simulation` | CPU-bound | Weight 3 |
| `world_generation` | CPU-bound | Weight 1, only when at least two unreserved workers are available |
| `chunk_io` | Dedicated | Two workers |

### Errors

`ProcessorError` is either `InvalidConfiguration(String)` or `RayonThreadPoolBuild { name: String, source: rayon::ThreadPoolBuildError }`.

## System events

Each event type is the argument of the matching `Game::*_event` callback; see [Engine and Game](@/external-api/engine-and-games.md).

| Event type | Variants and fields | Units |
| --- | --- | --- |
| `SystemKeyboardEvent` | Fields `physical_key: PhysicalKey`, `pressed: bool`, `repeat: bool` | None |
| `SystemMouseEvent` | `Button { button: MouseButton, pressed: bool }`, `CursorMoved { x, y }`, `CursorEntered`, `CursorLeft`, `WheelLines { horizontal, vertical }`, `WheelPixels { horizontal, vertical }` | Cursor coordinates and pixel wheel deltas are physical pixels (`f64`); line wheel deltas are `f32` lines |
| `SystemWindowEvent` | `CloseRequested`, `Resized { width: u32, height: u32 }`, `FocusAcquired`, `FocusReleased` | `Resized` is in physical pixels |

- `repeat` indicates an operating-system key repeat.
- `KeyCode` and `PhysicalKey` are re-exported from Winit under `engine::system_events`.
- `button` is a Winit `MouseButton`.

## Window settings and errors

`WindowConfiguration` has one public field, `fps_limit: u32`. `u32::MAX` means uncapped, and the default is uncapped. Finite limits are clamped to at least 1 FPS, so `0` means 1 FPS. `Game::window_configuration` returns it, and the engine passes it to the host window.

The facade does not re-export `engine_host::windows`. To use the window crate directly, depend on `engine_host`: `Window`, `WindowConfiguration`, and `WindowError` live there, and `Window::open(...) -> Result<(), WindowError>` opens the native window and dispatches host events.

`WindowError` variants:

- `WinitEventLoop(winit::error::EventLoopError)`
- `WinitOs(winit::error::OsError)`
- `SurfaceCreation(wgpu::CreateSurfaceError)`
- `UnsupportedSurface`
- `SurfaceValidation`

The first three have `From` conversions for the corresponding Winit or WGPU errors.

{{ <rustdoc path="engine/struct.Accelerator.html" text="Accelerator" /> }} · {{ <rustdoc path="engine/struct.Processor.html" text="Processor" /> }} · {{ <rustdoc path="engine/struct.ProcessorConfiguration.html" text="ProcessorConfiguration" /> }} · {{ <rustdoc path="engine/system_events/index.html" text="system_events" /> }} · {{ <rustdoc path="engine_host/windows/struct.WindowConfiguration.html" text="WindowConfiguration" /> }} · {{ <rustdoc path="engine_host/windows/enum.WindowError.html" text="WindowError" /> }}
