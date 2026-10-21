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

The initializer has an `UnsupportedPlatform` return branch for targets outside Linux, Windows, and macOS, but the current code does not compile on those targets because `backends` is undefined there.

### Adapter request

- `PowerPreference::HighPerformance`.
- `compatible_surface` is unset, so adapter selection is not constrained to the window's surface.
- No fallback adapter is forced.
- Limit buckets are disabled.

### Device request

Device creation requests `max_storage_buffers_per_shader_stage = 20`. All other limits and device descriptor fields use WGPU defaults. The fluid particle bind group requires twenty storage buffers per compute stage.

## Initialization and errors

`Default` is the only public initializer: it creates the instance and blocks on async initialization with `pollster::block_on`. `Clone` duplicates an existing bundle.

- The async adapter and device requests map failures to `AcceleratorError::RequestAdapter` and `RequestDevice`.
- Any failure makes `Accelerator::default()` panic with the error text; there is no public fallible constructor.

For surface format choice and recovery, see [windows and surfaces](@/internals/runtime/surfaces.md).
