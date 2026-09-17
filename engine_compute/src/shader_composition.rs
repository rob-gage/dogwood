// Copyright Rob Gage 2026

//! Composes root shaders with utility modules owned by their subsystem.

use naga_oil::compose::{ComposableModuleDescriptor, Composer, NagaModuleDescriptor};
use std::borrow::Cow;

/// A composable shader source supplied by the owning subsystem.
pub struct ComposableShaderUtility {
    /// WGSL source containing the utility's importable declarations
    pub source: &'static str,
    /// Stable source path used in composition diagnostics
    pub file_path: &'static str,
}

/// Composes one root shader with caller-provided utility modules.
pub fn create_composed_shader_module(
    device: &wgpu::Device,
    label: &'static str,
    source: &'static str,
    file_path: &'static str,
) -> wgpu::ShaderModule {
    create_composed_shader_module_with_utilities(device, label, source, file_path, &[])
}

/// Composes one root shader with caller-provided utility modules.
pub fn create_composed_shader_module_with_utilities(
    device: &wgpu::Device,
    label: &'static str,
    source: &'static str,
    file_path: &'static str,
    utilities: &[ComposableShaderUtility],
) -> wgpu::ShaderModule {
    let mut composer: Composer = Composer::default();
    for utility in utilities {
        composer
            .add_composable_module(ComposableModuleDescriptor {
                source: utility.source,
                file_path: utility.file_path,
                ..Default::default()
            })
            .unwrap_or_else(|error| {
                panic!(
                    "Failed to register shader utility {}: {error}",
                    utility.file_path
                )
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
