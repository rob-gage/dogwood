// Copyright Rob Gage 2026
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;
use std::sync::mpsc::sync_channel;
use std::time::Duration;

use engine_compute::Accelerator;
use engine_compute::AcceleratorBuffer;

use super::CHUNK_STREAMING_QUEUE_CAPACITY;
use super::RIGID_DORMANCY_READBACK_SLOTS;
use super::Scene;
use super::SceneRigidBodyStreamingResponse;
use crate::actors::ActorRegistry;
use crate::chunks::Chunk;
use crate::chunks::ChunkEntry;
use crate::chunks::ChunkStreamingResponse;
use crate::materials::MaterialTable;
use crate::scenes::SceneData;
use crate::scenes::SceneEditBatch;
use crate::scenes::SceneGenerator;
use crate::scenes_streaming::FluidDownload;
use crate::scenes_streaming::GasDownload;
use crate::simulation::CellularCollision;
use crate::simulation::CellularDynamic;
use crate::simulation::CellularPhysicsBodyProxy;
use crate::simulation::CellularPressure;
use crate::simulation::CellularStaticStateGather;
use crate::simulation::Fluids;
use crate::simulation::Gases;
use crate::simulation::MaterialMutations;
use crate::simulation::MaterialReactions;
use crate::simulation::RigidCellStateGather;
use crate::simulation::RigidCellStateUpload;
use crate::simulation::ScenePhysicsWorld;
use crate::simulation::SceneSimulationConfiguration;
use crate::simulation::ThermalConduction;
use crate::simulation::ThermalEdits;
use crate::simulation::ThermalInteraction;
use crate::simulation::ThermalPhaseTransitions;
use crate::simulation::ThermalScatter;
use crate::simulation_materials::MaterialExtraction;
use crate::tiles::Tile;
use crate::tiles::TileArea;
use crate::tiles::TileCoordinates;

