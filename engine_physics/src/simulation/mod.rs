// Copyright Rob Gage 2026

//! Shared simulation shader composition and simulation-wide constants.

pub(crate) mod simulation_constants;
#[cfg(test)]
pub(crate) mod tests;

pub(crate) fn create_simulation_shader_module(
    device: &wgpu::Device,
    label: &'static str,
    source: &'static str,
    file_path: &'static str,
) -> wgpu::ShaderModule {
    engine_compute::create_composed_shader_module_with_utilities(
        device,
        label,
        source,
        file_path,
        &PHYSICS_SHADER_UTILITIES,
    )
}

pub(crate) fn storage_bind_group_layout_entry(
    binding: u32,
    read_only: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub(crate) fn uniform_bind_group_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

pub(crate) fn accelerator_buffer_bind_group_entry<'a>(
    binding: u32,
    buffer: &'a engine_compute::AcceleratorBuffer,
) -> wgpu::BindGroupEntry<'a> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.wgpu_buffer().as_entire_binding(),
    }
}

pub(crate) fn create_simulation_uniform_buffer(
    device: &wgpu::Device,
    label: &'static str,
    size: u64,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

const PHYSICS_SHADER_UTILITIES: [engine_compute::ComposableShaderUtility; 8] = [
    engine_compute::ComposableShaderUtility {
        source: include_str!("simulation_constants.wgsl"),
        file_path: "engine_physics/src/simulation/simulation_constants.wgsl",
    },
    engine_compute::ComposableShaderUtility {
        source: include_str!("../simulation_utility/actor_collision_shape.wgsl"),
        file_path: "engine_physics/src/simulation_utility/actor_collision_shape.wgsl",
    },
    engine_compute::ComposableShaderUtility {
        source: include_str!("../simulation_utility/cell_coordinates.wgsl"),
        file_path: "engine_physics/src/simulation_utility/cell_coordinates.wgsl",
    },
    engine_compute::ComposableShaderUtility {
        source: include_str!("../simulation_utility/fluid_edit.wgsl"),
        file_path: "engine_physics/src/simulation_utility/fluid_edit.wgsl",
    },
    engine_compute::ComposableShaderUtility {
        source: include_str!("../simulation_utility/fluid_spatial.wgsl"),
        file_path: "engine_physics/src/simulation_utility/fluid_spatial.wgsl",
    },
    engine_compute::ComposableShaderUtility {
        source: include_str!("../simulation_utility/material_identifier.wgsl"),
        file_path: "engine_physics/src/simulation_utility/material_identifier.wgsl",
    },
    engine_compute::ComposableShaderUtility {
        source: include_str!("../simulation_utility/thermal_properties.wgsl"),
        file_path: "engine_physics/src/simulation_utility/thermal_properties.wgsl",
    },
    engine_compute::ComposableShaderUtility {
        source: include_str!("../simulation_utility/tile_ring.wgsl"),
        file_path: "engine_physics/src/simulation_utility/tile_ring.wgsl",
    },
];

/// Composes a physics shader with the shared non-mathematical utility modules.
pub fn create_physics_shader_module(
    device: &wgpu::Device,
    label: &'static str,
    source: &'static str,
    file_path: &'static str,
) -> wgpu::ShaderModule {
    create_simulation_shader_module(device, label, source, file_path)
}

pub use crate::simulation_actors::{SceneSimulation, SceneSimulationConfiguration};
pub(crate) use crate::simulation_cellulars::CellularStaticStateGather;
pub use crate::simulation_cellulars::{
    CellParticle, CellularCollision, CellularDynamic, CellularPhysicsBodyProxy, CellularPressure,
    CollisionOccupancySnapshot,
};
pub use crate::simulation_fluids::Fluids;
pub use crate::simulation_gases::Gases;
pub use crate::simulation_materials::MaterialMutations;
pub(crate) use crate::simulation_materials::MaterialReactions;
pub use crate::simulation_rigid_bodies::ScenePhysicsWorld;
pub(crate) use crate::simulation_rigid_bodies::{
    RigidCellStateGather, RigidCellStateUpload, RigidCellularBody, RigidCellularBodyCell,
    RigidCellularBodyState, RigidGranularReactionBatch, RigidGranularReadbackSlot,
    RigidGranularReadbackStatus,
};
#[cfg(test)]
pub(crate) use crate::simulation_thermal::test_rigid_phase_readback_len;
pub(crate) use crate::simulation_thermal::{
    ThermalConduction, ThermalEdits, ThermalInteraction, ThermalPhaseTransitions, ThermalScatter,
};
