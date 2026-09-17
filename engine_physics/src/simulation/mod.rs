// Copyright Rob Gage 2026

mod cell_particle;
mod cellular_collision;
mod cellular_dynamic;
mod cellular_physics_body_proxy;
mod cellular_pressure;
mod cellular_static_state_gather;
mod collision_occupancy_snapshot;
mod collision_readback_slot;
mod collision_readback_status;
mod fluids;
mod gases;
mod material_mutations;
mod material_reactions;
mod reaction_material_table;
mod rigid_cell_state_gather;
mod rigid_cell_state_upload;
mod rigid_cellular_body;
mod rigid_cellular_body_state;
mod rigid_granular_reaction_batch;
mod rigid_granular_readback_slot;
mod rigid_granular_readback_status;
mod scene_physics_world;
mod scene_simulation;
mod scene_simulation_configuration;
mod thermal_conduction;
mod thermal_edits;
mod thermal_interaction;
mod thermal_material_table;
mod thermal_phase_transitions;
mod thermal_scatter;

fn create_simulation_shader_module(
    device: &wgpu::Device,
    label: &'static str,
    source: &'static str,
    file_path: &'static str,
) -> wgpu::ShaderModule {
    engine_compute::create_composed_shader_module(device, label, source, file_path)
}

pub use cell_particle::CellParticle;
pub use cellular_collision::CellularCollision;
pub use cellular_dynamic::CellularDynamic;
pub use cellular_physics_body_proxy::CellularPhysicsBodyProxy;
pub use cellular_pressure::CellularPressure;
pub(crate) use cellular_static_state_gather::CellularStaticStateGather;
pub use collision_occupancy_snapshot::CollisionOccupancySnapshot;
pub use fluids::Fluids;
pub use gases::Gases;
pub use material_mutations::MaterialMutations;
pub(crate) use material_reactions::MaterialReactions;
pub(crate) use reaction_material_table::ReactionMaterialTable;
pub(crate) use rigid_cell_state_gather::RigidCellStateGather;
pub(crate) use rigid_cell_state_upload::RigidCellStateUpload;
pub(crate) use rigid_cellular_body::{RigidCellularBody, RigidCellularBodyCell};
pub(crate) use rigid_cellular_body_state::RigidCellularBodyState;
pub(crate) use rigid_granular_reaction_batch::RigidGranularReactionBatch;
pub use scene_physics_world::ScenePhysicsWorld;
pub use scene_simulation::SceneSimulation;
pub use scene_simulation_configuration::SceneSimulationConfiguration;
pub(crate) use thermal_conduction::ThermalConduction;
pub(crate) use thermal_edits::ThermalEdits;
pub(crate) use thermal_interaction::ThermalInteraction;
pub(crate) use thermal_material_table::ThermalMaterialTable;
pub(crate) use thermal_phase_transitions::ThermalPhaseTransitions;
pub(crate) use thermal_scatter::ThermalScatter;