impl Scene {
    /// Loads a `Scene` from existing `SceneData`, generating chunks that are not stored
    pub fn load_with_generator(
        accelerator: &Arc<Accelerator>,
        simulation: SceneSimulationConfiguration,
        data: SceneData,
        generator: impl SceneGenerator + 'static,
    ) -> Result<Self, Box<dyn Error>> {
        simulation.validate()?;
        let accelerator: Arc<Accelerator> = accelerator.clone();
        let generator: Arc<dyn SceneGenerator> = Arc::new(generator);
        let buffer_size: u16 = u16::from(simulation.buffer_size) * 2;
        let buffered_tile_count: usize =
            (simulation.width + buffer_size) as usize * (simulation.height + buffer_size) as usize;
        let buffered_cell_count: usize = buffered_tile_count * 64;
        let cellular_material_identifiers: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_cell_count);
        let cellular_appearances: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_cell_count);
        let cellular_integrities: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count);
        let cellular_amounts: AcceleratorBuffer = accelerator.allocate::<f32>(buffered_cell_count);
        let cellular_temperatures: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count);
        let rigid_cell_integrities: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count);
        let rigid_cell_amounts: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count);
        let rigid_cell_temperatures: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count);
        let cellular_physics_body_proxy: CellularPhysicsBodyProxy =
            CellularPhysicsBodyProxy::new(accelerator.as_ref(), buffered_cell_count);
        let material_table: MaterialTable =
            MaterialTable::new(accelerator.as_ref(), data.materials());
        // one-tick chemical energy source consumed by unified thermal gathering.
        let reaction_energy: AcceleratorBuffer = accelerator.allocate::<f32>(buffered_cell_count);
        let fluids: Fluids = Fluids::new(
            accelerator.as_ref(),
            &cellular_material_identifiers,
            cellular_physics_body_proxy.occupancy_buffer(),
            cellular_physics_body_proxy.velocity_buffer(),
            &material_table.graphics().fluid_properties,
            material_table.properties_buffer(),
            material_table.parameters_buffer(),
            simulation.width + buffer_size,
            simulation.height + buffer_size,
        );
        let gases: Gases = Gases::new(
            accelerator.as_ref(),
            data.materials(),
            &cellular_material_identifiers,
            cellular_physics_body_proxy.occupancy_buffer(),
            fluids.coverage_buffer(),
            &material_table.graphics().gas_properties,
            buffered_cell_count,
            simulation.ambient_temperature,
        );
        let ambient_gas_temperature: Vec<u8> =
            vec![simulation.ambient_temperature.to_bits().to_le_bytes(); buffered_cell_count]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
        accelerator.wgpu_queue().write_buffer(
            gases.temperature_buffer().wgpu_buffer(),
            0,
            &ambient_gas_temperature,
        );
        let thermal_interaction: ThermalInteraction = ThermalInteraction::new(
            accelerator.as_ref(),
            data.materials(),
            &cellular_material_identifiers,
            &cellular_amounts,
            &cellular_temperatures,
            cellular_physics_body_proxy.rigid_claims_buffer(),
            cellular_physics_body_proxy.rigid_cells_buffer(),
            &rigid_cell_amounts,
            &rigid_cell_temperatures,
            fluids.derived_thermal_buffer(),
            fluids.coverage_buffer(),
            &reaction_energy,
            gases.concentrations_buffer(),
            gases.temperature_buffer(),
            material_table.properties_buffer(),
            material_table.parameters_buffer(),
            cellular_physics_body_proxy.occupancy_buffer(),
            buffered_cell_count as u32,
            buffered_cell_count as u32,
            gases.gas_count(),
            simulation.ambient_temperature,
            simulation.empty_space_thermal_conductivity,
            simulation.empty_space_heat_capacity,
        );
        let thermal_conduction: ThermalConduction = ThermalConduction::new(
            accelerator.as_ref(),
            thermal_interaction.interaction_buffer(),
            buffered_cell_count as u32,
        );
        let thermal_scatter: ThermalScatter = ThermalScatter::new(
            accelerator.as_ref(),
            thermal_conduction.solved_buffer(),
            &cellular_material_identifiers,
            &cellular_amounts,
            &cellular_temperatures,
            fluids.particles_buffer(),
            gases.concentrations_buffer(),
            gases.temperature_buffer(),
            fluids.coverage_buffer(),
            cellular_physics_body_proxy.rigid_claims_buffer(),
            cellular_physics_body_proxy.rigid_cells_buffer(),
            &rigid_cell_amounts,
            &rigid_cell_temperatures,
            thermal_interaction.rigid_raster_claim_counts_buffer(),
            cellular_physics_body_proxy.occupancy_buffer(),
            buffered_cell_count as u32,
            fluids.particle_capacity(),
            gases.gas_count(),
            buffered_cell_count as u32,
            simulation.empty_space_heat_capacity,
        );
        let cellular_dynamic: CellularDynamic = CellularDynamic::new(
            accelerator.as_ref(),
            &cellular_material_identifiers,
            &cellular_appearances,
            &cellular_amounts,
            &cellular_temperatures,
            cellular_physics_body_proxy.occupancy_buffer(),
            simulation.width + buffer_size,
            simulation.height + buffer_size,
        );
        let material_mutations: MaterialMutations = MaterialMutations::new(
            accelerator.as_ref(),
            data.materials(),
            &cellular_material_identifiers,
            &cellular_appearances,
            &cellular_integrities,
            cellular_dynamic.kinematics_buffer(),
            &cellular_amounts,
            &cellular_temperatures,
            fluids.edit_cells_buffer(),
            fluids.edit_amounts_buffer(),
            fluids.edit_temperatures_buffer(),
            fluids.accelerator_edits_pending_buffer(),
            gases.velocity_buffer(),
            gases.concentrations_buffer(),
            gases.temperature_buffer(),
            fluids.particles_buffer(),
            fluids.free_indices_buffer(),
            fluids.free_count_buffer(),
            buffered_cell_count,
            gases.gas_count(),
        );
        let rigid_cell_state_upload: RigidCellStateUpload = RigidCellStateUpload::new(
            accelerator.as_ref(),
            &rigid_cell_integrities,
            &rigid_cell_amounts,
            &rigid_cell_temperatures,
            buffered_cell_count,
        );
        let rigid_cell_state_gather: RigidCellStateGather = RigidCellStateGather::new(
            accelerator.as_ref(),
            &rigid_cell_integrities,
            &rigid_cell_amounts,
            &rigid_cell_temperatures,
            buffered_cell_count,
        );
        let cellular_static_state_gather: CellularStaticStateGather =
            CellularStaticStateGather::new(
                accelerator.as_ref(),
                &cellular_material_identifiers,
                &cellular_appearances,
                &cellular_integrities,
                &cellular_amounts,
                &cellular_temperatures,
                buffered_cell_count,
            );
        let rigid_dormancy_readbacks: Vec<wgpu::Buffer> = (0..RIGID_DORMANCY_READBACK_SLOTS)
            .map(|index| {
                accelerator
                    .wgpu_device()
                    .create_buffer(&wgpu::BufferDescriptor {
                        label: Some(&format!("rigid dormancy readback {index}")),
                        size: buffered_cell_count as u64 * 16,
                        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    })
            })
            .collect();
        let thermal_phase_transitions: ThermalPhaseTransitions = ThermalPhaseTransitions::new(
            accelerator.as_ref(),
            &cellular_material_identifiers,
            &cellular_amounts,
            &cellular_temperatures,
            fluids.particles_buffer(),
            gases.concentrations_buffer(),
            gases.temperature_buffer(),
            material_table.properties_buffer(),
            material_table.parameters_buffer(),
            material_mutations.requests_buffer(),
            material_mutations.request_count_buffer(),
            material_mutations.gas_fluid_candidates_buffer(),
            buffered_cell_count as u32,
            fluids.particle_capacity(),
            gases.gas_count(),
            cellular_physics_body_proxy.rigid_claims_buffer(),
            cellular_physics_body_proxy.rigid_cells_buffer(),
            &rigid_cell_amounts,
            &rigid_cell_temperatures,
            fluids.free_indices_buffer(),
            fluids.free_count_buffer(),
        );
        let thermal_edits: ThermalEdits = ThermalEdits::new(
            accelerator.as_ref(),
            &cellular_material_identifiers,
            &cellular_temperatures,
            gases.temperature_buffer(),
            fluids.particles_buffer(),
            cellular_physics_body_proxy.rigid_claims_buffer(),
            cellular_physics_body_proxy.rigid_cells_buffer(),
            &rigid_cell_temperatures,
            buffered_cell_count as u32,
            fluids.particle_capacity(),
            buffered_cell_count as u32,
        );
        let cellular_pressure: CellularPressure = CellularPressure::new(
            accelerator.as_ref(),
            data.materials(),
            &cellular_material_identifiers,
            &cellular_appearances,
            &cellular_integrities,
            &rigid_cell_integrities,
            cellular_dynamic.kinematics_buffer(),
            cellular_physics_body_proxy.occupancy_buffer(),
            cellular_physics_body_proxy.velocity_buffer(),
            cellular_physics_body_proxy.rigid_owners_buffer(),
            cellular_physics_body_proxy.rigid_claims_buffer(),
            cellular_physics_body_proxy.rigid_material_identifiers_buffer(),
            cellular_physics_body_proxy.rigid_transforms_buffer(),
            cellular_physics_body_proxy.rigid_cells_buffer(),
            fluids.mechanical_cells_buffer(),
            gases.velocity_buffer(),
            gases.concentrations_buffer(),
            &material_table.graphics().gas_properties,
            fluids.coverage_buffer(),
            material_mutations.requests_buffer(),
            material_mutations.request_count_buffer(),
            gases.gas_count(),
            buffered_cell_count,
        );
        let material_reactions: MaterialReactions = MaterialReactions::new(
            accelerator.as_ref(),
            &material_table,
            &cellular_material_identifiers,
            &cellular_amounts,
            &cellular_temperatures,
            gases.temperature_buffer(),
            &rigid_cell_temperatures,
            cellular_pressure.retained_pressure(),
            fluids.coverage_buffer(),
            gases.concentrations_buffer(),
            cellular_physics_body_proxy.occupancy_buffer(),
            cellular_physics_body_proxy.rigid_claims_buffer(),
            cellular_physics_body_proxy.rigid_cells_buffer(),
            &rigid_cell_amounts,
            reaction_energy,
            cellular_pressure.pending_pressure(),
            material_mutations.requests_buffer(),
            material_mutations.request_count_buffer(),
            fluids.authority_view(),
            buffered_cell_count as u32,
            gases.gas_count(),
            data.materials().reactions().len() as u32,
        );
        let material_extraction: MaterialExtraction = MaterialExtraction::new(
            accelerator.as_ref(),
            data.materials(),
            &cellular_material_identifiers,
            &cellular_appearances,
            &cellular_integrities,
            cellular_dynamic.kinematics_buffer(),
            &cellular_amounts,
            &cellular_temperatures,
            cellular_physics_body_proxy.occupancy_buffer(),
            fluids.particles_buffer(),
            fluids.free_indices_buffer(),
            fluids.free_count_buffer(),
            gases.concentrations_buffer(),
            cellular_physics_body_proxy.rigid_cells_buffer(),
            cellular_physics_body_proxy.rigid_transforms_buffer(),
            &rigid_cell_amounts,
            buffered_cell_count as u32,
            fluids.particle_capacity(),
            gases.gas_count(),
        );
        let cellular_collision: CellularCollision = CellularCollision::new(
            accelerator.as_ref(),
            &cellular_material_identifiers,
            simulation.width + buffer_size,
            simulation.height + buffer_size,
        );
        let fluid_download_pool: Vec<Arc<Mutex<FluidDownload>>> =
            vec![Arc::new(Mutex::new(FluidDownload::new(
                accelerator.as_ref(),
                TileArea::new(TileCoordinates { x: 0, y: 0 }, 1, 1),
                fluids.particle_capacity(),
            )))];
        let maximum_gas_streaming_cell_count: u32 = u32::from(simulation.streaming_batch_size)
            * u32::from((simulation.width + buffer_size).max(simulation.height + buffer_size))
            * 64;
        let gas_download_pool: Vec<Arc<Mutex<GasDownload>>> =
            vec![Arc::new(Mutex::new(GasDownload::new(
                accelerator.as_ref(),
                TileArea::new(TileCoordinates { x: 0, y: 0 }, 1, 1),
                maximum_gas_streaming_cell_count,
                gases.gas_count(),
            )))];
        let fluid_sample_buffer: wgpu::Buffer =
            accelerator
                .wgpu_device()
                .create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Pawn fluid sample readback"),
                    size: 32,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
        let tile_count: u32 = buffered_tile_count as u32;
        let tiles: Box<[Tile]> = (0..tile_count).map(Tile).collect();
        let (chunk_streaming_response_sender, chunk_streaming_responses): (
            SyncSender<ChunkStreamingResponse>,
            Receiver<ChunkStreamingResponse>,
        ) = sync_channel(CHUNK_STREAMING_QUEUE_CAPACITY);
        let (rigid_streaming_response_sender, rigid_streaming_responses): (
            SyncSender<SceneRigidBodyStreamingResponse>,
            Receiver<SceneRigidBodyStreamingResponse>,
        ) = sync_channel(CHUNK_STREAMING_QUEUE_CAPACITY);
        let mut scene: Self = Self {
            accelerator,
            data,
            generator,
            actor_registry: ActorRegistry::new(),
            possessed_actor: None,
            actor_contact_events: Vec::new(),
            actor_snapshots: HashMap::new(),
            actor_initialized_regions: HashSet::new(),
            chunks: HashMap::new(),
            chunk_streaming_response_sender,
            chunk_streaming_responses,
            chunks_streaming_identifier_next: 0,
            tick_time: Duration::ZERO,
            pending_runtime_edits: SceneEditBatch::new(),
            tiles,
            tile_streaming_batch_size: simulation.streaming_batch_size,
            simulation_width: simulation.width,
            simulation_height: simulation.height,
            simulation_buffer_size: simulation.buffer_size,
            origin: TileCoordinates { x: 0, y: 0 },
            origin_target: TileCoordinates { x: 0, y: 0 },
            area_request: None,
            tiles_ring_offset_x: 0,
            tiles_ring_offset_y: 0,
            tile_downloads: Mutex::new(Vec::new()),
            outgoing_tile_downloads: Vec::new(),
            fluid_downloads: Vec::new(),
            fluid_download_pool,
            gas_downloads: Vec::new(),
            gas_download_pool,
            tile_uploads: Mutex::new(Vec::new()),
            fluid_uploads: Vec::new(),
            fluid_sample_buffer,
            fluid_sample_result: Arc::new(Mutex::new(None)),
            fluid_sample_actor: None,
            cellular_material_identifiers,
            cellular_appearances,
            cellular_integrities,
            cellular_amounts,
            cellular_temperatures,
            ambient_temperature: simulation.ambient_temperature,
            rigid_cell_integrities,
            rigid_cell_amounts,
            rigid_cell_temperatures,
            rigid_cell_state_upload,
            rigid_cell_state_gather,
            cellular_static_state_gather,
            cellular_physics_body_proxy,
            rigid_cellular_bodies: Vec::new(),
            rigid_cellular_body_identifier_next: 1,
            rigid_cell_state_generations: vec![0; buffered_cell_count],
            rigid_cell_state_free: (0..buffered_cell_count as u32).rev().collect(),
            rigid_dormancy_batches: Vec::new(),
            rigid_dormancy_readbacks,
            rigid_dormancy_readback_free: (0..RIGID_DORMANCY_READBACK_SLOTS).rev().collect(),
            rigid_streaming_response_sender,
            rigid_streaming_responses,
            rigid_owner_loads: HashMap::new(),
            rigid_owner_load_queue: VecDeque::new(),
            rigid_owner_generation: HashMap::new(),
            rigid_desired_owners: HashSet::new(),
            rigid_persistence_queue: VecDeque::new(),
            rigid_io_in_flight: 0,
            rigid_activation_pending: HashSet::new(),
            rigid_sleeping_pending: HashSet::new(),
            rigid_activation_collision_origin: None,
            rigid_cellular_topology_revision: 0,
            rigid_cellular_contact_active: Vec::new(),
            rigid_cellular_support: Vec::new(),
            rigid_cellular_recovery: Vec::new(),
            rigid_granular_contact_active: Vec::new(),
            rigid_detachment_snapshot: None,
            pending_static_detachment: None,
            static_detachment_in_flight_generation: None,
            static_detachment_generation: 0,
            static_detachment_visit_stamps: Vec::new(),
            static_detachment_visit_generation: 0,
            fluids,
            gases,
            material_mutations,
            thermal_edits,
            material_table,
            material_reactions,
            material_extraction,
            material_extractions_queue: VecDeque::new(),
            material_extraction_results: Vec::new(),
            material_extraction_request_next: 0,
            thermal_interaction,
            thermal_conduction,
            thermal_scatter,
            thermal_phase_transitions,
            cellular_dynamic,
            cellular_pressure,
            cellular_collision,
            cellular_collision_dirty: true,
            gravity: simulation.gravity,
            physics_world: ScenePhysicsWorld::new(),
        };
        scene.rigid_cellular_body_identifier_next = scene.data.next_dormant_rigid_id()?;
        for coordinates in scene.area_streaming().iterate_chunk_coordinates() {
            let mut generated: bool = false;
            let mut chunk: Chunk = match scene.data.read_chunk(coordinates)? {
                Some(chunk) => chunk,
                None => {
                    generated = true;
                    scene.generator.generate_chunk(coordinates)
                }
            };
            chunk.resolve_uninitialized_temperatures(|identifier| {
                scene.initial_temperature(identifier)
            });
            scene.chunks.insert(
                coordinates,
                ChunkEntry::Active {
                    chunk,
                    is_dirty: false,
                },
            );
            if generated {
                scene.generate_actor_region(coordinates);
            }
        }
        for owner in scene
            .area_buffered()
            .expanded(Chunk::WIDTH, Chunk::WIDTH, Chunk::WIDTH, Chunk::WIDTH)
            .chunk_area()
            .iterate_chunk_coordinates()
        {
            scene.rigid_desired_owners.insert(owner);
            scene.rigid_owner_load(owner);
        }
        drop(scene.tiles_upload(scene.area_buffered()));
        scene.fluid_uploads_queue(scene.area_buffered())?;
        let buffered_area: TileArea = scene.area_buffered();
        scene.gas_clear_area(buffered_area);
        scene.gas_upload_area(buffered_area)?;
        Ok(scene)
    }
}
