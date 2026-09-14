// Copyright Rob Gage 2026

mod cell_particle;
mod cellular_collision;
mod cellular_physics_body_proxy;
mod collision_readback_slot;
mod collision_readback_status;
mod collision_occupancy_snapshot;
mod cellular_dynamic;
mod cellular_pressure;
mod fluids;
mod gases;
mod scene_simulation;
mod scene_simulation_configuration;
mod scene_physics_world;
mod rigid_cellular_body;

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
pub use cellular_physics_body_proxy::CellularPhysicsBodyProxy;
pub use collision_occupancy_snapshot::CollisionOccupancySnapshot;
pub use cellular_dynamic::CellularDynamic;
pub use cellular_pressure::CellularPressure;
pub use fluids::Fluids;
pub use gases::Gases;
pub use scene_simulation::SceneSimulation;
pub use scene_simulation_configuration::SceneSimulationConfiguration;
pub use scene_physics_world::ScenePhysicsWorld;
pub(crate) use rigid_cellular_body::RigidCellularBody;
