+++
title = "GPU accelerator"
description = "The host's WGPU instance, adapter, device, queue, and initialization path."
weight = 30
[extra]
sources = ["engine_host/src/acceleration", "engine_host/src/windows/window.rs"]
+++

This page describes the host's `Accelerator`: the WGPU instance, adapter, device, and queue, and how they are initialized. It is for engine contributors.

`Accelerator` is a cloneable bundle of a WGPU `Instance`, `Adapter`, `Device`, and `Queue`. Its public accessors return shared references to those resources. In the facade, the type and its error are available as {{ <rustdoc path="engine/struct.Accelerator.html" text="engine::Accelerator" /> }} and {{ <rustdoc path="engine/enum.AcceleratorError.html" text="engine::AcceleratorError" /> }}.

## How other layers use it

- Graphics and simulation passes use the device and queue to create GPU resources and submit work.
- The window layer creates the surface with the instance, gets its default configuration and capabilities from the adapter, and configures it with the device.

## Instance and device selection

The instance enables these backends and is created without a display handle:

| Target | Backends |
| --- | --- |
| Linux | Vulkan |
| Windows | Vulkan and DX12 |
| macOS | Metal |

The host initializer returns `UnsupportedPlatform` outside Linux, Windows, and macOS; this does not imply that the remaining workspace dependencies support those targets.

### Adapter request

- `PowerPreference::HighPerformance`.
- `compatible_surface` is unset, so adapter selection is not constrained to the window's surface.
- No fallback adapter is forced.
- Limit buckets are disabled.

### Device request

Device creation requests `max_storage_buffers_per_shader_stage = 20`. All other limits and device descriptor fields use WGPU defaults. The combined buffer budget remains 28; Metal exposes 29 usable slots shared by storage, uniform, and vertex buffers. Individual buffer maxima are not added together. Before requesting a device, the initializer checks all requested limits against the adapter and reports the first unsupported limit with the adapter name, backend, required value, and supported value. The fluid particle bind group requires twenty storage buffers per compute stage.

## Initialization and errors

`Accelerator::new()` is the fallible initializer: it creates the instance and blocks on async initialization with `pollster::block_on`. `Default` calls `new()`. `Clone` duplicates an existing bundle.

- Limit checks return `AcceleratorError::UnsupportedLimit`.
- The async adapter and device requests map failures to `AcceleratorError::RequestAdapter` and `RequestDevice`.
- Any failure makes `Accelerator::default()` panic with the error text; `Accelerator::new()` returns the error instead.

For surface format choice and recovery, see [windows and surfaces](@/internals/runtime/surfaces.md).
