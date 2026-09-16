// Copyright Rob Gage 2026

use naga_oil::compose::{ComposableModuleDescriptor, Composer, NagaModuleDescriptor};
use std::borrow::Cow;

/// Composes one root shader with Dogwood's focused utility modules
pub fn create_composed_shader_module(
    device: &wgpu::Device,
    label: &'static str,
    source: &'static str,
    file_path: &'static str,
) -> wgpu::ShaderModule {
    let mut composer: Composer = Composer::default();
    for (utility_source, utility_path) in [
        (
            include_str!("cell_coordinates.wgsl"),
            "engine_compute/src/cell_coordinates.wgsl",
        ),
        (
            include_str!("tile_ring.wgsl"),
            "engine_compute/src/tile_ring.wgsl",
        ),
        (
            include_str!("material_identifier.wgsl"),
            "engine_compute/src/material_identifier.wgsl",
        ),
        (
            include_str!("fluid_edit.wgsl"),
            "engine_compute/src/fluid_edit.wgsl",
        ),
        (
            include_str!("actor_collision_shape.wgsl"),
            "engine_compute/src/actor_collision_shape.wgsl",
        ),
    ] {
        composer
            .add_composable_module(ComposableModuleDescriptor {
                source: utility_source,
                file_path: utility_path,
                ..Default::default()
            })
            .unwrap_or_else(|error| {
                panic!("Failed to register shader utility {utility_path}: {error}")
            });
    }
    let module: wgpu::naga::Module = composer
        .make_naga_module(NagaModuleDescriptor {
            source,
            file_path,
            ..Default::default()
        })
        .unwrap_or_else(|error| panic!("Failed to compose shader {file_path}: {error}"));
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Naga(Cow::Owned(module)),
    })
}
