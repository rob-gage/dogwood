// Copyright Rob Gage 2026

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

const PHYSICS_SHADER_UTILITIES: [engine_compute::ComposableShaderUtility; 7] = [
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
        source: include_str!("../simulation_utility/thermal_material.wgsl"),
        file_path: "engine_physics/src/simulation_utility/thermal_material.wgsl",
    },
    engine_compute::ComposableShaderUtility {
        source: include_str!("../simulation_utility/tile_ring.wgsl"),
        file_path: "engine_physics/src/simulation_utility/tile_ring.wgsl",
    },
];

pub fn create_physics_shader_module(
    device: &wgpu::Device,
    label: &'static str,
    source: &'static str,
    file_path: &'static str,
) -> wgpu::ShaderModule {
    create_simulation_shader_module(device, label, source, file_path)
}

pub use crate::scene_simulation::{SceneSimulation, SceneSimulationConfiguration};
pub(crate) use crate::simulation_cellulars::CellularStaticStateGather;
pub use crate::simulation_cellulars::{
    CellParticle, CellularCollision, CellularDynamic, CellularPhysicsBodyProxy, CellularPressure,
    CollisionOccupancySnapshot,
};
pub use crate::simulation_fluids::Fluids;
pub use crate::simulation_gases::Gases;
pub use crate::simulation_materials::MaterialMutations;
pub(crate) use crate::simulation_materials::{MaterialReactions, ReactionMaterialTable};
pub use crate::simulation_rigid_bodies::ScenePhysicsWorld;
pub(crate) use crate::simulation_rigid_bodies::{
    RigidCellStateGather, RigidCellStateUpload, RigidCellularBody, RigidCellularBodyCell,
    RigidCellularBodyState, RigidGranularReactionBatch, RigidGranularReadbackSlot,
    RigidGranularReadbackStatus,
};
#[cfg(test)]
pub(crate) use crate::simulation_thermal::test_rigid_phase_readback_len;
pub(crate) use crate::simulation_thermal::{
    ThermalConduction, ThermalEdits, ThermalInteraction, ThermalMaterialTable,
    ThermalPhaseTransitions, ThermalScatter,
};
