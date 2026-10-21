+++
title = "Host internals"
description = "Window event dispatch, GPU context creation, and worker pool ownership."
weight = 30
[extra]
sources = ["engine_host/src/windows", "engine_host/src/acceleration", "engine_host/src/processors", "engine_user_interface/src/user_interface_context.rs", "engine_user_interface/src/user_interface_window_state.rs"]
+++

This page is for engine contributors. It covers how `Window` dispatches native events to game callbacks, how `Accelerator` creates the GPU context, and how `Processor` allocates worker pools.

## Window and callback boundary

`Window<'a>` implements `winit::application::ApplicationHandler`. It stores optional native-window, surface, surface configuration, and UI window-state values so that suspension and zero-sized windows can release or suspend presentation resources.

### Callbacks

The callback references are borrowed for the event loop's lifetime:

```text
// window events
&mut dyn FnMut(SystemWindowEvent)
// keyboard
&mut dyn FnMut(SystemKeyboardEvent)
// mouse
&mut dyn FnMut(SystemMouseEvent)
// fixed update (elapsed)
&mut dyn FnMut(Duration)
// render: draws and returns UI platform output
&mut dyn FnMut(&wgpu::TextureView, egui::RawInput) -> egui::PlatformOutput
```

`Window::open` accepts, in addition to those five callbacks:

- `&Accelerator`
- `&UserInterfaceContext`
- `WindowConfiguration`
- `update_interval: Duration`
- `maximum_accumulated_time: Duration`

It returns `Result<(), WindowError>`.

### Event routing

The UI processes every native window event first, but UI consumption does not stop translated events from reaching the game. The host translates recognized events:

- Focus and close events go to the window callback.
- A resize event reaches the window callback only if `resize_surface` succeeds; on failure, the host exits first.
- Keyboard and mouse events go to their respective callbacks.
- `RedrawRequested` attempts presentation when the surface is configured.

Other native events are not translated to game callbacks. See [Event loop and timing](@/internals/runtime/event-loop.md) for timing and event details.

Frame acquisition may skip the frame on timeout or occlusion; an outdated or lost texture triggers one retry, which may also be skipped or fail. See [Windows and surfaces](@/internals/runtime/surfaces.md).

### Surface lifecycle

On resume, the window creates the native window and a `'static` WGPU surface backed by an `Arc<winit::window::Window>`. It stores the current `SurfaceConfiguration` only when the physical size is nonzero.

- Resizing can clear the configuration when the window is minimized; presentation returns early while it is absent.
- A lost surface is recreated, and outdated or suboptimal surfaces are reconfigured.
- `Window` delivers a frame to the render callback, applies its UI platform output, and presents through the accelerator queue.

## Accelerator

`Accelerator` is cloneable and privately owns a WGPU instance, adapter, device, and queue. Public getters expose references to all four.

- The instance backend mask is Vulkan on Linux, Vulkan | DX12 on Windows, and Metal on macOS.
- It requests a high-performance adapter without a compatible surface or fallback adapter.
- Device requirements and the shared Metal buffer budget are described in [GPU accelerator](@/internals/runtime/accelerator.md).
- `Accelerator::new` returns initialization errors; `Default` panics on failure. Initialization blocks through `pollster::block_on`.
 See [GPU accelerator](@/internals/runtime/accelerator.md) for backend and error details.

## Processor pools

`Processor` owns a `HashMap<String, Arc<rayon::ThreadPool>>`. `thread_pool(&self, thread_pool_name: &str) -> Option<Arc<rayon::ThreadPool>>` clones a named handle; a pool's worker count does not cap queued jobs.

`Processor::new(configuration: ProcessorConfiguration) -> Result<Self, ProcessorError>` validates the configuration and builds named pools. CPU-bound pools share unreserved capacity by weight; dedicated pools use configured counts. See [CPU worker pools](@/internals/runtime/processor-pools.md) for validation, allocation, and defaults.
