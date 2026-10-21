+++
title = "Windows and surfaces"
description = "How the host creates a native window, configures its WGPU surface, and presents frames."
weight = 20
[extra]
sources = ["engine_host/src/windows"]
+++

This page describes how the host creates a native window, configures its WGPU surface, and recovers while presenting frames. It is for engine contributors; the event-driven scheduling that triggers these steps is in [event loop and timing](@/internals/runtime/event-loop.md).

## Window and surface creation

`Window::open` creates a Winit event loop and runs a `Window` application handler. On `resumed`, the handler does the following:

1. Creates an opaque native window and wraps it in `Arc`.
2. Creates a WGPU surface from that window.
3. If the initial physical size is nonzero, gets WGPU's default surface configuration for the selected adapter and size. Otherwise it leaves the configuration unset and skips presentation until a nonzero resize.
4. Chooses the first advertised sRGB format when available.
5. If a default configuration was created, configures the surface.

If there is no supported default configuration, initialization returns `WindowError::UnsupportedSurface`.

On Linux, when `XDG_CURRENT_DESKTOP` includes `GNOME` and `DISPLAY` is nonempty, the host selects X11 (including XWayland) so GNOME supplies system window decorations. Otherwise Winit selects the window system automatically.

### Configuration source

The host explicitly inspects advertised formats only to prefer an sRGB format; the remaining configuration values, including present and alpha modes, come from `Surface::get_default_config`. When an sRGB format is available, a source comment says scene rendering writes linear color and the surface encodes it as sRGB. See [GPU accelerator](@/internals/runtime/accelerator.md) for the adapter selection that precedes surface setup.

## Resize and suspension

A resize to zero width or height clears the stored `SurfaceConfiguration`, which suspends presentation while minimized. A nonzero resize reuses the current configuration, or gets a default one if none is stored. It updates the width and height, then calls `configure`. If no default configuration can be obtained, the resize returns `UnsupportedSurface` and the event loop exits.

When Winit calls `suspended`, the handler drops its surface, configuration, UI window input state, and native window, and clears the update accumulator. A later `resumed` call recreates the window, surface, and UI input state. It creates a surface configuration only when the window has nonzero physical size.

## Acquiring and presenting frames

On `RedrawRequested`, a missing surface configuration makes the host skip presentation. Otherwise, `present` acquires the current surface texture:

| Acquisition status | Host action |
| --- | --- |
| `Success` | Render and present the frame. |
| `Suboptimal` | Render and present, then reconfigure the surface. |
| `Timeout` or `Occluded` | Skip this frame. |
| `Outdated` | Reconfigure, then make one retry through `present_after_reconfigure`. |
| `Lost` | Recreate the surface from the window. If creation succeeds, reconfigure and retry once; if it fails, return `WindowError::SurfaceCreation` and exit the event loop. |
| `Validation` | Return `WindowError::SurfaceValidation` and exit the event loop. |

### Retry behaviour

A retry renders `Success` or `Suboptimal` and reconfigures after the latter. Retry results of `Timeout`, `Occluded`, `Outdated`, or `Lost` are skipped without a second retry. `Validation` still returns an error.

### Rendering a frame

For a rendered frame, the host takes these steps:

1. Create a texture view.
2. Take pending egui input.
3. Invoke the render callback.
4. Apply egui platform output.
5. Present through the accelerator queue.

## Window errors

`WindowError` has five variants:

| Variant | Meaning |
| --- | --- |
| `WinitEventLoop` | Winit event loop error |
| `WinitOs` | The OS refused an operation |
| `SurfaceCreation` | WGPU could not create the surface |
| `UnsupportedSurface` | The adapter lacks surface support or a default configuration |
| `SurfaceValidation` | Validation failure while acquiring a frame |

See [host types](@/external-api/host.md) for the public window configuration and error types.
