// Copyright Rob Gage 2026

use super::{
    FluidDownload, FluidUpload, GasDownload, GasUpload, SceneData, SceneEdit, SceneEditBatch,
    SceneEditCellPlacement, SceneGenerator, ScenePosition, SceneVelocity, TileDownload, TileUpload,
};
use crate::simulation::{
    CellularCollision, CellularDynamic, CellularPhysicsBodyProxy, CellularPressure,
    CollisionOccupancySnapshot, Fluids, Gases, MaterialMutations, RigidCellularBody,
    RigidCellularBodyCell, RigidCellularBodyState, ScenePhysicsWorld, SceneSimulationConfiguration,
    ThermalEdits, ThermalInteraction, ThermalMaterialTable,
};
use crate::{
    actors::{Actor, ActorRegistry},
    chunks::{Chunk, ChunkEntry, ChunkFluidParticle, ChunkGasCell, ChunkStreamingResponse},
    materials::{Material, MaterialIdentifier, MaterialRegistry},
    tiles::{CellCoordinates, CellularAppearance, Tile, TileArea, TileCoordinates, TileData},
};
use engine_compute::{Accelerator, AcceleratorBuffer};
use engine_graphics::{MaterialGraphics, SceneGraphics};
use rapier2d::prelude::Vector;
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    error::Error,
    future::poll_fn,
    io,
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    time::Duration,
};

/// The capacity of the chunk streaming queue
pub const CHUNK_STREAMING_QUEUE_CAPACITY: usize = 64;

/// The fixed scene tick rate
const TICK_RATE: u32 = 60;
const MAX_CATCH_UP_TICKS: u32 = 4;

const RIGID_DETACHMENT_MAXIMUM_CELLS: usize = 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum RigidCellRemovalCause {
    Erase,
    Fracture,
}

/// A scene that can be simulated by the engine
pub struct Scene {
    /// The `Accelerator` this `Scene` is running on
    accelerator: Arc<Accelerator>,
    /// The persistent `SceneData` backing this `Scene`
    data: SceneData,
    /// The GPU graphics properties derived from the scene's material registry
    material_graphics: MaterialGraphics,
    /// The `SceneGenerator` used to generate new tiles for this `Scene`
    generator: Arc<dyn SceneGenerator>,
    /// The `ActorRegistry` currently managed by this `Scene`
    actor_registry: ActorRegistry,
    /// The actor currently receiving player control, if any
    possessed_actor: Option<Actor>,
    /// Chunks in this scene indexed by their `TilePosition`s
    chunks: HashMap<TileCoordinates, ChunkEntry>,
    /// The sender used by chunk streaming threads to return streamed chunks
    chunk_streaming_response_sender: SyncSender<ChunkStreamingResponse>,
    /// The chunks that are being streamed into the `Scene`
    chunk_streaming_responses: Receiver<ChunkStreamingResponse>,
    /// The next identifier to be used for streaming chunks
    chunks_streaming_identifier_next: u64,
    /// Elapsed time not yet consumed by fixed ticks
    tick_time: Duration,
    /// Mandatory world-space edits submitted by editor and gameplay producers.
    pending_runtime_edits: SceneEditBatch,
    /// The width of the active tile area
    simulation_width: u16,
    /// The height of the active tile area
    simulation_height: u16,
    /// The size of the GPU tile buffer outside the active area
    simulation_buffer_size: u8,
    /// The `TilePosition` of the tile in `tiles` that is furthest to the left and bottom
    origin: TileCoordinates,
    /// The desired `origin` for the active tile area
    origin_target: TileCoordinates,
    /// An explicit area-follow request to apply before automatic pawn following
    area_request: Option<TileCoordinates>,
    /// The current GPU-resident tiles in this `Scene`
    tiles: Box<[Tile]>,
    /// The streaming batch size for tiles
    tile_streaming_batch_size: u8,
    /// The tile downloads pending processing by `tick`
    tile_downloads: Mutex<Vec<Arc<Mutex<TileDownload>>>>,
    /// Mandatory outgoing downloads awaiting application to their CPU chunks
    outgoing_tile_downloads: Vec<Arc<Mutex<TileDownload>>>,
    /// Fluid exports that own particles until asynchronous readback reaches their CPU chunks
    fluid_downloads: Vec<Arc<Mutex<FluidDownload>>>,
    /// Completed fluid-export staging storage available for reuse
    fluid_download_pool: Vec<Arc<Mutex<FluidDownload>>>,
    /// Gas exports that own outgoing fields until asynchronous readback reaches CPU chunks
    gas_downloads: Vec<Arc<Mutex<GasDownload>>>,
    /// Completed gas-export staging storage available for reuse
    gas_download_pool: Vec<Arc<Mutex<GasDownload>>>,
    /// The tile uploads pending processing by `tick`
    tile_uploads: Mutex<Vec<Arc<Mutex<TileUpload>>>>,
    /// Fluid imports that own dormant records until GPU reconstruction is confirmed
    fluid_uploads: Vec<Arc<Mutex<FluidUpload>>>,
    /// Fixed staging storage for the possessed pawn's asynchronous derived-fluid sample
    fluid_sample_buffer: wgpu::Buffer,
    /// Completed derived-fluid sample or readback failure
    fluid_sample_result: Arc<Mutex<Option<Result<[f32; 5], String>>>>,
    /// Actor for which the in-flight derived-fluid sample was generated
    fluid_sample_actor: Option<Actor>,
    /// The physical X slot containing the buffered area's leftmost tile
    tiles_ring_offset_x: u16,
    /// The physical Y slot containing the buffered area's bottommost tile
    tiles_ring_offset_y: u16,
    /// The buffer containing `MaterialIdentifier`s for GPU-resident tiles
    cellular_material_identifiers: AcceleratorBuffer,
    /// The parallel buffer containing persistent cell appearance samples
    cellular_appearances: AcceleratorBuffer,
    /// The parallel buffer containing persistent static-cell integrity
    cellular_integrities: AcceleratorBuffer,
    cellular_amounts: AcceleratorBuffer,
    cellular_temperatures: AcceleratorBuffer,
    /// Ambient fallback for authored matter without a material temperature default.
    ambient_temperature: f32,
    /// Fixed-capacity persistent integrity for authoritative rigid cells.
    rigid_cell_integrities: AcceleratorBuffer,
    /// Fixed-capacity persistent material inventory for authoritative rigid cells.
    rigid_cell_amounts: AcceleratorBuffer,
    /// Fixed-capacity persistent temperature for authoritative rigid cells.
    rigid_cell_temperatures: AcceleratorBuffer,
    /// Transient rasterized possessed-pawn interaction geometry
    cellular_physics_body_proxy: CellularPhysicsBodyProxy,
    /// Authoritative body-local cellular matter paired with Rapier bodies
    rigid_cellular_bodies: Vec<RigidCellularBody>,
    /// Generation protects a delayed raster result from a recycled slot.
    rigid_cell_state_generations: Vec<u32>,
    rigid_cell_state_free: Vec<u32>,
    /// Changes whenever rigid body-local topology changes
    rigid_cellular_topology_revision: u64,
    /// Last asynchronously confirmed cellular contact state per rigid vector index
    rigid_cellular_contact_active: Vec<bool>,
    rigid_cellular_support: Vec<[f32; 4]>,
    rigid_cellular_recovery: Vec<[f32; 4]>,
    /// Last asynchronously confirmed granular contact state per rigid vector index
    rigid_granular_contact_active: Vec<bool>,
    /// Prior static snapshot used to ignore initial islands and detect topology changes
    rigid_detachment_snapshot: Option<CollisionOccupancySnapshot>,
    /// GPU-authoritative fluid particles and their transient cellular representation
    fluids: Fluids,
    /// GPU-authoritative shared gas velocity and per-species concentrations
    gases: Gases,
    /// GPU-resident cross-form material replacement requests.
    material_mutations: MaterialMutations,
    thermal_edits: ThermalEdits,
    thermal_material_table: ThermalMaterialTable,
    thermal_interaction: ThermalInteraction,
    /// GPU simulation of dynamic cells in the canonical cellular buffers
    cellular_dynamic: CellularDynamic,
    /// GPU impulse, pressure, integrity, and fracture subsystem
    cellular_pressure: CellularPressure,
    /// Compact CPU-readable occupancy derived from the authoritative cellular GPU buffer
    cellular_collision: CellularCollision,
    /// Whether cellular data or its ring mapping needs a replacement collision extraction
    cellular_collision_dirty: bool,
    /// Scene gravity acceleration in tiles per second squared
    gravity: [f32; 2],
    /// Rapier rigid-body world plus the CPU-readable actor collision snapshot
    physics_world: ScenePhysicsWorld,
}

impl Scene {
    /// Creates a temporary `Scene`, its buffered GPU storage, and every initial chunk.
    ///
    /// The initial streaming area is synchronously loaded from disk or generated so the returned
    /// scene has data for its active area and its non-simulated GPU buffer. Tile uploads are
    /// queued here and submitted by the first `tick`.
    pub fn new(
        accelerator: &Arc<Accelerator>,
        materials: MaterialRegistry,
        simulation: SceneSimulationConfiguration,
    ) -> Result<Self, Box<dyn Error>> {
        Self::load(
            accelerator,
            simulation,
            SceneData::new_temporary(materials)?,
        )
    }

    /// Creates a temporary `Scene` using a generator for chunks that are not already stored
    pub fn new_with_generator(
        accelerator: &Arc<Accelerator>,
        materials: MaterialRegistry,
        simulation: SceneSimulationConfiguration,
        generator: impl SceneGenerator + 'static,
    ) -> Result<Self, Box<dyn Error>> {
        Self::load_with_generator(
            accelerator,
            simulation,
            SceneData::new_temporary(materials)?,
            generator,
        )
    }

    /// Loads a `Scene` from existing `SceneData`
    pub fn load(
        accelerator: &Arc<Accelerator>,
        simulation: SceneSimulationConfiguration,
        data: SceneData,
    ) -> Result<Self, Box<dyn Error>> {
        Self::load_with_generator(accelerator, simulation, data, ())
    }

    /// Loads a `Scene` from existing `SceneData`, generating chunks that are not stored
    pub fn load_with_generator(
        accelerator: &Arc<Accelerator>,
        simulation: SceneSimulationConfiguration,
        data: SceneData,
        generator: impl SceneGenerator + 'static,
    ) -> Result<Self, Box<dyn Error>> {
        simulation.validate()?;
        let accelerator: Arc<Accelerator> = accelerator.clone();
        let material_graphics: MaterialGraphics = data
            .materials()
            .build_material_graphics(accelerator.as_ref());
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
        let cellular_amounts = accelerator.allocate::<f32>(buffered_cell_count);
        let cellular_temperatures = accelerator.allocate::<f32>(buffered_cell_count);
        let rigid_cell_integrities: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count);
        let rigid_cell_amounts: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count);
        let rigid_cell_temperatures: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count);
        let cellular_physics_body_proxy =
            CellularPhysicsBodyProxy::new(accelerator.as_ref(), buffered_cell_count);
        let thermal_material_table =
            ThermalMaterialTable::new(accelerator.as_ref(), data.materials());
        let fluids: Fluids = Fluids::new(
            accelerator.as_ref(),
            &cellular_material_identifiers,
            cellular_physics_body_proxy.occupancy_buffer(),
            cellular_physics_body_proxy.velocity_buffer(),
            &material_graphics.fluid_properties,
            thermal_material_table.properties_buffer(),
            thermal_material_table.parameters_buffer(),
            simulation.width + buffer_size,
            simulation.height + buffer_size,
        );
        let gases: Gases = Gases::new(
            accelerator.as_ref(),
            data.materials(),
            &cellular_material_identifiers,
            cellular_physics_body_proxy.occupancy_buffer(),
            fluids.coverage_buffer(),
            &material_graphics.gas_properties,
            buffered_cell_count,
        );
        let ambient_gas_temperature =
            vec![simulation.ambient_temperature.to_bits().to_le_bytes(); buffered_cell_count]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
        accelerator.wgpu_queue().write_buffer(
            gases.temperature_buffer().wgpu_buffer(),
            0,
            &ambient_gas_temperature,
        );
        let thermal_interaction = ThermalInteraction::new(
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
            gases.concentrations_buffer(),
            gases.temperature_buffer(),
            thermal_material_table.properties_buffer(),
            thermal_material_table.parameters_buffer(),
            cellular_physics_body_proxy.occupancy_buffer(),
            buffered_cell_count as u32,
            buffered_cell_count as u32,
            gases.gas_count(),
            simulation.ambient_temperature,
            simulation.empty_space_thermal_conductivity,
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
        let material_mutations = MaterialMutations::new(
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
            fluids.gpu_edits_pending_buffer(),
            gases.velocity_buffer(),
            gases.concentrations_buffer(),
            gases.temperature_buffer(),
            buffered_cell_count,
            gases.gas_count(),
        );
        let thermal_edits = ThermalEdits::new(
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
            &material_graphics.gas_properties,
            fluids.coverage_buffer(),
            material_mutations.requests_buffer(),
            material_mutations.request_count_buffer(),
            gases.gas_count(),
            buffered_cell_count,
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
        let (chunk_streaming_response_sender, chunk_streaming_responses) =
            sync_channel(CHUNK_STREAMING_QUEUE_CAPACITY);
        let mut scene: Self = Self {
            accelerator,
            data,
            material_graphics,
            generator,
            actor_registry: ActorRegistry::new(),
            possessed_actor: None,
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
            cellular_physics_body_proxy,
            rigid_cellular_bodies: Vec::new(),
            rigid_cell_state_generations: vec![0; buffered_cell_count],
            rigid_cell_state_free: (0..buffered_cell_count as u32).rev().collect(),
            rigid_cellular_topology_revision: 0,
            rigid_cellular_contact_active: Vec::new(),
            rigid_cellular_support: Vec::new(),
            rigid_cellular_recovery: Vec::new(),
            rigid_granular_contact_active: Vec::new(),
            rigid_detachment_snapshot: None,
            fluids,
            gases,
            material_mutations,
            thermal_edits,
            thermal_material_table,
            thermal_interaction,
            cellular_dynamic,
            cellular_pressure,
            cellular_collision,
            cellular_collision_dirty: true,
            gravity: simulation.gravity,
            physics_world: ScenePhysicsWorld::new(),
        };
        for coordinates in scene.area_streaming().iterate_chunk_coordinates() {
            let chunk: Chunk = match scene.data.read_chunk(coordinates)? {
                Some(chunk) => chunk,
                None => scene.generator.generate_chunk(coordinates),
            };
            scene.chunks.insert(
                coordinates,
                ChunkEntry::Active {
                    chunk,
                    is_dirty: false,
                },
            );
        }
        drop(scene.tiles_upload(scene.area_buffered()));
        scene.fluid_uploads_queue(scene.area_buffered())?;
        let buffered_area: TileArea = scene.area_buffered();
        scene.gas_clear_area(buffered_area);
        scene.gas_upload_area(buffered_area)?;
        Ok(scene)
    }

    /// Sets the `SceneGenerator` of this `Scene` that will be used for generating new chunks
    pub fn with_generator(mut self, generator: impl SceneGenerator + 'static) -> Self {
        self.generator = Arc::new(generator);
        self
    }

    /// Returns graphics information for this scene
    pub fn graphics(&self) -> SceneGraphics<'_> {
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let dimensions: u32 = u32::from(self.simulation_buffer_size) * 2;
        let walking_pawn: Option<([f32; 2], [f32; 2])> = self
            .actor_registry
            .first_walking_pawn_graphics(self.tick_interpolation());
        SceneGraphics {
            material_graphics: &self.material_graphics,
            cellular_material_identifiers: &self.cellular_material_identifiers,
            cellular_appearances: &self.cellular_appearances,
            rigid_material_identifiers: self
                .cellular_physics_body_proxy
                .rigid_material_identifiers_buffer(),
            rigid_appearances: self.cellular_physics_body_proxy.rigid_appearances_buffer(),
            fluid_material_identifiers: self.fluids.material_identifiers_buffer(),
            fluid_coverage: self.fluids.coverage_buffer(),
            gas_concentrations: self.gases.concentrations_buffer(),
            gas_count: self.gases.gas_count(),
            cellular_pressure: self.cellular_pressure.retained_pressure(),
            buffered_origin: [self.origin.x - buffer_size, self.origin.y - buffer_size],
            buffered_tile_size: [
                u32::from(self.simulation_width) + dimensions,
                u32::from(self.simulation_height) + dimensions,
            ],
            ring_offset: [
                u32::from(self.tiles_ring_offset_x),
                u32::from(self.tiles_ring_offset_y),
            ],
            walking_pawn,
        }
    }

    /// Returns the materials registered for this scene
    pub fn materials(&self) -> &MaterialRegistry {
        self.data.materials()
    }

    /// Returns the `ActorRegistry` for this `Scene`
    pub const fn actor_registry(&self) -> &ActorRegistry {
        &self.actor_registry
    }

    /// Returns mutable access to the `ActorRegistry` for this `Scene`
    pub const fn actor_registry_mutable(&mut self) -> &mut ActorRegistry {
        &mut self.actor_registry
    }

    /// Returns the currently possessed actor if one exists
    pub fn possessed_actor(&self) -> Option<Actor> {
        self.possessed_actor
            .filter(|actor| self.actor_registry.contains(*actor))
    }

    /// Returns an actor's position interpolated between its latest fixed ticks
    pub fn actor_render_position(&self, actor: Actor) -> Option<ScenePosition> {
        self.actor_registry
            .get_render_position(actor, self.tick_interpolation())
    }

    /// Possesses an actor if it exists in this `Scene`
    pub fn possess_actor(&mut self, identifier: Actor) -> bool {
        if !self.actor_registry.is_possessable(identifier) {
            return false;
        }
        if self.possessed_actor != Some(identifier)
            && let Some(possessed) = self.possessed_actor
        {
            self.actor_registry.clear_control_state(possessed);
        }
        self.possessed_actor = Some(identifier);
        true
    }

    /// Releases the currently possessed actor
    pub fn dispossess_actor(&mut self) {
        if let Some(possessed) = self.possessed_actor.take() {
            self.actor_registry.clear_control_state(possessed);
        }
    }

    /// Requests that the active scene area recenter around a world position
    pub fn request_area_around(&mut self, position: ScenePosition) {
        self.area_request = Some(TileCoordinates {
            x: position.tile_coordinates.x - i32::from(self.simulation_width) / 2,
            y: position.tile_coordinates.y - i32::from(self.simulation_height) / 2,
        });
    }

    /// Returns whether a world position is currently resident in the GPU tile buffer
    pub fn is_position_resident(&self, position: ScenePosition) -> bool {
        self.area_buffered().contains(position.tile_coordinates)
    }

    /// Queues mandatory runtime material edits for one coalesced update-time flush.
    pub fn queue_edits(&mut self, edits: SceneEditBatch) {
        self.pending_runtime_edits.append(edits);
    }

    /// Applies a scene-owned transaction immediately; nonresident requests remain queued.
    fn apply_edits_immediate(&mut self, edits: &mut SceneEditBatch) -> Result<(), io::Error> {
        let mut cell_edits: HashMap<
            usize,
            (CellCoordinates, MaterialIdentifier, CellularAppearance, f32),
        > = HashMap::new();
        let mut fluid_edits: HashMap<usize, u32> = HashMap::new();
        let mut gas_edits: HashSet<(usize, u32)> = HashSet::new();
        let mut gas_clear_cells: HashSet<usize> = HashSet::new();
        let mut rigid_destroy_indices = Vec::new();
        let mut deferred = SceneEditBatch::new();
        for edit in edits.drain() {
            match edit {
                SceneEdit::PlaceRigidBody { cells } => {
                    if cells.is_empty() {
                        continue;
                    }
                    if !cells
                        .iter()
                        .all(|cell| self.cell_edit_index(cell.coordinates).is_some())
                    {
                        deferred.place_rigid_body(cells);
                    } else {
                        self.insert_authored_rigid_cellular_body(cells);
                    }
                }
                SceneEdit::PlaceCells { cells } => {
                    for SceneEditCellPlacement {
                        coordinates,
                        material_identifier,
                        appearance,
                    } in cells
                    {
                        if let Some(physical_index) = self.cell_edit_index(coordinates) {
                            match self.data.materials().get(material_identifier) {
                                Some(Material::CellularStatic {
                                    default_integrity, ..
                                }) => {
                                    cell_edits.insert(
                                        physical_index,
                                        (
                                            coordinates,
                                            material_identifier,
                                            appearance,
                                            *default_integrity,
                                        ),
                                    );
                                    fluid_edits.insert(physical_index, Fluids::erase_edit());
                                    if self.gases.gas_count() != 0 {
                                        gas_clear_cells.insert(physical_index);
                                        for species in 0..self.gases.gas_count() {
                                            gas_edits.remove(&(physical_index, species));
                                        }
                                    }
                                }
                                Some(Material::CellularDynamic { .. }) => {
                                    cell_edits.insert(
                                        physical_index,
                                        (coordinates, material_identifier, appearance, 0.0),
                                    );
                                    fluid_edits.insert(physical_index, Fluids::erase_edit());
                                    if self.gases.gas_count() != 0 {
                                        gas_clear_cells.insert(physical_index);
                                        for species in 0..self.gases.gas_count() {
                                            gas_edits.remove(&(physical_index, species));
                                        }
                                    }
                                }
                                Some(Material::Fluid { .. }) => {
                                    cell_edits.insert(
                                        physical_index,
                                        (
                                            coordinates,
                                            MaterialIdentifier::NULL,
                                            CellularAppearance::NEUTRAL,
                                            0.0,
                                        ),
                                    );
                                    fluid_edits
                                        .insert(physical_index, material_identifier.as_u32());
                                    if self.gases.gas_count() != 0 {
                                        gas_clear_cells.insert(physical_index);
                                        for species in 0..self.gases.gas_count() {
                                            gas_edits.remove(&(physical_index, species));
                                        }
                                    }
                                }
                                Some(Material::Gas { .. }) => {
                                    gas_edits.insert((physical_index, material_identifier.index()));
                                }
                                None => {}
                            }
                        } else {
                            deferred.place_cells(vec![SceneEditCellPlacement {
                                coordinates,
                                material_identifier,
                                appearance,
                            }]);
                        }
                    }
                }
                SceneEdit::Erase { cells } => {
                    for coordinates in cells {
                        if let Some(physical_index) = self.cell_edit_index(coordinates) {
                            rigid_destroy_indices.push(physical_index);
                            cell_edits.insert(
                                physical_index,
                                (
                                    coordinates,
                                    MaterialIdentifier::NULL,
                                    CellularAppearance::NEUTRAL,
                                    0.0,
                                ),
                            );
                            fluid_edits.insert(physical_index, Fluids::erase_edit());
                            if self.gases.gas_count() != 0 {
                                gas_clear_cells.insert(physical_index);
                                for species in 0..self.gases.gas_count() {
                                    gas_edits.remove(&(physical_index, species));
                                }
                            }
                        } else {
                            deferred.erase(vec![coordinates]);
                        }
                    }
                }
                SceneEdit::DestroyCells { cells } => {
                    for coordinates in cells {
                        if let Some(physical_index) = self.cell_edit_index(coordinates) {
                            rigid_destroy_indices.push(physical_index);
                            cell_edits.insert(
                                physical_index,
                                (
                                    coordinates,
                                    MaterialIdentifier::NULL,
                                    CellularAppearance::NEUTRAL,
                                    0.0,
                                ),
                            );
                            fluid_edits.insert(physical_index, Fluids::erase_edit());
                            if self.gases.gas_count() != 0 {
                                gas_clear_cells.insert(physical_index);
                                for species in 0..self.gases.gas_count() {
                                    gas_edits.remove(&(physical_index, species));
                                }
                            }
                        } else {
                            deferred.destroy_cells(vec![coordinates]);
                        }
                    }
                }
                SceneEdit::Thermal {
                    cells,
                    delta_temperature,
                } => {
                    let mut physical_indices = Vec::new();
                    for coordinates in cells {
                        if let Some(index) = self.cell_edit_index(coordinates) {
                            physical_indices.push(index as u32);
                        } else {
                            deferred.thermal(vec![coordinates], delta_temperature);
                        }
                    }
                    physical_indices.sort_unstable();
                    physical_indices.dedup();
                    self.thermal_edits.apply(
                        self.accelerator.as_ref(),
                        &physical_indices,
                        delta_temperature,
                        [
                            self.origin.x - i32::from(self.simulation_buffer_size),
                            self.origin.y - i32::from(self.simulation_buffer_size),
                        ],
                        [
                            u32::from(
                                self.simulation_width + u16::from(self.simulation_buffer_size) * 2,
                            ),
                            u32::from(
                                self.simulation_height + u16::from(self.simulation_buffer_size) * 2,
                            ),
                        ],
                        [
                            u32::from(self.tiles_ring_offset_x),
                            u32::from(self.tiles_ring_offset_y),
                        ],
                    );
                }
            }
        }
        edits.append(deferred);
        if !rigid_destroy_indices.is_empty() {
            rigid_destroy_indices.sort_unstable();
            rigid_destroy_indices.dedup();
            self.cellular_physics_body_proxy
                .resolve_destruction_requests(self.accelerator.as_ref(), &rigid_destroy_indices);
        }
        let mut cell_edits: Vec<(
            usize,
            CellCoordinates,
            MaterialIdentifier,
            CellularAppearance,
            f32,
        )> = cell_edits
            .into_iter()
            .map(
                |(index, (coordinates, material_identifier, appearance, integrity))| {
                    (
                        index,
                        coordinates,
                        material_identifier,
                        appearance,
                        integrity,
                    )
                },
            )
            .collect();
        cell_edits.sort_unstable_by_key(|(physical_index, ..)| *physical_index);
        for (_, coordinates, material_identifier, appearance, integrity) in &cell_edits {
            let tile_coordinates: TileCoordinates = coordinates.tile_coordinates();
            let [x, y]: [usize; 2] = coordinates.local_tile_coordinates();
            let Some(ChunkEntry::Active { chunk, is_dirty }) =
                self.chunks.get_mut(&tile_coordinates.chunk_coordinates())
            else {
                return Err(io::Error::other("Resident tile chunk is not active"));
            };
            chunk
                .set_cell_with_integrity(
                    tile_coordinates,
                    x,
                    y,
                    *material_identifier,
                    *appearance,
                    *integrity,
                )
                .map_err(|_| io::Error::other("Resident tile is not in its active chunk"))?;
            *is_dirty = true;
        }
        if !cell_edits.is_empty() {
            self.cellular_collision_dirty = true;
            self.write_cell_edits(&cell_edits);
        }
        if !fluid_edits.is_empty() {
            let mut fluid_edits: Vec<(usize, u32, f32, f32)> = fluid_edits
                .into_iter()
                .map(|(index, material)| {
                    let identifier = MaterialIdentifier::from_u32(material);
                    let temperature = if identifier == MaterialIdentifier::NULL {
                        0.0
                    } else {
                        self.initial_temperature(identifier)
                    };
                    (index, material, 1.0, temperature)
                })
                .collect();
            fluid_edits.sort_unstable_by_key(|(physical_index, ..)| *physical_index);
            let buffer_size: i32 = i32::from(self.simulation_buffer_size);
            let dimensions: u16 = u16::from(self.simulation_buffer_size) * 2;
            let fluid_active_area: TileArea = self.area_fluid_active();
            let fluid_active_dimensions: [u16; 2] = fluid_active_area.dimensions();
            self.fluids.apply_edits(
                self.accelerator.as_ref(),
                &fluid_edits,
                fluid_active_area.origin(),
                fluid_active_dimensions[0],
                fluid_active_dimensions[1],
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            );
        }
        if !gas_edits.is_empty() || !gas_clear_cells.is_empty() {
            let mut gas_edits: Vec<(usize, u32)> = gas_edits.into_iter().collect();
            gas_edits.sort_unstable();
            let mut gas_clear_cells: Vec<usize> = gas_clear_cells.into_iter().collect();
            gas_clear_cells.sort_unstable();
            self.gases
                .apply_edits(self.accelerator.as_ref(), &gas_edits, &gas_clear_cells);
        }
        Ok(())
    }

    /// Applies one GPU radial cellular impulse without moving cells immediately.
    pub fn apply_cellular_radial_impulse(
        &mut self,
        center: CellCoordinates,
        radius_cells: f32,
        strength: f32,
    ) {
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let dimensions: u16 = u16::from(self.simulation_buffer_size) * 2;
        self.cellular_pressure.apply_radial_impulse(
            self.accelerator.as_ref(),
            TileCoordinates {
                x: self.origin.x - buffer_size,
                y: self.origin.y - buffer_size,
            },
            self.simulation_width + dimensions,
            self.simulation_height + dimensions,
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
            center,
            radius_cells.max(0.75),
            strength,
        );
    }

    /// Handles Scene streaming and returns the number of completed fixed-rate ticks
    pub fn update(
        &mut self,
        elapsed: Duration,
        is_simulation_active: bool,
    ) -> Result<u32, io::Error> {
        if let Some(origin_target) = self.area_request.take() {
            self.origin_target = origin_target;
        } else {
            let possessed_position: Option<ScenePosition> = self
                .possessed_actor()
                .and_then(|actor| self.actor_registry.get_position(actor))
                .copied();
            if let Some(position) = possessed_position {
                self.follow_position(position);
            }
        }
        self.chunks_refresh()?;
        self.tile_downloads_submit()?;
        self.tile_uploads_submit()?;
        self.fluid_downloads_submit()?;
        self.fluid_uploads_submit()?;
        self.gas_downloads_submit()?;
        if !self.pending_runtime_edits.is_empty() {
            let mut edits = SceneEditBatch::new();
            std::mem::swap(&mut edits, &mut self.pending_runtime_edits);
            self.apply_edits_immediate(&mut edits)?;
            self.pending_runtime_edits.append(edits);
        }
        self.accelerator
            .poll()
            .map_err(|error| io::Error::other(error.to_string()))?;
        self.apply_completed_rigid_cellular_reactions()?;
        self.cellular_collision.collect_collision()?;
        self.tile_downloads_apply_completed()?;
        self.fluid_downloads_apply_completed()?;
        self.fluid_uploads_apply_completed()?;
        self.gas_downloads_apply_completed()?;
        self.fluid_sample_apply_completed()?;
        self.tile_downloads_clean()?;
        self.tile_uploads_clean()?;
        self.tick_time += elapsed;
        let tick_time: Duration = Duration::from_secs(1) / TICK_RATE;
        self.tick_time = self
            .tick_time
            .min(tick_time.saturating_mul(MAX_CATCH_UP_TICKS));
        let mut ticks: u32 = 0;
        while self.tick_time >= tick_time && ticks < MAX_CATCH_UP_TICKS {
            // A catch-up update may submit several fixed ticks. Give tiny reaction
            // readbacks a nonblocking chance to complete between them so each
            // reaction is applied as its own fixed-tick batch.
            self.accelerator
                .poll()
                .map_err(|error| io::Error::other(error.to_string()))?;
            self.apply_completed_rigid_cellular_reactions()?;
            self.tick(is_simulation_active)?;
            self.tick_time -= tick_time;
            ticks = ticks.saturating_add(1);
        }
        if ticks == MAX_CATCH_UP_TICKS && self.tick_time >= tick_time {
            self.tick_time = tick_time.saturating_sub(Duration::from_nanos(1));
        }
        Ok(ticks)
    }

    /// Returns progress from the previous fixed tick to the current fixed tick
    fn tick_interpolation(&self) -> f32 {
        (self.tick_time.as_secs_f32() * TICK_RATE as f32).clamp(0.0, 1.0)
    }

    /// Applies every compatible completed GPU reaction in submission order
    fn apply_completed_rigid_cellular_reactions(&mut self) -> Result<(), io::Error> {
        let destroyed: HashSet<[u32; 2]> = self
            .cellular_physics_body_proxy
            .take_destroyed_handles()
            .into_iter()
            .filter(|handle| handle[0] != u32::MAX)
            .collect();
        if !destroyed.is_empty() {
            let mut removals: Vec<(usize, HashSet<[i32; 2]>)> = self
                .rigid_cellular_bodies
                .iter()
                .enumerate()
                .filter_map(|(body, rigid)| {
                    let cells = rigid
                        .cells
                        .iter()
                        .filter(|cell| {
                            destroyed.contains(&[cell.state_slot, cell.state_generation])
                        })
                        .map(|cell| cell.local)
                        .collect::<HashSet<_>>();
                    (!cells.is_empty()).then_some((body, cells))
                })
                .collect();
            removals.sort_unstable_by_key(|(body, _)| std::cmp::Reverse(*body));
            for (body, cells) in removals {
                self.remove_rigid_cellular_body_cells(body, &cells, RigidCellRemovalCause::Erase);
            }
        }
        let body_count: usize = self.rigid_cellular_bodies.len();
        let mut newest = None;
        for batch in self.cellular_pressure.collect_rigid_reactions()? {
            if batch.topology_revision != self.rigid_cellular_topology_revision
                || batch.body_count != body_count
            {
                continue;
            }
            for index in 0..batch.body_count {
                let reaction = batch.reactions[index];
                let granular_contacts = batch.granular_contact_counts[index] != 0;
                let contacts = granular_contacts || batch.contact_counts[index] != 0;
                let wake = contacts != self.rigid_cellular_contact_active[index]
                    || batch.moving_contact_counts[index] != 0;
                if !self.physics_world.apply_rigid_cellular_body_reaction(
                    &self.rigid_cellular_bodies[index],
                    [reaction[0], reaction[1]],
                    reaction[2],
                    batch.energy_budgets[index],
                    wake,
                ) {
                    return Err(io::Error::other("Rigid cellular body handle is missing"));
                }
            }
            newest = Some(batch);
        }
        if let Some(batch) = newest {
            for index in 0..body_count {
                let contacts = batch.contact_counts[index] != 0;
                let wake = contacts != self.rigid_cellular_contact_active[index]
                    || batch.moving_contact_counts[index] != 0;
                self.physics_world.apply_rigid_constraint(
                    &self.rigid_cellular_bodies[index],
                    batch.constraints[index],
                    batch.source_motion[index],
                    wake,
                );
                self.rigid_cellular_contact_active[index] = contacts;
                self.rigid_granular_contact_active[index] =
                    batch.granular_contact_counts[index] != 0;
            }
            if !batch.fractured_slots.is_empty() {
                let fractured: HashSet<u32> = batch.fractured_slots.iter().copied().collect();
                let mut removals: Vec<(usize, HashSet<[i32; 2]>)> = self
                    .rigid_cellular_bodies
                    .iter()
                    .enumerate()
                    .filter_map(|(body, rigid)| {
                        let cells: HashSet<[i32; 2]> = rigid
                            .cells
                            .iter()
                            .filter(|cell| fractured.contains(&cell.state_slot))
                            .map(|cell| cell.local)
                            .collect();
                        (!cells.is_empty()).then_some((body, cells))
                    })
                    .collect();
                removals.sort_unstable_by_key(|(body, _)| std::cmp::Reverse(*body));
                for (body, cells) in removals {
                    self.remove_rigid_cellular_body_cells(
                        body,
                        &cells,
                        RigidCellRemovalCause::Fracture,
                    );
                }
            }
        }
        Ok(())
    }

    /// Runs one fixed-rate physics simulation tick
    fn tick(&mut self, is_simulation_active: bool) -> Result<(), io::Error> {
        if let Some(mut snapshot) = self.cellular_collision.latest.take() {
            let age = self.cellular_collision.snapshot_age(snapshot.sequence);
            self.physics_world.set_collision_snapshot_age(age);
            self.detach_unanchored_static_components(&mut snapshot)?;
            self.physics_world.update_cellular_snapshot(snapshot);
        }
        let delta_time: f32 = 1.0 / TICK_RATE as f32;
        let actor_proxies = self.actor_registry.cellular_proxy_states();
        if is_simulation_active {
            let up = if self.gravity[0].hypot(self.gravity[1]) > 0.0 {
                Vector::new(-self.gravity[0], -self.gravity[1]).normalize()
            } else {
                Vector::Y
            };
            self.physics_world
                .sync_pawn_proxies(&self.actor_registry.physics_proxy_states(), up);
            self.physics_world.prepare_cellular_terrain(
                &self.rigid_cellular_bodies,
                &actor_proxies,
                self.gravity,
                delta_time,
            );
            self.physics_world.step(self.gravity, delta_time);
        }
        self.actor_registry.simulate_actor_pawns(
            1.0 / TICK_RATE as f32,
            is_simulation_active,
            self.gravity,
            &self.physics_world,
        );
        let up = if self.gravity[0].hypot(self.gravity[1]) > 0.0 {
            Vector::new(-self.gravity[0], -self.gravity[1]).normalize()
        } else {
            Vector::Y
        };
        self.physics_world
            .sync_pawn_proxies(&self.actor_registry.physics_proxy_states(), up);
        let actor_proxies = self.actor_registry.cellular_proxy_states();
        let possessed_position: Option<ScenePosition> = self
            .possessed_actor()
            .and_then(|actor| self.actor_registry.get_position(actor))
            .copied();
        if let Some(position) = possessed_position {
            self.follow_position(position);
        }
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let dimensions: u16 = u16::from(self.simulation_buffer_size) * 2;
        let rigid_body_states: Vec<RigidCellularBodyState> = self
            .rigid_cellular_bodies
            .iter()
            .map(|body| {
                self.physics_world
                    .rigid_cellular_body_state(body)
                    .ok_or_else(|| io::Error::other("Rigid cellular body handle is missing"))
            })
            .collect::<Result<_, _>>()?;
        self.cellular_physics_body_proxy.rasterize(
            self.accelerator.as_ref(),
            TileCoordinates {
                x: self.origin.x - buffer_size,
                y: self.origin.y - buffer_size,
            },
            self.simulation_width + dimensions,
            self.simulation_height + dimensions,
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
            self.gravity,
            &actor_proxies,
            &self.rigid_cellular_bodies,
            &rigid_body_states,
            self.rigid_cellular_topology_revision,
        );
        if is_simulation_active {
            let fluid_active_area: TileArea = self.area_fluid_active();
            let fluid_active_dimensions: [u16; 2] = fluid_active_area.dimensions();
            self.fluids.simulate(
                self.accelerator.as_ref(),
                fluid_active_area.origin(),
                fluid_active_dimensions[0],
                fluid_active_dimensions[1],
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
                self.gravity,
                1.0 / TICK_RATE as f32,
            );
            self.gases.simulate_pre_coupling(
                self.accelerator.as_ref(),
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
                self.gravity,
                1.0 / TICK_RATE as f32,
            );
            self.material_mutations.reset(self.accelerator.as_ref());
            self.cellular_pressure.simulate(
                self.accelerator.as_ref(),
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
                1.0 / TICK_RATE as f32,
                self.gravity,
                self.rigid_cellular_bodies.len(),
                self.cellular_physics_body_proxy
                    .rigid_cell_count(&self.rigid_cellular_bodies),
                self.rigid_cellular_topology_revision,
            )?;
            self.material_mutations.resolve(
                self.accelerator.as_ref(),
                u32::from(self.simulation_width + dimensions)
                    * u32::from(self.simulation_height + dimensions)
                    * 64,
                self.gases.gas_count(),
            );
            self.fluids.consume_gpu_edits(
                self.accelerator.as_ref(),
                fluid_active_area.origin(),
                fluid_active_dimensions[0],
                fluid_active_dimensions[1],
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            );
            self.cellular_dynamic.simulate_cellular_dynamic_tick(
                self.accelerator.as_ref(),
                &self.cellular_material_identifiers,
                &self.cellular_appearances,
                &self.cellular_amounts,
                &self.cellular_temperatures,
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
                self.gravity,
                1.0 / TICK_RATE as f32,
            );
            self.fluids
                .scatter_mechanical_response(self.accelerator.as_ref());
            self.gases.simulate_post_coupling(self.accelerator.as_ref());
            self.thermal_interaction.gather(
                self.accelerator.as_ref(),
                [
                    self.origin.x - i32::from(self.simulation_buffer_size),
                    self.origin.y - i32::from(self.simulation_buffer_size),
                ],
                [
                    u32::from(self.simulation_width + u16::from(self.simulation_buffer_size) * 2),
                    u32::from(self.simulation_height + u16::from(self.simulation_buffer_size) * 2),
                ],
                [
                    u32::from(self.tiles_ring_offset_x),
                    u32::from(self.tiles_ring_offset_y),
                ],
            );
            self.fluid_sample_submit()?;
            self.cellular_collision_dirty = true;
        }
        if !self.cellular_collision_dirty {
            return Ok(());
        }
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        if self.cellular_collision.extract_collision(
            self.accelerator.as_ref(),
            TileCoordinates {
                x: self.origin.x - buffer_size,
                y: self.origin.y - buffer_size,
            },
            self.simulation_width + u16::from(self.simulation_buffer_size) * 2,
            self.simulation_height + u16::from(self.simulation_buffer_size) * 2,
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
        )? {
            self.cellular_collision_dirty = false;
        }
        Ok(())
    }

    /// Transfers newly disconnected static components into authoritative body-local matter
    fn detach_unanchored_static_components(
        &mut self,
        snapshot: &mut CollisionOccupancySnapshot,
    ) -> Result<(), io::Error> {
        let Some(previous) = self.rigid_detachment_snapshot.take() else {
            self.rigid_detachment_snapshot = Some(snapshot.clone());
            return Ok(());
        };
        if previous.origin != snapshot.origin
            || previous.width != snapshot.width
            || previous.height != snapshot.height
        {
            self.rigid_detachment_snapshot = Some(snapshot.clone());
            return Ok(());
        }
        if previous.static_masks == snapshot.static_masks {
            self.rigid_detachment_snapshot = Some(snapshot.clone());
            return Ok(());
        }
        let origin_x: i32 = snapshot.origin.x * 8;
        let origin_y: i32 = snapshot.origin.y * 8;
        let width: i32 = i32::from(snapshot.width) * 8;
        let height: i32 = i32::from(snapshot.height) * 8;
        let mut occupancy: Vec<u8> = vec![0; (width * height) as usize];
        for y in origin_y..origin_y + height {
            for x in origin_x..origin_x + width {
                let index: usize = ((y - origin_y) * width + x - origin_x) as usize;
                occupancy[index] = u8::from(snapshot.is_static_cell_occupied(x, y) == Some(true));
            }
        }
        let mut candidates: Vec<Vec<CellCoordinates>> = Vec::new();
        for y in origin_y..origin_y + height {
            for x in origin_x..origin_x + width {
                let index: usize = ((y - origin_y) * width + x - origin_x) as usize;
                if occupancy[index] != 1 {
                    continue;
                }
                let mut queue: VecDeque<[i32; 2]> = VecDeque::from([[x, y]]);
                let mut component: Vec<CellCoordinates> = Vec::new();
                let mut anchored: bool = false;
                occupancy[index] = 2;
                while let Some([cell_x, cell_y]) = queue.pop_front() {
                    component.push(CellCoordinates {
                        x: cell_x,
                        y: cell_y,
                    });
                    anchored |= cell_x == origin_x
                        || cell_y == origin_y
                        || cell_x == origin_x + width - 1
                        || cell_y == origin_y + height - 1;
                    for neighbor in [
                        [cell_x - 1, cell_y],
                        [cell_x + 1, cell_y],
                        [cell_x, cell_y - 1],
                        [cell_x, cell_y + 1],
                    ] {
                        if neighbor[0] < origin_x
                            || neighbor[1] < origin_y
                            || neighbor[0] >= origin_x + width
                            || neighbor[1] >= origin_y + height
                        {
                            continue;
                        }
                        let neighbor_index: usize =
                            ((neighbor[1] - origin_y) * width + neighbor[0] - origin_x) as usize;
                        if occupancy[neighbor_index] == 1 {
                            occupancy[neighbor_index] = 2;
                            queue.push_back(neighbor);
                        }
                    }
                }
                if !anchored && component.len() <= RIGID_DETACHMENT_MAXIMUM_CELLS {
                    candidates.push(component);
                }
            }
        }
        let capacity: usize = usize::from(snapshot.width) * usize::from(snapshot.height) * 64;
        for component in candidates {
            let represented: usize = self
                .rigid_cellular_bodies
                .iter()
                .map(|body| body.cells.len())
                .sum();
            if represented + component.len() > capacity {
                continue;
            }
            let minimum_x: i32 = component.iter().map(|cell| cell.x).min().unwrap();
            let minimum_y: i32 = component.iter().map(|cell| cell.y).min().unwrap();
            let mut cells = Vec::with_capacity(component.len());
            let mut integrities = Vec::with_capacity(component.len());
            let mut amounts = Vec::with_capacity(component.len());
            let mut temperatures = Vec::with_capacity(component.len());
            let mut friction: f32 = 0.0;
            let mut restitution: f32 = 0.0;
            for coordinates in &component {
                let tile_coordinates: TileCoordinates = coordinates.tile_coordinates();
                let [x, y] = coordinates.local_tile_coordinates();
                let Some(ChunkEntry::Active { chunk, .. }) =
                    self.chunks.get(&tile_coordinates.chunk_coordinates())
                else {
                    continue;
                };
                let Ok(tile) = chunk.get_tile(tile_coordinates) else {
                    continue;
                };
                let material_identifier = tile.cell_material_identifier(x, y);
                let Some(Material::CellularStatic {
                    friction: cell_friction,
                    restitution: cell_restitution,
                    ..
                }) = self.data.materials().get(material_identifier)
                else {
                    continue;
                };
                friction += *cell_friction;
                restitution += *cell_restitution;
                cells.push(RigidCellularBodyCell {
                    local: [coordinates.x - minimum_x, coordinates.y - minimum_y],
                    material: material_identifier,
                    appearance: tile.cell_appearance(x, y),
                    state_slot: u32::MAX,
                    state_generation: 0,
                });
                integrities.push(tile.cell_integrity(x, y));
                amounts.push(tile.cell_amount(x, y));
                temperatures.push(tile.cell_temperature(x, y));
            }
            if cells.len() != component.len() {
                continue;
            }
            if cells.len() < self.rigid_component_minimum(&cells) {
                let debris = cells
                    .iter()
                    .filter_map(|cell| match self.data.materials().get(cell.material) {
                        Some(Material::CellularStatic {
                            debris_material: Some(material_identifier),
                            debris_yield_rate,
                            ..
                        }) if (((cell.local[0].wrapping_mul(31).wrapping_add(cell.local[1])) as u32
                            % 10_000) as f32)
                            < *debris_yield_rate * 10_000.0 =>
                        {
                            Some(SceneEditCellPlacement {
                                coordinates: CellCoordinates {
                                    x: minimum_x + cell.local[0],
                                    y: minimum_y + cell.local[1],
                                },
                                material_identifier: *material_identifier,
                                appearance: cell.appearance,
                            })
                        }
                        _ => None,
                    })
                    .collect();
                let mut edits = SceneEditBatch::new();
                edits.erase(component.clone());
                edits.place_cells(debris);
                self.apply_edits_immediate(&mut edits)?;
                for coordinates in &component {
                    snapshot.clear_static_cell(coordinates.x, coordinates.y);
                }
                continue;
            }
            let divisor: f32 = cells.len() as f32;
            let mut edits = SceneEditBatch::new();
            edits.erase(component.clone());
            self.apply_edits_immediate(&mut edits)?;
            for coordinates in &component {
                snapshot.clear_static_cell(coordinates.x, coordinates.y);
            }
            self.insert_rigid_cellular_body(
                [minimum_x as f32 / 8.0, minimum_y as f32 / 8.0],
                cells,
                friction / divisor,
                restitution / divisor,
            );
            for (cell, (integrity, (amount, temperature))) in
                self.rigid_cellular_bodies.last().unwrap().cells.iter().zip(
                    integrities
                        .into_iter()
                        .zip(amounts.into_iter().zip(temperatures)),
                )
            {
                self.accelerator.wgpu_queue().write_buffer(
                    self.rigid_cell_integrities.wgpu_buffer(),
                    cell.state_slot as u64 * 4,
                    &integrity.to_le_bytes(),
                );
                self.accelerator.wgpu_queue().write_buffer(
                    self.rigid_cell_amounts.wgpu_buffer(),
                    cell.state_slot as u64 * 4,
                    &amount.to_le_bytes(),
                );
                self.accelerator.wgpu_queue().write_buffer(
                    self.rigid_cell_temperatures.wgpu_buffer(),
                    cell.state_slot as u64 * 4,
                    &temperature.to_le_bytes(),
                );
            }
        }
        self.rigid_detachment_snapshot = Some(snapshot.clone());
        Ok(())
    }

    fn rigid_cell_debris_placement(
        &self,
        state: &RigidCellularBodyState,
        cell: &RigidCellularBodyCell,
    ) -> Option<SceneEditCellPlacement> {
        let Some(Material::CellularStatic {
            debris_material: Some(material_identifier),
            debris_yield_rate,
            ..
        }) = self.data.materials().get(cell.material)
        else {
            return None;
        };
        let seed = cell
            .state_slot
            .wrapping_mul(747_796_405)
            .wrapping_add(2_891_336_453);
        if (seed % 10_000) as f32 >= debris_yield_rate * 10_000.0 {
            return None;
        }
        let local = [
            (cell.local[0] as f32 + 0.5) / 8.0,
            (cell.local[1] as f32 + 0.5) / 8.0,
        ];
        let world = [
            state.translation[0] + state.angle.cos() * local[0] - state.angle.sin() * local[1],
            state.translation[1] + state.angle.sin() * local[0] + state.angle.cos() * local[1],
        ];
        Some(SceneEditCellPlacement {
            coordinates: CellCoordinates {
                x: (world[0] * 8.0).floor() as i32,
                y: (world[1] * 8.0).floor() as i32,
            },
            material_identifier: *material_identifier,
            appearance: cell.appearance,
        })
    }

    /// Removes body-local cells and replaces the body with its remaining connected pieces
    #[allow(dead_code)]
    fn remove_rigid_cellular_body_cells(
        &mut self,
        body_index: usize,
        removed: &HashSet<[i32; 2]>,
        cause: RigidCellRemovalCause,
    ) {
        if body_index >= self.rigid_cellular_bodies.len() || removed.is_empty() {
            return;
        }
        let Some(state) = self
            .physics_world
            .rigid_cellular_body_state(&self.rigid_cellular_bodies[body_index])
        else {
            return;
        };
        let body = self.rigid_cellular_bodies.swap_remove(body_index);
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_support.clear();
        self.rigid_cellular_recovery.clear();
        self.rigid_cellular_contact_active.clear();
        self.rigid_granular_contact_active.clear();
        self.physics_world.remove_rigid_cellular_body(&body);
        let mut debris = Vec::new();
        for cell in body
            .cells
            .iter()
            .filter(|cell| removed.contains(&cell.local))
        {
            if cause == RigidCellRemovalCause::Fracture {
                if let Some(placement) = self.rigid_cell_debris_placement(&state, cell) {
                    debris.push(placement);
                }
            }
            self.release_rigid_cell_state(cell.state_slot);
        }
        let remaining = body
            .cells
            .into_iter()
            .filter(|cell| !removed.contains(&cell.local))
            .collect();
        for cells in RigidCellularBody::connected_components(remaining) {
            if cells.is_empty() {
                continue;
            }
            if cells.len() < self.rigid_component_minimum(&cells) {
                for cell in cells {
                    if let Some(placement) = self.rigid_cell_debris_placement(&state, &cell) {
                        debris.push(placement);
                    }
                    self.release_rigid_cell_state(cell.state_slot);
                }
                continue;
            }
            let local_center =
                RigidCellularBody::mass_properties(&cells, self.data.materials()).local_com;
            let child_center = [
                state.translation[0] + state.angle.cos() * local_center.x
                    - state.angle.sin() * local_center.y,
                state.translation[1]
                    + state.angle.sin() * local_center.x
                    + state.angle.cos() * local_center.y,
            ];
            let offset = [
                child_center[0] - state.center_of_mass[0],
                child_center[1] - state.center_of_mass[1],
            ];
            let child_velocity = [
                state.linear_velocity[0] - state.angular_velocity * offset[1],
                state.linear_velocity[1] + state.angular_velocity * offset[0],
            ];
            let (friction, restitution) = self.rigid_cellular_material_response(&cells);
            self.rigid_cellular_bodies
                .push(self.physics_world.insert_rigid_cellular_body(
                    state.translation,
                    state.angle,
                    self.data.materials(),
                    cells,
                    friction,
                    restitution,
                    child_velocity,
                    state.angular_velocity,
                ));
        }
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        if !debris.is_empty() {
            self.pending_runtime_edits.place_cells(debris);
        }
    }

    /// Averages the existing static material response for one concrete body
    fn rigid_cellular_material_response(&self, cells: &[RigidCellularBodyCell]) -> (f32, f32) {
        let (friction, restitution) = cells.iter().fold((0.0, 0.0), |sum, cell| {
            match self.data.materials().get(cell.material) {
                Some(Material::CellularStatic {
                    friction,
                    restitution,
                    ..
                }) => (sum.0 + friction, sum.1 + restitution),
                _ => sum,
            }
        });
        let divisor = cells.len().max(1) as f32;
        (friction / divisor, restitution / divisor)
    }

    /// The strictest material in a mixed component controls its minimum size.
    fn rigid_component_minimum(&self, cells: &[RigidCellularBodyCell]) -> usize {
        cells
            .iter()
            .filter_map(|cell| match self.data.materials().get(cell.material) {
                Some(Material::CellularStatic {
                    minimum_rigid_body_cell_count,
                    ..
                }) => Some(*minimum_rigid_body_cell_count as usize),
                _ => None,
            })
            .max()
            .unwrap_or(1)
    }

    fn release_rigid_cell_state(&mut self, slot: u32) {
        let generation = &mut self.rigid_cell_state_generations[slot as usize];
        *generation = generation.wrapping_add(1);
        let offset = slot as u64 * 4;
        self.accelerator.wgpu_queue().write_buffer(
            self.rigid_cell_integrities.wgpu_buffer(),
            offset,
            &0.0f32.to_le_bytes(),
        );
        self.accelerator.wgpu_queue().write_buffer(
            self.rigid_cell_amounts.wgpu_buffer(),
            offset,
            &0.0f32.to_le_bytes(),
        );
        self.accelerator.wgpu_queue().write_buffer(
            self.rigid_cell_temperatures.wgpu_buffer(),
            offset,
            &0.0f32.to_le_bytes(),
        );
        self.rigid_cell_state_free.push(slot);
    }

    /// Validates and inserts one direct, non-canonical rigid cellular body.
    fn insert_authored_rigid_cellular_body(&mut self, placements: Vec<SceneEditCellPlacement>) {
        let mut unique: BTreeMap<(i32, i32), SceneEditCellPlacement> = BTreeMap::new();
        for placement in placements {
            unique.insert(
                (placement.coordinates.x, placement.coordinates.y),
                placement,
            );
        }
        if unique.is_empty()
            || !unique.values().all(|cell| {
                matches!(
                    self.data.materials().get(cell.material_identifier),
                    Some(Material::CellularStatic { .. })
                )
            })
        {
            return;
        }
        let used: usize = self
            .rigid_cellular_bodies
            .iter()
            .map(|body| body.cells.len())
            .sum();
        if used + unique.len() > self.cellular_physics_body_proxy.rigid_cell_capacity() {
            return;
        }
        let min_x = unique.keys().map(|(x, _)| *x).min().unwrap();
        let min_y = unique.keys().map(|(_, y)| *y).min().unwrap();
        let cells: Vec<_> = unique
            .into_values()
            .map(|cell| RigidCellularBodyCell {
                local: [cell.coordinates.x - min_x, cell.coordinates.y - min_y],
                material: cell.material_identifier,
                appearance: cell.appearance,
                state_slot: u32::MAX,
                state_generation: 0,
            })
            .collect();
        if cells.len() < self.rigid_component_minimum(&cells) {
            let debris = cells
                .into_iter()
                .filter_map(|cell| match self.data.materials().get(cell.material) {
                    Some(Material::CellularStatic {
                        debris_material: Some(material_identifier),
                        debris_yield_rate,
                        ..
                    }) if (((cell.local[0].wrapping_mul(31).wrapping_add(cell.local[1])) as u32
                        % 10_000) as f32)
                        < *debris_yield_rate * 10_000.0 =>
                    {
                        Some(SceneEditCellPlacement {
                            coordinates: CellCoordinates {
                                x: min_x + cell.local[0],
                                y: min_y + cell.local[1],
                            },
                            material_identifier: *material_identifier,
                            appearance: cell.appearance,
                        })
                    }
                    _ => None,
                })
                .collect();
            self.pending_runtime_edits.place_cells(debris);
            return;
        }
        let (friction, restitution) = self.rigid_cellular_material_response(&cells);
        self.insert_rigid_cellular_body(
            [min_x as f32 / 8.0, min_y as f32 / 8.0],
            cells,
            friction,
            restitution,
        );
    }

    /// Centralizes topology invalidation for every rigid-body insertion.
    fn insert_rigid_cellular_body(
        &mut self,
        position: [f32; 2],
        mut cells: Vec<RigidCellularBodyCell>,
        friction: f32,
        restitution: f32,
    ) {
        for cell in &mut cells {
            if cell.state_slot != u32::MAX {
                continue;
            }
            let slot = self
                .rigid_cell_state_free
                .pop()
                .expect("rigid cell capacity checked");
            cell.state_slot = slot;
            cell.state_generation = self.rigid_cell_state_generations[slot as usize];
            let integrity = match self.data.materials().get(cell.material) {
                Some(Material::CellularStatic {
                    default_integrity, ..
                }) => *default_integrity,
                _ => 0.0,
            };
            self.accelerator.wgpu_queue().write_buffer(
                self.rigid_cell_integrities.wgpu_buffer(),
                slot as u64 * 4,
                &integrity.to_le_bytes(),
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.rigid_cell_amounts.wgpu_buffer(),
                slot as u64 * 4,
                &1.0f32.to_le_bytes(),
            );
            let temperature = self.initial_temperature(cell.material);
            self.accelerator.wgpu_queue().write_buffer(
                self.rigid_cell_temperatures.wgpu_buffer(),
                slot as u64 * 4,
                &temperature.to_le_bytes(),
            );
        }
        self.rigid_cellular_bodies
            .push(self.physics_world.insert_rigid_cellular_body(
                position,
                0.0,
                self.data.materials(),
                cells,
                friction,
                restitution,
                [0.0; 2],
                0.0,
            ));
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_support.clear();
        self.rigid_cellular_recovery.clear();
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
    }

    /// Resolves one world cell to a resident physical GPU cell
    fn cell_edit_index(&self, coordinates: CellCoordinates) -> Option<usize> {
        let tile_coordinates: TileCoordinates = coordinates.tile_coordinates();
        let tile: Tile = self.tile_at(tile_coordinates)?;
        let [x, y]: [usize; 2] = coordinates.local_tile_coordinates();
        if !matches!(
            self.chunks.get(&tile_coordinates.chunk_coordinates()),
            Some(ChunkEntry::Active { chunk, .. }) if chunk.get_tile(tile_coordinates).is_ok()
        ) {
            return None;
        }
        Some(tile.0 as usize * 64 + y * 8 + x)
    }

    /// Writes final contiguous cellular edits to the two authoritative GPU buffers
    fn write_cell_edits(
        &self,
        edits: &[(
            usize,
            CellCoordinates,
            MaterialIdentifier,
            CellularAppearance,
            f32,
        )],
    ) {
        let mut start: usize = 0;
        while start < edits.len() {
            let mut end: usize = start + 1;
            while end < edits.len() && edits[end].0 == edits[end - 1].0 + 1 {
                end += 1;
            }
            let mut material_identifiers: Vec<u8> = Vec::with_capacity((end - start) * 4);
            let mut appearances: Vec<u8> = Vec::with_capacity((end - start) * 4);
            let mut integrities: Vec<u8> = Vec::with_capacity((end - start) * 4);
            let mut amounts: Vec<u8> = Vec::with_capacity((end - start) * 4);
            let mut temperatures: Vec<u8> = Vec::with_capacity((end - start) * 4);
            for edit in &edits[start..end] {
                material_identifiers.extend_from_slice(&edit.2.as_u32().to_le_bytes());
                appearances.extend_from_slice(&edit.3.0.to_le_bytes());
                integrities.extend_from_slice(&edit.4.to_bits().to_le_bytes());
                let (amount, temperature): (f32, f32) = if edit.2 == MaterialIdentifier::NULL {
                    (0.0, 0.0)
                } else {
                    (1.0, self.initial_temperature(edit.2))
                };
                amounts.extend_from_slice(&amount.to_bits().to_le_bytes());
                temperatures.extend_from_slice(&temperature.to_bits().to_le_bytes());
            }
            let offset: u64 = edits[start].0 as u64 * 4;
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_material_identifiers.wgpu_buffer(),
                offset,
                &material_identifiers,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_appearances.wgpu_buffer(),
                offset,
                &appearances,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_integrities.wgpu_buffer(),
                offset,
                &integrities,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_amounts.wgpu_buffer(),
                offset,
                &amounts,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_temperatures.wgpu_buffer(),
                offset,
                &temperatures,
            );
            self.cellular_dynamic.clear_cellular_dynamic_kinematics(
                self.accelerator.as_ref(),
                edits[start].0,
                end - start,
            );
            self.cellular_pressure.clear_transient_state(
                self.accelerator.as_ref(),
                edits[start].0,
                end - start,
            );
            start = end;
        }
    }

    fn initial_temperature(&self, material_identifier: MaterialIdentifier) -> f32 {
        self.data
            .materials()
            .thermal_properties(material_identifier)
            .and_then(|properties| properties.default_temperature)
            .unwrap_or(self.ambient_temperature)
    }

    pub(crate) const fn rigid_cell_amounts_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_cell_amounts
    }

    pub(crate) const fn rigid_cell_temperatures_buffer(&self) -> &AcceleratorBuffer {
        &self.rigid_cell_temperatures
    }

    /// Sets the automatic active-area target around a world position
    fn follow_position(&mut self, position: ScenePosition) {
        self.origin_target = TileCoordinates {
            x: position.tile_coordinates.x - i32::from(self.simulation_width) / 2,
            y: position.tile_coordinates.y - i32::from(self.simulation_height) / 2,
        };
    }

    /// Returns the exact tile area currently being simulated
    const fn area_active(&self) -> TileArea {
        TileArea::new(self.origin, self.simulation_width, self.simulation_height)
    }

    /// Returns the moving-fluid area including one camera-streaming batch outside the viewport
    fn area_fluid_active(&self) -> TileArea {
        let padding: u16 = u16::from(self.tile_streaming_batch_size);
        self.area_active()
            .expanded(padding, padding, padding, padding)
    }

    /// Returns the tile area resident on the GPU
    fn area_buffered(&self) -> TileArea {
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let dimensions: u16 = u16::from(self.simulation_buffer_size) * 2;
        TileArea::new(
            TileCoordinates {
                x: self.origin.x - buffer_size,
                y: self.origin.y - buffer_size,
            },
            self.simulation_width + dimensions,
            self.simulation_height + dimensions,
        )
    }

    /// Returns the chunk-aligned area required by the current GPU buffer
    fn area_streaming(&self) -> TileArea {
        self.area_buffered().chunk_area()
    }

    /// Returns the current chunk area plus one chunk in each movement direction
    fn area_prefetching(&self) -> TileArea {
        let velocity: Option<SceneVelocity> = self
            .possessed_actor()
            .and_then(|actor| self.actor_registry.get_velocity(actor))
            .copied();
        let left: bool = self.origin_target.x < self.origin.x
            || velocity.is_some_and(|velocity| velocity.x < 0.0);
        let bottom: bool = self.origin_target.y < self.origin.y
            || velocity.is_some_and(|velocity| velocity.y < 0.0);
        let right: bool = self.origin_target.x > self.origin.x
            || velocity.is_some_and(|velocity| velocity.x > 0.0);
        let top: bool = self.origin_target.y > self.origin.y
            || velocity.is_some_and(|velocity| velocity.y > 0.0);
        self.area_streaming().expanded(
            if left { Chunk::WIDTH } else { 0 },
            if bottom { Chunk::WIDTH } else { 0 },
            if right { Chunk::WIDTH } else { 0 },
            if top { Chunk::WIDTH } else { 0 },
        )
    }

    /// Requests every chunk in a chunk-aligned streaming area
    fn chunks_fetch(&mut self, streaming_area: TileArea) -> Result<(), io::Error> {
        for coordinates in streaming_area.iterate_chunk_coordinates() {
            match self.chunks.get(&coordinates) {
                None | Some(ChunkEntry::Error(_)) => self.chunk_load(coordinates)?,
                Some(ChunkEntry::Active { .. })
                | Some(ChunkEntry::Loading { .. })
                | Some(ChunkEntry::Generating { .. })
                | Some(ChunkEntry::Saving { .. }) => {}
            };
        }
        Ok(())
    }

    /// Reads an unloaded chunk into this `Scene`
    fn chunk_load(&mut self, coordinates: TileCoordinates) -> Result<(), io::Error> {
        // avoid duplicate work
        if let Some(entry) = self.chunks.get(&coordinates) {
            match entry {
                ChunkEntry::Active { .. }
                | ChunkEntry::Loading { .. }
                | ChunkEntry::Generating { .. }
                | ChunkEntry::Saving { .. } => return Ok(()),
                _ => (),
            }
        }
        let streaming_identifier: u64 = self.chunks_streaming_identifier_next;
        self.chunks_streaming_identifier_next =
            self.chunks_streaming_identifier_next.wrapping_add(1);
        self.chunks.insert(
            coordinates,
            ChunkEntry::Loading {
                streaming_identifier,
            },
        );
        let data: SceneData = self.data.clone();
        let sender: SyncSender<ChunkStreamingResponse> =
            self.chunk_streaming_response_sender.clone();
        // spawn new thread to attempt read
        std::thread::spawn(move || {
            let result: Result<Option<Box<Chunk>>, Box<dyn Error + Send + Sync>> = data
                .read_chunk(coordinates)
                .map(|chunk| chunk.map(Box::new))
                .map_err(|error| Box::new(error).into());
            sender
                .send(ChunkStreamingResponse::Loaded {
                    streaming_identifier,
                    coordinates,
                    result,
                })
                .unwrap();
        });
        Ok(())
    }

    /// Generates a new chunk for this `Scene`
    fn chunk_generate(&mut self, coordinates: TileCoordinates) -> Result<(), io::Error> {
        // avoid duplicate work
        if let Some(entry) = self.chunks.get(&coordinates) {
            match entry {
                ChunkEntry::Active { .. }
                | ChunkEntry::Generating { .. }
                | ChunkEntry::Saving { .. } => return Ok(()),
                _ => (),
            }
        }
        let streaming_identifier: u64 = self.chunks_streaming_identifier_next;
        self.chunks_streaming_identifier_next =
            self.chunks_streaming_identifier_next.wrapping_add(1);
        self.chunks.insert(
            coordinates,
            ChunkEntry::Generating {
                streaming_identifier,
            },
        );
        let generator: Arc<dyn SceneGenerator> = self.generator.clone();
        let sender: SyncSender<ChunkStreamingResponse> =
            self.chunk_streaming_response_sender.clone();
        // spawn new thread for generation
        std::thread::spawn(move || {
            let result: Result<Box<Chunk>, Box<dyn Error + Send + Sync>> =
                Ok(Box::new(generator.generate_chunk(coordinates)));
            sender
                .send(ChunkStreamingResponse::Generated {
                    streaming_identifier,
                    coordinates,
                    result,
                })
                .unwrap();
        });
        Ok(())
    }

    /// Saves dirty chunks and removes entries outside the one-chunk retention region
    fn chunks_save(&mut self) -> Result<(), io::Error> {
        let retention_area: TileArea =
            self.area_streaming()
                .expanded(Chunk::WIDTH, Chunk::WIDTH, Chunk::WIDTH, Chunk::WIDTH);
        let coordinates: Vec<TileCoordinates> = self
            .chunks
            .iter()
            .filter_map(|(coordinates, entry)| {
                if retention_area.contains(*coordinates)
                    || matches!(
                        entry,
                        ChunkEntry::Loading { .. }
                            | ChunkEntry::Generating { .. }
                            | ChunkEntry::Saving { .. },
                    )
                {
                    None
                } else {
                    Some(*coordinates)
                }
            })
            .collect();
        for coordinates in coordinates {
            // keep stale CPU chunks unavailable to save or removal until downloads are applied
            if self.tile_download_pending_for_chunk(coordinates)?
                || self.fluid_transfer_pending_for_chunk(coordinates)?
                || self.gas_transfer_pending_for_chunk(coordinates)?
            {
                continue;
            }
            let Some(entry) = self.chunks.remove(&coordinates) else {
                continue;
            };
            let ChunkEntry::Active {
                chunk,
                is_dirty: true,
            } = entry
            else {
                continue;
            };
            let streaming_identifier: u64 = self.chunks_streaming_identifier_next;
            self.chunks_streaming_identifier_next =
                self.chunks_streaming_identifier_next.wrapping_add(1);
            self.chunks.insert(
                coordinates,
                ChunkEntry::Saving {
                    streaming_identifier,
                },
            );
            let data: SceneData = self.data.clone();
            let sender: SyncSender<ChunkStreamingResponse> =
                self.chunk_streaming_response_sender.clone();
            std::thread::spawn(move || {
                let result: Result<Box<Chunk>, (Box<Chunk>, io::Error)> =
                    match data.write_chunk(&chunk) {
                        Ok(()) => Ok(Box::new(chunk)),
                        Err(error) => Err((Box::new(chunk), error)),
                    };
                sender
                    .send(ChunkStreamingResponse::Saved {
                        streaming_identifier,
                        coordinates,
                        result,
                    })
                    .unwrap();
            });
        }
        Ok(())
    }

    /// Refreshes streamed chunks and queues newly available resident tiles for upload
    fn chunks_refresh(&mut self) -> Result<(), io::Error> {
        // apply completed background loads and generations before planning movement
        let mut chunks_available: Vec<TileCoordinates> = Vec::new();
        while let Ok(response) = self.chunk_streaming_responses.try_recv() {
            match response {
                ChunkStreamingResponse::Loaded {
                    streaming_identifier,
                    coordinates,
                    result,
                } => {
                    let matches_request = matches!( // ignore mutated entries
                        self.chunks.get(&coordinates),
                        Some(ChunkEntry::Loading { streaming_identifier: current })
                            if *current == streaming_identifier
                    );
                    if !matches_request {
                        continue;
                    }
                    match result {
                        // missing chunk begins a separate generation operation
                        Ok(Some(chunk)) => {
                            self.chunks.insert(
                                coordinates,
                                ChunkEntry::Active {
                                    chunk: *chunk,
                                    is_dirty: false,
                                },
                            );
                            chunks_available.push(coordinates);
                        }
                        Ok(None) => self.chunk_generate(coordinates)?,
                        Err(error) => {
                            let error: Box<dyn Error> = error;
                            self.chunks.insert(coordinates, ChunkEntry::Error(error));
                        }
                    }
                }
                ChunkStreamingResponse::Generated {
                    streaming_identifier,
                    coordinates,
                    result,
                } => {
                    if !matches!( // ignore mutated entries
                        self.chunks.get(&coordinates),
                        Some(ChunkEntry::Generating { streaming_identifier: current })
                            if *current == streaming_identifier
                    ) {
                        continue;
                    }
                    match result {
                        // retain the successfully generated chunk
                        Ok(chunk) => {
                            self.chunks.insert(
                                coordinates,
                                ChunkEntry::Active {
                                    chunk: *chunk,
                                    is_dirty: false,
                                },
                            );
                            chunks_available.push(coordinates);
                        }
                        Err(error) => {
                            let error: Box<dyn Error> = error;
                            self.chunks.insert(coordinates, ChunkEntry::Error(error));
                        }
                    }
                }
                ChunkStreamingResponse::Saved {
                    streaming_identifier,
                    coordinates,
                    result,
                } => {
                    if !matches!(
                        self.chunks.get(&coordinates),
                        Some(ChunkEntry::Saving { streaming_identifier: current })
                            if *current == streaming_identifier
                    ) {
                        continue;
                    }
                    match result {
                        Ok(chunk) => {
                            if self.area_prefetching().contains(coordinates) {
                                self.chunks.insert(
                                    coordinates,
                                    ChunkEntry::Active {
                                        chunk: *chunk,
                                        is_dirty: false,
                                    },
                                );
                                chunks_available.push(coordinates);
                            } else {
                                self.chunks.remove(&coordinates);
                            }
                        }
                        Err((chunk, error)) => {
                            self.chunks.insert(
                                coordinates,
                                ChunkEntry::Active {
                                    chunk: *chunk,
                                    is_dirty: true,
                                },
                            );
                            return Err(error);
                        }
                    }
                }
            }
        }
        self.chunks_fetch(self.area_prefetching())?;
        // move by at most one batch
        let batch_size: i64 = self.tile_streaming_batch_size as i64;
        let x_difference: i64 = self.origin_target.x as i64 - self.origin.x as i64;
        let y_difference: i64 = self.origin_target.y as i64 - self.origin.y as i64;
        if x_difference >= batch_size {
            self.shift_right()?;
        } else if x_difference <= -batch_size {
            self.shift_left()?;
        } else if y_difference >= batch_size {
            self.shift_up()?;
        } else if y_difference <= -batch_size {
            self.shift_down()?;
        }
        self.chunks_save()?;
        // queue upload only newly available chunks
        for coordinates in chunks_available {
            let _ = self.tiles_upload(TileArea::new(coordinates, Chunk::WIDTH, Chunk::WIDTH));
        }
        Ok(())
    }

    /// Shifts the active tile area up by the configured streaming batch size
    fn shift_up(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        let mut origin: TileCoordinates = self.origin;
        origin.y += batch_size;
        self.shift_to(origin)
    }

    /// Shifts the active tile area down by the configured streaming batch size
    fn shift_down(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        let mut origin: TileCoordinates = self.origin;
        origin.y -= batch_size;
        self.shift_to(origin)
    }

    /// Shifts the active tile area right by the configured streaming batch size
    fn shift_right(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        let mut origin: TileCoordinates = self.origin;
        origin.x += batch_size;
        self.shift_to(origin)
    }

    /// Shifts the active tile area left by the configured streaming batch size
    fn shift_left(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        let mut origin: TileCoordinates = self.origin;
        origin.x -= batch_size;
        self.shift_to(origin)
    }

    /// Moves the origin, remaps ring slots, and streams tiles
    fn shift_to(&mut self, new_origin: TileCoordinates) -> Result<(), io::Error> {
        // verify the incoming CPU state before reserving outgoing GPU state
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let dimensions: u16 = u16::from(self.simulation_buffer_size) * 2;
        let width: u16 = self.simulation_width + dimensions;
        let height: u16 = self.simulation_height + dimensions;
        let buffered_area: TileArea = TileArea::new(
            TileCoordinates {
                x: new_origin.x - buffer_size,
                y: new_origin.y - buffer_size,
            },
            width,
            height,
        );
        let streaming_area: TileArea = buffered_area.chunk_area();
        self.chunks_fetch(streaming_area)?;
        if !streaming_area
            .iterate_chunk_coordinates()
            .all(|coordinates| {
                matches!(
                    self.chunks.get(&coordinates),
                    Some(ChunkEntry::Active { .. })
                )
            })
        {
            return Ok(());
        }
        let batch_size: u16 = u16::from(self.tile_streaming_batch_size);
        let old_buffered_origin: TileCoordinates = TileCoordinates {
            x: self.origin.x - buffer_size,
            y: self.origin.y - buffer_size,
        };
        let tiles_download_area: TileArea;
        let tiles_upload_area: TileArea;
        if new_origin.x > self.origin.x {
            tiles_download_area = TileArea::new(old_buffered_origin, batch_size, height);
            tiles_upload_area = TileArea::new(
                TileCoordinates {
                    x: new_origin.x - buffer_size + width as i32 - batch_size as i32,
                    y: new_origin.y - buffer_size,
                },
                batch_size,
                height,
            );
        } else if new_origin.x < self.origin.x {
            tiles_download_area = TileArea::new(
                TileCoordinates {
                    x: old_buffered_origin.x + width as i32 - batch_size as i32,
                    y: old_buffered_origin.y,
                },
                batch_size,
                height,
            );
            tiles_upload_area = TileArea::new(
                TileCoordinates {
                    x: new_origin.x - buffer_size,
                    y: new_origin.y - buffer_size,
                },
                batch_size,
                height,
            );
        } else if new_origin.y > self.origin.y {
            tiles_download_area = TileArea::new(old_buffered_origin, width, batch_size);
            tiles_upload_area = TileArea::new(
                TileCoordinates {
                    x: new_origin.x - buffer_size,
                    y: new_origin.y - buffer_size + height as i32 - batch_size as i32,
                },
                width,
                batch_size,
            );
        } else if new_origin.y < self.origin.y {
            tiles_download_area = TileArea::new(
                TileCoordinates {
                    x: old_buffered_origin.x,
                    y: old_buffered_origin.y + height as i32 - batch_size as i32,
                },
                width,
                batch_size,
            );
            tiles_upload_area = TileArea::new(
                TileCoordinates {
                    x: new_origin.x - buffer_size,
                    y: new_origin.y - buffer_size,
                },
                width,
                batch_size,
            );
        } else {
            return Ok(());
        }

        // defer rapid re-entry until the prior download has reached its CPU chunk
        if self.tile_download_pending_in(tiles_upload_area)?
            || self.fluid_download_pending_in(tiles_upload_area)?
            || self.fluid_upload_pending_in(tiles_download_area)?
            || self.gas_download_pending_in(tiles_upload_area)?
        {
            return Ok(());
        }

        // materialize queued CPU state before capturing the old physical slots
        self.tile_uploads_submit()?;
        self.fluid_uploads_submit()?;

        // capture and submit old physical slots before changing their world interpretation
        self.tile_downloads_queue(tiles_download_area)?;
        self.tile_downloads_submit()?;
        self.fluid_downloads_queue(tiles_download_area);
        self.fluid_downloads_submit()?;
        self.gas_downloads_queue(tiles_download_area);
        self.gas_downloads_submit()?;

        // remap only the reused edge; retained tiles keep their physical kinematic slots
        if new_origin.x > self.origin.x {
            self.tiles_ring_offset_x = (self.tiles_ring_offset_x + batch_size) % width;
        } else if new_origin.x < self.origin.x {
            self.tiles_ring_offset_x = (self.tiles_ring_offset_x + width - batch_size) % width;
        } else if new_origin.y > self.origin.y {
            self.tiles_ring_offset_y = (self.tiles_ring_offset_y + batch_size) % height;
        } else {
            self.tiles_ring_offset_y = (self.tiles_ring_offset_y + height - batch_size) % height;
        }
        self.origin = new_origin;
        self.cellular_collision_dirty = true;
        self.fluids.refresh(
            self.accelerator.as_ref(),
            self.area_fluid_active().origin(),
            self.area_fluid_active().dimensions()[0],
            self.area_fluid_active().dimensions()[1],
            TileCoordinates {
                x: new_origin.x - buffer_size,
                y: new_origin.y - buffer_size,
            },
            width,
            height,
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
        );
        self.gas_clear_area(tiles_upload_area);
        let _ = self.tiles_upload(tiles_upload_area);
        self.fluid_uploads_queue(tiles_upload_area)?;
        self.gas_upload_area(tiles_upload_area)?;
        Ok(())
    }

    /// Returns the active tile at a provided `TileCoordinates` if one exists
    pub fn tile_at(&self, coordinates: TileCoordinates) -> Option<Tile> {
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let width: usize = self.simulation_width as usize + buffer_size as usize * 2;
        let x: usize = usize::try_from(coordinates.x - (self.origin.x - buffer_size)).ok()?;
        let y: usize = usize::try_from(coordinates.y - (self.origin.y - buffer_size)).ok()?;
        let height: usize = self.simulation_height as usize + buffer_size as usize * 2;
        if x >= width || y >= height {
            return None;
        }
        let x: usize = (x + self.tiles_ring_offset_x as usize) % width;
        let y: usize = (y + self.tiles_ring_offset_y as usize) % height;
        self.tiles.get(y * width + x).copied()
    }

    /// Queues one dense gas strip export under the current ring interpretation
    fn gas_downloads_queue(&mut self, area: TileArea) {
        if self.gases.gas_count() == 0 {
            return;
        }
        let dimensions: u16 = (self.simulation_width + u16::from(self.simulation_buffer_size) * 2)
            .max(self.simulation_height + u16::from(self.simulation_buffer_size) * 2);
        let maximum_cell_count: u32 =
            u32::from(self.tile_streaming_batch_size) * u32::from(dimensions) * 64;
        let download: Arc<Mutex<GasDownload>> = self.gas_download_pool.pop().unwrap_or_else(|| {
            Arc::new(Mutex::new(GasDownload::new(
                self.accelerator.as_ref(),
                area,
                maximum_cell_count,
                self.gases.gas_count(),
            )))
        });
        download.lock().unwrap().reset(area);
        self.gas_downloads.push(download);
    }

    /// Returns whether an incoming area overlaps unresolved exported gas
    fn gas_download_pending_in(&self, area: TileArea) -> Result<bool, io::Error> {
        for download in &self.gas_downloads {
            if download
                .lock()
                .map_err(|_| io::Error::other("Gas download is unavailable"))?
                .area
                .intersects(area)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns whether a chunk is pinned by an unresolved gas export
    fn gas_transfer_pending_for_chunk(
        &self,
        coordinates: TileCoordinates,
    ) -> Result<bool, io::Error> {
        for download in &self.gas_downloads {
            if download
                .lock()
                .map_err(|_| io::Error::other("Gas download is unavailable"))?
                .area
                .chunk_area()
                .contains(coordinates)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Clears a world area through its current physical ring mapping
    fn gas_clear_area(&self, area: TileArea) {
        let buffered_area: TileArea = self.area_buffered();
        let dimensions: [u16; 2] = buffered_area.dimensions();
        self.gases.clear_area(
            self.accelerator.as_ref(),
            area,
            buffered_area.origin(),
            dimensions[0],
            dimensions[1],
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
        );
    }

    /// Moves dormant sparse gas from CPU chunks into dense resident GPU fields
    fn gas_upload_area(&mut self, area: TileArea) -> Result<(), io::Error> {
        if self.gases.gas_count() == 0 {
            return Ok(());
        }
        let chunk_coordinates: Vec<TileCoordinates> =
            area.chunk_area().iterate_chunk_coordinates().collect();
        for coordinates in &chunk_coordinates {
            if !matches!(
                self.chunks.get(coordinates),
                Some(ChunkEntry::Active { .. })
            ) {
                return Err(io::Error::other("Incoming gas chunk is not active"));
            }
        }
        let mut cells: Vec<ChunkGasCell> = Vec::new();
        for coordinates in chunk_coordinates {
            let Some(ChunkEntry::Active { chunk, is_dirty }) = self.chunks.get_mut(&coordinates)
            else {
                unreachable!();
            };
            let mut chunk_cells: Vec<ChunkGasCell> = chunk.take_dormant_gas_cells(area);
            if !chunk_cells.is_empty() {
                *is_dirty = true;
            }
            cells.append(&mut chunk_cells);
        }
        if cells.is_empty() {
            return Ok(());
        }
        let mut cells = cells;
        for cell in &mut cells {
            if !cell.temperature.is_finite() {
                cell.temperature = self.ambient_temperature;
            }
        }
        let upload: GasUpload = GasUpload::new(area, cells);
        if let Err(error) = upload.validate(self.data.materials()) {
            self.gas_cells_restore(upload.cells)?;
            return Err(error);
        }
        let physical_indices: Option<Vec<usize>> = upload
            .cells
            .iter()
            .map(|cell| self.cell_edit_index(cell.coordinates))
            .collect();
        let Some(physical_indices) = physical_indices else {
            self.gas_cells_restore(upload.cells)?;
            return Err(io::Error::other(
                "Incoming dormant gas cell is outside GPU residency",
            ));
        };
        self.gases
            .import(self.accelerator.as_ref(), &upload, &physical_indices);
        Ok(())
    }

    /// Returns sparse gas cells to their owning CPU chunks after a failed import validation
    fn gas_cells_restore(&mut self, cells: Vec<ChunkGasCell>) -> Result<(), io::Error> {
        for cell in cells {
            let coordinates: TileCoordinates = cell.tile_coordinates().chunk_coordinates();
            let Some(ChunkEntry::Active { chunk, is_dirty }) = self.chunks.get_mut(&coordinates)
            else {
                return Err(io::Error::other("Dormant gas source chunk is not active"));
            };
            chunk
                .insert_dormant_gas_cell(cell)
                .map_err(|_| io::Error::other("Dormant gas cell is outside its source chunk"))?;
            *is_dirty = true;
        }
        Ok(())
    }

    /// Applies completed gas exports to their world-position CPU chunks
    fn gas_downloads_apply_completed(&mut self) -> Result<(), io::Error> {
        let mut index: usize = 0;
        while index < self.gas_downloads.len() {
            let mut download = self.gas_downloads[index]
                .lock()
                .map_err(|_| io::Error::other("Gas download is unavailable"))?;
            let Some(result) = download.result.as_ref() else {
                index += 1;
                continue;
            };
            if let Err(error) = result {
                return Err(io::Error::other(format!("Gas download failed: {error}")));
            }
            let area: TileArea = download.area;
            for cell in result.as_ref().unwrap() {
                let coordinates: TileCoordinates = cell.tile_coordinates();
                if !area.contains(coordinates)
                    || !matches!(
                        self.chunks.get(&coordinates.chunk_coordinates()),
                        Some(ChunkEntry::Active { .. }),
                    )
                {
                    return Err(io::Error::other(
                        "Exported gas cell has no active destination chunk",
                    ));
                }
            }
            let cells: Vec<ChunkGasCell> = download.result.take().unwrap().unwrap();
            drop(download);
            self.gas_cells_restore(cells)?;
            let download: Arc<Mutex<GasDownload>> = self.gas_downloads.swap_remove(index);
            self.gas_download_pool.push(download);
        }
        Ok(())
    }

    /// Submits queued gas exports and begins their asynchronous readbacks
    fn gas_downloads_submit(&self) -> Result<(), io::Error> {
        for download in &self.gas_downloads {
            let mut state = download
                .lock()
                .map_err(|_| io::Error::other("Gas download is unavailable"))?;
            if state.is_started {
                continue;
            }
            let buffered_area: TileArea = self.area_buffered();
            let buffered_dimensions: [u16; 2] = buffered_area.dimensions();
            self.gases.export(
                self.accelerator.as_ref(),
                &state,
                buffered_area.origin(),
                buffered_dimensions[0],
                buffered_dimensions[1],
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            );
            state.is_started = true;
            let area: TileArea = state.area;
            let dimensions: [u16; 2] = area.dimensions();
            let byte_count: usize = usize::from(dimensions[0])
                * usize::from(dimensions[1])
                * 64
                * (5 + self.gases.gas_count() as usize)
                * 4;
            let gas_identifiers: Vec<MaterialIdentifier> = self
                .data
                .materials()
                .iter()
                .filter_map(|(identifier, material)| {
                    matches!(material, Material::Gas { .. }).then_some(identifier)
                })
                .collect();
            let buffer: wgpu::Buffer = state.buffer.clone();
            let mapped_buffer: wgpu::Buffer = buffer.clone();
            let download: Arc<Mutex<GasDownload>> = download.clone();
            drop(state);
            buffer
                .slice(0..byte_count as u64)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let bytes: Result<Vec<u8>, io::Error> = match result {
                        Ok(()) => {
                            match mapped_buffer.slice(0..byte_count as u64).get_mapped_range() {
                                Ok(mapped_data) => {
                                    let bytes: Vec<u8> = mapped_data.to_vec();
                                    drop(mapped_data);
                                    mapped_buffer.unmap();
                                    Ok(bytes)
                                }
                                Err(error) => {
                                    mapped_buffer.unmap();
                                    Err(io::Error::other(error.to_string()))
                                }
                            }
                        }
                        Err(_) => Err(io::Error::other("Gas download failed")),
                    };
                    std::thread::spawn(move || {
                        let result: Result<Vec<ChunkGasCell>, io::Error> =
                            bytes.and_then(|bytes| {
                                GasDownload::deserialize(&bytes, area, &gas_identifiers)
                            });
                        download.lock().unwrap().result = Some(result);
                    });
                });
        }
        Ok(())
    }

    /// Queues one authoritative fluid export under the current ring interpretation
    fn fluid_downloads_queue(&mut self, area: TileArea) {
        let download: Arc<Mutex<FluidDownload>> =
            self.fluid_download_pool.pop().unwrap_or_else(|| {
                Arc::new(Mutex::new(FluidDownload::new(
                    self.accelerator.as_ref(),
                    area,
                    self.fluids.particle_capacity(),
                )))
            });
        download.lock().unwrap().reset(area);
        self.fluid_downloads.push(download);
    }

    /// Returns whether an incoming area overlaps unresolved exported fluid
    fn fluid_download_pending_in(&self, area: TileArea) -> Result<bool, io::Error> {
        for download in &self.fluid_downloads {
            if download
                .lock()
                .map_err(|_| io::Error::other("Fluid download is unavailable"))?
                .area
                .intersects(area)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns whether an outgoing area overlaps unresolved imported fluid
    fn fluid_upload_pending_in(&self, area: TileArea) -> Result<bool, io::Error> {
        for upload in &self.fluid_uploads {
            if upload
                .lock()
                .map_err(|_| io::Error::other("Fluid upload is unavailable"))?
                .area
                .intersects(area)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns whether a chunk is pinned by an unresolved fluid ownership transfer
    fn fluid_transfer_pending_for_chunk(
        &self,
        coordinates: TileCoordinates,
    ) -> Result<bool, io::Error> {
        for download in &self.fluid_downloads {
            if download
                .lock()
                .map_err(|_| io::Error::other("Fluid download is unavailable"))?
                .area
                .chunk_area()
                .contains(coordinates)
            {
                return Ok(true);
            }
        }
        for upload in &self.fluid_uploads {
            if upload
                .lock()
                .map_err(|_| io::Error::other("Fluid upload is unavailable"))?
                .area
                .chunk_area()
                .contains(coordinates)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Removes incoming dormant records from chunks into a pending GPU transfer
    fn fluid_uploads_queue(&mut self, area: TileArea) -> Result<(), io::Error> {
        let chunk_coordinates: Vec<TileCoordinates> =
            area.chunk_area().iterate_chunk_coordinates().collect();
        for coordinates in &chunk_coordinates {
            let Some(ChunkEntry::Active { .. }) = self.chunks.get(coordinates) else {
                return Err(io::Error::other("Incoming fluid chunk is not active"));
            };
        }
        let mut particles: Vec<ChunkFluidParticle> = Vec::new();
        for coordinates in chunk_coordinates {
            let Some(ChunkEntry::Active { chunk, is_dirty }) = self.chunks.get_mut(&coordinates)
            else {
                unreachable!();
            };
            let mut chunk_particles: Vec<ChunkFluidParticle> =
                chunk.take_dormant_fluid_particles(area);
            if !chunk_particles.is_empty() {
                *is_dirty = true;
            }
            particles.append(&mut chunk_particles);
        }
        if particles.is_empty() {
            return Ok(());
        }
        if particles.len() > self.fluids.particle_capacity() as usize {
            for particle in particles {
                let Some(ChunkEntry::Active { chunk, .. }) = self
                    .chunks
                    .get_mut(&particle.tile_coordinates().chunk_coordinates())
                else {
                    unreachable!();
                };
                chunk.insert_dormant_fluid_particle(particle).unwrap();
            }
            return Err(io::Error::other(
                "Incoming dormant fluid exceeds the GPU particle pool capacity",
            ));
        }
        for particle in &mut particles {
            if !particle.temperature.is_finite() {
                particle.temperature = self.initial_temperature(particle.material_identifier);
            }
        }
        if !particles.iter().all(|particle| {
            matches!(
                self.data.materials().get(particle.material_identifier),
                Some(Material::Fluid { .. }),
            )
        }) {
            for particle in particles {
                let Some(ChunkEntry::Active { chunk, .. }) = self
                    .chunks
                    .get_mut(&particle.tile_coordinates().chunk_coordinates())
                else {
                    unreachable!();
                };
                chunk.insert_dormant_fluid_particle(particle).unwrap();
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Dormant particle references an unregistered fluid material",
            ));
        }
        self.fluid_uploads
            .push(Arc::new(Mutex::new(FluidUpload::new(
                self.accelerator.as_ref(),
                area,
                particles,
            ))));
        Ok(())
    }

    /// Applies completed fluid exports to their current-position CPU chunks
    fn fluid_downloads_apply_completed(&mut self) -> Result<(), io::Error> {
        let mut index: usize = 0;
        while index < self.fluid_downloads.len() {
            let mut download = self.fluid_downloads[index]
                .lock()
                .map_err(|_| io::Error::other("Fluid download is unavailable"))?;
            let Some(result) = download.result.as_ref() else {
                index += 1;
                continue;
            };
            if let Err(error) = result {
                return Err(io::Error::other(format!("Fluid download failed: {error}")));
            }
            let area: TileArea = download.area;
            for particle in result.as_ref().unwrap() {
                let coordinates: TileCoordinates = particle.tile_coordinates();
                if !area.contains(coordinates)
                    || !matches!(
                        self.chunks.get(&coordinates.chunk_coordinates()),
                        Some(ChunkEntry::Active { .. }),
                    )
                {
                    return Err(io::Error::other(
                        "Exported fluid particle has no active destination chunk",
                    ));
                }
            }
            let particles: Vec<ChunkFluidParticle> = download.result.take().unwrap().unwrap();
            drop(download);
            for particle in particles {
                let Some(ChunkEntry::Active { chunk, is_dirty }) = self
                    .chunks
                    .get_mut(&particle.tile_coordinates().chunk_coordinates())
                else {
                    unreachable!();
                };
                chunk.insert_dormant_fluid_particle(particle).map_err(|_| {
                    io::Error::other("Exported fluid particle is outside its destination chunk")
                })?;
                *is_dirty = true;
            }
            let download: Arc<Mutex<FluidDownload>> = self.fluid_downloads.swap_remove(index);
            self.fluid_download_pool.push(download);
        }
        Ok(())
    }

    /// Restores failed imports to CPU ownership and completes successful transfers
    fn fluid_uploads_apply_completed(&mut self) -> Result<(), io::Error> {
        let mut index: usize = 0;
        while index < self.fluid_uploads.len() {
            let mut upload = self.fluid_uploads[index]
                .lock()
                .map_err(|_| io::Error::other("Fluid upload is unavailable"))?;
            let Some(result) = upload.result.as_ref() else {
                index += 1;
                continue;
            };
            if let Err(error) = result {
                return Err(io::Error::other(format!("Fluid upload failed: {error}")));
            }
            for particle in result.as_ref().unwrap() {
                if !matches!(
                    self.chunks
                        .get(&particle.tile_coordinates().chunk_coordinates()),
                    Some(ChunkEntry::Active { .. }),
                ) {
                    return Err(io::Error::other(
                        "Rejected fluid particle has no active source chunk",
                    ));
                }
            }
            let failed: Vec<ChunkFluidParticle> = upload.result.take().unwrap().unwrap();
            drop(upload);
            for particle in &failed {
                let Some(ChunkEntry::Active { chunk, is_dirty }) = self
                    .chunks
                    .get_mut(&particle.tile_coordinates().chunk_coordinates())
                else {
                    unreachable!();
                };
                chunk
                    .insert_dormant_fluid_particle(*particle)
                    .map_err(|_| {
                        io::Error::other("Rejected fluid particle is outside its source chunk")
                    })?;
                *is_dirty = true;
            }
            self.fluid_uploads.swap_remove(index);
            if !failed.is_empty() {
                return Err(io::Error::other(format!(
                    "GPU fluid pool rejected {} dormant particles",
                    failed.len(),
                )));
            }
        }
        Ok(())
    }

    /// Applies a completed sample only to the actor for which it was dispatched
    fn fluid_sample_apply_completed(&mut self) -> Result<(), io::Error> {
        let Some(result) = self
            .fluid_sample_result
            .lock()
            .map_err(|_| io::Error::other("Pawn fluid sample result is unavailable"))?
            .take()
        else {
            return Ok(());
        };
        let actor: Actor = self
            .fluid_sample_actor
            .take()
            .ok_or_else(|| io::Error::other("Completed pawn fluid sample has no actor"))?;
        let sample: [f32; 5] = result.map_err(io::Error::other)?;
        if self.possessed_actor() == Some(actor) {
            self.actor_registry.apply_swimming_sample(actor, sample);
        }
        Ok(())
    }

    /// Dispatches one tiny derived-cell sample without waiting for its readback
    fn fluid_sample_submit(&mut self) -> Result<(), io::Error> {
        if self.fluid_sample_actor.is_some() {
            return Ok(());
        }
        let Some(actor) = self.possessed_actor() else {
            return Ok(());
        };
        let Some((center, shape)) = self.actor_registry.swimming_pawn_sample(actor) else {
            return Ok(());
        };
        let active_area: TileArea = self.area_fluid_active();
        let active_dimensions: [u16; 2] = active_area.dimensions();
        let buffered_area: TileArea = self.area_buffered();
        let buffered_dimensions: [u16; 2] = buffered_area.dimensions();
        self.fluids.sample_pawn(
            self.accelerator.as_ref(),
            &self.fluid_sample_buffer,
            center,
            shape,
            active_area.origin(),
            active_dimensions[0],
            active_dimensions[1],
            buffered_area.origin(),
            buffered_dimensions[0],
            buffered_dimensions[1],
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
            self.gravity,
        );
        self.fluid_sample_actor = Some(actor);
        let mapped_buffer: wgpu::Buffer = self.fluid_sample_buffer.clone();
        let result: Arc<Mutex<Option<Result<[f32; 5], String>>>> = self.fluid_sample_result.clone();
        self.fluid_sample_buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |mapping| {
                let sample: Result<[f32; 5], String> = mapping
                    .map_err(|_| "Pawn fluid sample readback failed".to_owned())
                    .and_then(|()| {
                        mapped_buffer
                            .slice(..)
                            .get_mapped_range()
                            .map_err(|error| error.to_string())
                            .and_then(|mapped| {
                                let sample: [f32; 5] = std::array::from_fn(|index| {
                                    f32::from_bits(u32::from_le_bytes(
                                        mapped[index * 4..index * 4 + 4].try_into().unwrap(),
                                    ))
                                });
                                drop(mapped);
                                if sample.into_iter().all(f32::is_finite) {
                                    Ok(sample)
                                } else {
                                    Err("Pawn fluid sample contains a non-finite value".to_owned())
                                }
                            })
                    });
                mapped_buffer.unmap();
                if let Ok(mut result) = result.lock() {
                    *result = Some(sample);
                }
            });
        Ok(())
    }

    /// Submits queued fluid exports and begins their asynchronous readbacks
    fn fluid_downloads_submit(&self) -> Result<(), io::Error> {
        for download in &self.fluid_downloads {
            let mut state = download
                .lock()
                .map_err(|_| io::Error::other("Fluid download is unavailable"))?;
            if state.is_started {
                continue;
            }
            self.fluids.export(
                self.accelerator.as_ref(),
                &state,
                self.area_fluid_active().origin(),
                self.area_fluid_active().dimensions()[0],
                self.area_fluid_active().dimensions()[1],
                self.area_buffered().origin(),
                self.area_buffered().dimensions()[0],
                self.area_buffered().dimensions()[1],
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            );
            state.is_started = true;
            let buffer: wgpu::Buffer = state.buffer.clone();
            let mapped_buffer: wgpu::Buffer = buffer.clone();
            let download: Arc<Mutex<FluidDownload>> = download.clone();
            let particle_capacity: u32 = self.fluids.particle_capacity();
            drop(state);
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let bytes: Result<Vec<u8>, io::Error> = match result {
                        Ok(()) => match mapped_buffer.slice(..).get_mapped_range() {
                            Ok(mapped_data) => {
                                let count: usize =
                                    u32::from_le_bytes(mapped_data[0..4].try_into().unwrap())
                                        as usize;
                                let byte_count: usize = if count <= particle_capacity as usize {
                                    16 + count * ChunkFluidParticle::GPU_SIZE
                                } else {
                                    16
                                };
                                let bytes: Vec<u8> = mapped_data[..byte_count].to_vec();
                                drop(mapped_data);
                                mapped_buffer.unmap();
                                Ok(bytes)
                            }
                            Err(error) => {
                                mapped_buffer.unmap();
                                Err(io::Error::other(error.to_string()))
                            }
                        },
                        Err(_) => Err(io::Error::other("Fluid download failed")),
                    };
                    std::thread::spawn(move || {
                        let result: Result<Vec<ChunkFluidParticle>, io::Error> =
                            bytes.and_then(|bytes| {
                                FluidDownload::deserialize(&bytes, particle_capacity)
                            });
                        download.lock().unwrap().result = Some(result);
                    });
                });
        }
        Ok(())
    }

    /// Submits queued dormant-fluid reconstruction and begins result readback
    fn fluid_uploads_submit(&self) -> Result<(), io::Error> {
        for upload in &self.fluid_uploads {
            let mut state = upload
                .lock()
                .map_err(|_| io::Error::other("Fluid upload is unavailable"))?;
            if state.is_started {
                continue;
            }
            self.fluids.import(
                self.accelerator.as_ref(),
                &state,
                self.area_fluid_active().origin(),
                self.area_fluid_active().dimensions()[0],
                self.area_fluid_active().dimensions()[1],
                self.area_buffered().origin(),
                self.area_buffered().dimensions()[0],
                self.area_buffered().dimensions()[1],
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            );
            state.is_started = true;
            let buffer: wgpu::Buffer = state.buffer.clone();
            let mapped_buffer: wgpu::Buffer = buffer.clone();
            let upload: Arc<Mutex<FluidUpload>> = upload.clone();
            drop(state);
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let bytes: Result<Vec<u8>, io::Error> = match result {
                        Ok(()) => match mapped_buffer.slice(..).get_mapped_range() {
                            Ok(mapped_data) => {
                                let bytes: Vec<u8> = mapped_data.to_vec();
                                drop(mapped_data);
                                mapped_buffer.unmap();
                                Ok(bytes)
                            }
                            Err(error) => {
                                mapped_buffer.unmap();
                                Err(io::Error::other(error.to_string()))
                            }
                        },
                        Err(_) => Err(io::Error::other("Fluid upload result readback failed")),
                    };
                    std::thread::spawn(move || {
                        let result: Result<Vec<ChunkFluidParticle>, io::Error> =
                            bytes.and_then(|bytes| upload.lock().unwrap().failed_particles(&bytes));
                        upload.lock().unwrap().result = Some(result);
                    });
                });
        }
        Ok(())
    }

    /// Queues tile downloads from the `Accelerator`
    pub fn tiles_download(
        &self,
        area: TileArea,
    ) -> impl Future<Output = Result<HashMap<TileCoordinates, TileData>, io::Error>> + 'static {
        let downloads: Vec<Arc<Mutex<TileDownload>>> = area
            .iterate_tile_coordinates()
            .filter_map(|coordinates| {
                self.tile_at(coordinates).map(|tile| {
                    Arc::new(Mutex::new(TileDownload::new(
                        self.accelerator.as_ref(),
                        coordinates,
                        tile,
                    )))
                })
            })
            .collect();
        let mut error: Option<io::Error> = None;
        if let Err(_) = self.tile_downloads.lock().map(|mut tile_downloads| {
            tile_downloads.extend(downloads.iter().cloned());
        }) {
            error = Some(io::Error::other("Tile download queue is unavailable"));
        }
        let mut downloads: Vec<Arc<Mutex<TileDownload>>> = downloads;
        let mut tile_data: HashMap<TileCoordinates, TileData> = HashMap::new();
        poll_fn(move |context| {
            if let Some(error) = error.take() {
                return std::task::Poll::Ready(Err(error));
            }
            let mut index: usize = 0;
            while index < downloads.len() {
                let mut download: std::sync::MutexGuard<TileDownload> =
                    downloads[index].lock().unwrap();
                match download.result.take() {
                    Some(Ok(data)) => {
                        let coordinates: TileCoordinates = download.coordinates;
                        drop(download);
                        downloads.swap_remove(index);
                        tile_data.insert(coordinates, data);
                    }
                    Some(Err(error)) => return std::task::Poll::Ready(Err(error)),
                    None => {
                        download.waker = Some(context.waker().clone());
                        index += 1;
                    }
                }
            }
            std::task::Poll::Ready(Ok(std::mem::take(&mut tile_data)))
        })
    }

    /// Queues tile uploads to the `Accelerator`
    pub fn tiles_upload(
        &mut self,
        area: TileArea,
    ) -> impl Future<Output = Result<(), io::Error>> + 'static {
        let mut error: Option<io::Error> = None;
        let mut uploads: Vec<Arc<Mutex<TileUpload>>> = Vec::new();
        let materials = self.data.materials();
        let ambient_temperature = self.ambient_temperature;
        for coordinates in area.iterate_tile_coordinates() {
            if self.tile_at(coordinates).is_none() {
                continue;
            }
            match self.chunks.get_mut(&coordinates.chunk_coordinates()) {
                Some(ChunkEntry::Active { chunk, .. }) => match chunk.get_tile_mut(coordinates) {
                    Ok(tile_data) => {
                        let tile_data = tile_data;
                        tile_data.resolve_uninitialized_temperatures(|identifier| {
                            materials
                                .thermal_properties(identifier)
                                .and_then(|properties| properties.default_temperature)
                                .unwrap_or(ambient_temperature)
                        });
                        let mut upload = TileUpload::new(coordinates, tile_data);
                        upload.resolve_uninitialized_state(|identifier| {
                            self.initial_temperature(identifier)
                        });
                        uploads.push(Arc::new(Mutex::new(upload)));
                    }
                    Err(()) => {
                        error = Some(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Tile is not in its active chunk",
                        ));
                        break;
                    }
                },
                _ => {
                    error = Some(io::Error::new(
                        io::ErrorKind::NotFound,
                        "Tile chunk is not active",
                    ));
                    break;
                }
            }
        }
        if error.is_none()
            && let Err(_) = self.tile_uploads.lock().map(|mut tile_uploads| {
                tile_uploads.extend(uploads.iter().cloned());
            })
        {
            error = Some(io::Error::other("Tile upload queue is unavailable"));
        }
        poll_fn(move |context| {
            if let Some(error) = error.take() {
                return std::task::Poll::Ready(Err(error));
            }
            let mut index: usize = 0;
            while index < uploads.len() {
                let mut upload: std::sync::MutexGuard<TileUpload> = uploads[index].lock().unwrap();
                match upload.result.take() {
                    Some(Ok(())) => {
                        drop(upload);
                        uploads.swap_remove(index);
                    }
                    Some(Err(error)) => return std::task::Poll::Ready(Err(error)),
                    None => {
                        upload.waker = Some(context.waker().clone());
                        index += 1;
                    }
                }
            }
            std::task::Poll::Ready(Ok(()))
        })
    }

    /// Queues mandatory downloads for tiles leaving GPU residency
    fn tile_downloads_queue(&mut self, area: TileArea) -> Result<(), io::Error> {
        // bind every world coordinate to its physical slot under the old ring mapping
        let mut downloads: Vec<Arc<Mutex<TileDownload>>> = Vec::new();
        for coordinates in area.iterate_tile_coordinates() {
            let tile: Tile = self
                .tile_at(coordinates)
                .ok_or_else(|| io::Error::other("Outgoing tile is outside the old GPU buffer"))?;
            downloads.push(Arc::new(Mutex::new(TileDownload::new(
                self.accelerator.as_ref(),
                coordinates,
                tile,
            ))));
        }
        // share the existing copy and deserialization path while retaining internal ownership
        self.tile_downloads
            .lock()
            .map_err(|_| io::Error::other("Tile download queue is unavailable"))?
            .extend(downloads.iter().cloned());
        self.outgoing_tile_downloads.extend(downloads);
        Ok(())
    }

    /// Returns whether an area contains an outgoing tile awaiting download
    fn tile_download_pending_in(&self, area: TileArea) -> Result<bool, io::Error> {
        for download in &self.outgoing_tile_downloads {
            let coordinates: TileCoordinates = download
                .lock()
                .map_err(|_| io::Error::other("Outgoing tile download is unavailable"))?
                .coordinates;
            if area.contains(coordinates) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns whether a chunk contains an outgoing tile awaiting download
    fn tile_download_pending_for_chunk(
        &self,
        coordinates: TileCoordinates,
    ) -> Result<bool, io::Error> {
        for download in &self.outgoing_tile_downloads {
            let tile_coordinates: TileCoordinates = download
                .lock()
                .map_err(|_| io::Error::other("Outgoing tile download is unavailable"))?
                .coordinates;
            if tile_coordinates.chunk_coordinates() == coordinates {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Applies completed outgoing tile downloads to persistent CPU chunks
    fn tile_downloads_apply_completed(&mut self) -> Result<(), io::Error> {
        let mut index: usize = 0;
        while index < self.outgoing_tile_downloads.len() {
            // leave unfinished and failed jobs pinned so stale chunks cannot be saved
            let mut download: std::sync::MutexGuard<TileDownload> = self.outgoing_tile_downloads
                [index]
                .lock()
                .map_err(|_| io::Error::other("Outgoing tile download is unavailable"))?;
            let Some(result) = download.result.as_ref() else {
                index += 1;
                continue;
            };
            if let Err(error) = result {
                return Err(io::Error::other(format!(
                    "Outgoing tile download failed: {error}"
                )));
            }
            let coordinates: TileCoordinates = download.coordinates;
            if !matches!(
                self.chunks.get(&coordinates.chunk_coordinates()),
                Some(ChunkEntry::Active { .. }),
            ) {
                return Err(io::Error::other(
                    "Outgoing tile download chunk is not active",
                ));
            }
            let tile_data: TileData = download.result.take().unwrap().unwrap();
            drop(download);

            // replace the stale persistence copy and route saving through normal dirty handling
            let Some(ChunkEntry::Active { chunk, is_dirty }) =
                self.chunks.get_mut(&coordinates.chunk_coordinates())
            else {
                unreachable!();
            };
            chunk.set_tile(coordinates, tile_data).map_err(|_| {
                io::Error::other("Outgoing tile download is outside its active chunk")
            })?;
            *is_dirty = true;
            self.outgoing_tile_downloads.swap_remove(index);
        }
        Ok(())
    }

    /// Submits queued GPU tile downloads
    fn tile_downloads_submit(&self) -> Result<(), io::Error> {
        // acquire the download queue
        let mut downloads_started: Vec<(Arc<Mutex<TileDownload>>, wgpu::Buffer)> = Vec::new();
        let mut command_encoder: Option<wgpu::CommandEncoder> = None;
        {
            let downloads = self
                .tile_downloads
                .lock()
                .map_err(|_| io::Error::other("Tile download queue is unavailable"))?;
            // process each incomplete download
            for download in downloads.iter() {
                let mut state: std::sync::MutexGuard<TileDownload> = download.lock().unwrap();
                if state.result.is_some() {
                    continue;
                }
                if state.is_started {
                    continue;
                }
                let tile: Tile = state.physical_tile;
                let command_encoder: &mut wgpu::CommandEncoder = command_encoder
                    .get_or_insert_with(|| {
                        self.accelerator.wgpu_device().create_command_encoder(
                            &wgpu::CommandEncoderDescriptor {
                                label: Some("tile_downloads_submit"),
                            },
                        )
                    });
                command_encoder.copy_buffer_to_buffer(
                    self.cellular_material_identifiers.wgpu_buffer(),
                    tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                    &state.buffer,
                    0,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                );
                command_encoder.copy_buffer_to_buffer(
                    self.cellular_appearances.wgpu_buffer(),
                    tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                    &state.buffer,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                );
                command_encoder.copy_buffer_to_buffer(
                    self.cellular_integrities.wgpu_buffer(),
                    tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                    &state.buffer,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64 * 2,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                );
                command_encoder.copy_buffer_to_buffer(
                    self.cellular_amounts.wgpu_buffer(),
                    tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                    &state.buffer,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64 * 3,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                );
                command_encoder.copy_buffer_to_buffer(
                    self.cellular_temperatures.wgpu_buffer(),
                    tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                    &state.buffer,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64 * 4,
                    TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                );
                state.is_started = true;
                downloads_started.push((download.clone(), state.buffer.clone()));
            }
        }
        if let Some(command_encoder) = command_encoder {
            self.accelerator
                .wgpu_queue()
                .submit(Some(command_encoder.finish()));
            // submit the encoded copies and register their readback callbacks
            for (download, buffer) in downloads_started {
                let mapped_buffer: wgpu::Buffer = buffer.clone();
                buffer
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        let result: Result<TileData, io::Error> = match result {
                            Ok(()) => match mapped_buffer.slice(..).get_mapped_range() {
                                Ok(mapped_data) => {
                                    let mut material_data: &[u8] =
                                        &mapped_data[..TileData::CELL_FIELD_SERIALIZED_SIZE];
                                    let mut appearance_data: &[u8] = &mapped_data
                                        [TileData::CELL_FIELD_SERIALIZED_SIZE
                                            ..TileData::CELL_FIELD_SERIALIZED_SIZE * 2];
                                    let mut integrity_data: &[u8] = &mapped_data
                                        [TileData::CELL_FIELD_SERIALIZED_SIZE * 2
                                            ..TileData::CELL_FIELD_SERIALIZED_SIZE * 3];
                                    let mut amount_data: &[u8] = &mapped_data
                                        [TileData::CELL_FIELD_SERIALIZED_SIZE * 3
                                            ..TileData::CELL_FIELD_SERIALIZED_SIZE * 4];
                                    let mut temperature_data: &[u8] =
                                        &mapped_data[TileData::CELL_FIELD_SERIALIZED_SIZE * 4..];
                                    let tile_data: Result<TileData, io::Error> =
                                        TileData::deserialize_fields(
                                            &mut material_data,
                                            &mut appearance_data,
                                            &mut integrity_data,
                                            &mut amount_data,
                                            &mut temperature_data,
                                        );
                                    drop(mapped_data);
                                    mapped_buffer.unmap();
                                    tile_data
                                }
                                Err(error) => {
                                    mapped_buffer.unmap();
                                    Err(io::Error::other(error.to_string()))
                                }
                            },
                            Err(_) => Err(io::Error::other("Tile download failed")),
                        };
                        let mut download: std::sync::MutexGuard<TileDownload> =
                            download.lock().unwrap();
                        download.result = Some(result);
                        download.is_complete = true;
                        if let Some(waker) = download.waker.take() {
                            waker.wake();
                        }
                    });
            }
        }
        Ok(())
    }

    /// Removes completed GPU tile downloads
    fn tile_downloads_clean(&self) -> Result<(), io::Error> {
        self.tile_downloads
            .lock()
            .map_err(|_| io::Error::other("Tile download queue is unavailable"))?
            .retain(|download| !download.lock().unwrap().is_complete);
        Ok(())
    }

    /// Submits queued GPU tile uploads
    fn tile_uploads_submit(&self) -> Result<(), io::Error> {
        // acquire the pending upload queue
        let uploads = self
            .tile_uploads
            .lock()
            .map_err(|_| io::Error::other("Tile upload queue is unavailable"))?;
        // process each incomplete upload
        for upload in uploads.iter() {
            let mut state: std::sync::MutexGuard<TileUpload> = upload.lock().unwrap();
            if state.result.is_some() {
                continue;
            }
            let Some(tile) = self.tile_at(state.coordinates) else {
                state.result = Some(Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Tile is outside the GPU buffer",
                )));
                state.is_complete = true;
                if let Some(waker) = state.waker.take() {
                    waker.wake();
                }
                continue;
            };
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_material_identifiers.wgpu_buffer(),
                tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                &state.material_identifiers,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_appearances.wgpu_buffer(),
                tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                &state.appearances,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_integrities.wgpu_buffer(),
                tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                &state.integrities,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_amounts.wgpu_buffer(),
                tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                &state.amounts,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_temperatures.wgpu_buffer(),
                tile.0 as u64 * TileData::CELL_FIELD_SERIALIZED_SIZE as u64,
                &state.temperatures,
            );
            self.cellular_dynamic.clear_cellular_dynamic_kinematics(
                self.accelerator.as_ref(),
                tile.0 as usize * 64,
                64,
            );
            self.cellular_pressure.clear_transient_state(
                self.accelerator.as_ref(),
                tile.0 as usize * 64,
                64,
            );
            state.result = Some(Ok(()));
            state.is_complete = true;
            if let Some(waker) = state.waker.take() {
                waker.wake();
            }
        }
        Ok(())
    }

    /// Removes completed GPU tile uploads
    fn tile_uploads_clean(&self) -> Result<(), io::Error> {
        self.tile_uploads
            .lock()
            .map_err(|_| io::Error::other("Tile upload queue is unavailable"))?
            .retain(|upload| !upload.lock().unwrap().is_complete);
        Ok(())
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        self.cellular_material_identifiers.free();
        self.cellular_appearances.free();
        self.cellular_integrities.free();
        self.rigid_cell_integrities.free();
        self.rigid_cell_amounts.free();
        self.rigid_cell_temperatures.free();
        self.fluid_sample_buffer.destroy();
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use engine_graphics::{Color, MaterialAppearance};
    use std::{sync::mpsc, time::Instant};

    #[test]
    fn gas_leaves_and_returns_through_ring_streaming() {
        let _gpu_test = crate::GPU_TEST_LOCK.lock().unwrap();
        let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new().unwrap());
        let mut materials: MaterialRegistry = MaterialRegistry::new();
        let vapor: MaterialIdentifier = materials.register(Material::Gas {
            name: "Vapor".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(120, 160, 190)),
            density: 0.65,
            diffusivity: 0.12,
            extinction: 0.08,
            dissipation: 0.0,
            compressibility: 0.05,
        });
        let mut scene: Scene = Scene::new(
            &accelerator,
            materials,
            SceneSimulationConfiguration {
                gravity: [0.0, -18.0],
                ambient_temperature: 293.15,
                empty_space_thermal_conductivity: 0.0,
                empty_space_heat_capacity: 1.0,
                maximum_gas_concentration: 4.0,
                width: 4,
                height: 4,
                buffer_size: 2,
                streaming_batch_size: 1,
            },
        )
        .unwrap();
        let coordinates: CellCoordinates = CellCoordinates { x: -16, y: 0 };
        let mut edits: SceneEditBatch = SceneEditBatch::new();
        edits.place_material(vapor, CellularAppearance::NEUTRAL, vec![coordinates]);
        scene.apply_edits_immediate(&mut edits).unwrap();
        scene.shift_to(TileCoordinates { x: 1, y: 0 }).unwrap();
        scene.origin_target = scene.origin;
        let started: Instant = Instant::now();
        while !scene.gas_downloads.is_empty()
            || !scene.fluid_downloads.is_empty()
            || !scene.outgoing_tile_downloads.is_empty()
        {
            scene.update(Duration::ZERO, false).unwrap();
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        }
        scene.shift_to(TileCoordinates { x: 0, y: 0 }).unwrap();

        let area: TileArea = TileArea::new(TileCoordinates { x: -2, y: 0 }, 1, 1);
        let download: GasDownload =
            GasDownload::new(accelerator.as_ref(), area, 64, scene.gases.gas_count());
        let buffered_area: TileArea = scene.area_buffered();
        let dimensions: [u16; 2] = buffered_area.dimensions();
        scene.gases.export(
            accelerator.as_ref(),
            &download,
            buffered_area.origin(),
            dimensions[0],
            dimensions[1],
            scene.tiles_ring_offset_x,
            scene.tiles_ring_offset_y,
        );
        let byte_count: u64 = 64 * u64::from(5 + scene.gases.gas_count()) * 4;
        let (sender, receiver) = mpsc::sync_channel(1);
        download
            .buffer
            .slice(0..byte_count)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        let started: Instant = Instant::now();
        loop {
            accelerator.poll().unwrap();
            if let Ok(result) = receiver.try_recv() {
                result.unwrap();
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        }
        let mapped = download
            .buffer
            .slice(0..byte_count)
            .get_mapped_range()
            .unwrap();
        let bytes: Vec<u8> = mapped.to_vec();
        drop(mapped);
        download.buffer.unmap();
        let restored: Vec<ChunkGasCell> = GasDownload::deserialize(&bytes, area, &[vapor]).unwrap();
        assert!(
            restored
                .iter()
                .any(|cell| cell.coordinates == coordinates && cell.species == vec![(vapor, 1.0)])
        );
    }

    #[test]
    fn cellular_indirect_dispatch_executes() {
        let _gpu_test = crate::GPU_TEST_LOCK.lock().unwrap();
        let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new().unwrap());
        let mut materials: MaterialRegistry = MaterialRegistry::new();
        let sand: MaterialIdentifier = materials.register(Material::CellularDynamic {
            name: "Sand".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(194, 178, 128)),
            mass: 1.0,
            pressure_transmission: 0.35,
            friction: 0.65,
            restitution: 0.05,
        });
        let mut scene: Scene = Scene::new(
            &accelerator,
            materials,
            SceneSimulationConfiguration {
                gravity: [0.0, -18.0],
                ambient_temperature: 293.15,
                empty_space_thermal_conductivity: 0.0,
                empty_space_heat_capacity: 1.0,
                maximum_gas_concentration: 4.0,
                width: 4,
                height: 4,
                buffer_size: 2,
                streaming_batch_size: 1,
            },
        )
        .unwrap();
        let mut edits: SceneEditBatch = SceneEditBatch::new();
        edits.place_material(
            sand,
            CellularAppearance::NEUTRAL,
            vec![CellCoordinates { x: 0, y: 8 }],
        );
        scene.apply_edits_immediate(&mut edits).unwrap();
        scene.update(Duration::from_secs(1) / 60, true).unwrap();
        accelerator.poll().unwrap();
    }

    #[test]
    fn full_screen_moving_sand_headless_tps() {
        let _gpu_test = crate::GPU_TEST_LOCK.lock().unwrap();
        let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new().unwrap());
        let mut materials = MaterialRegistry::new();
        let sand = materials.register(Material::CellularDynamic {
            name: "Sand".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(194, 178, 128)),
            mass: 1.0,
            pressure_transmission: 0.35,
            friction: 0.65,
            restitution: 0.05,
        });
        let data_path = std::env::temp_dir().join(format!(
            "dogwood-sand-stress-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&data_path).unwrap();
        materials
            .serialize(&mut std::fs::File::create(data_path.join("materials")).unwrap())
            .unwrap();
        let data = SceneData::load(data_path.clone()).unwrap();
        let mut scene = Scene::load(
            &accelerator,
            SceneSimulationConfiguration {
                gravity: [0.0, -18.0],
                ambient_temperature: 293.15,
                empty_space_thermal_conductivity: 0.0,
                empty_space_heat_capacity: 1.0,
                maximum_gas_concentration: 4.0,
                width: 8,
                height: 8,
                buffer_size: 2,
                streaming_batch_size: 1,
            },
            data,
        )
        .unwrap();
        let mut edits = SceneEditBatch::new();
        edits.place_material(
            sand,
            CellularAppearance::NEUTRAL,
            (8..64)
                .step_by(2)
                .flat_map(|y| (0..64).map(move |x| CellCoordinates { x, y }))
                .collect(),
        );
        scene.apply_edits_immediate(&mut edits).unwrap();
        let tick = Duration::from_secs(1) / 60;
        for _ in 0..5 {
            scene.update(tick, true).unwrap();
        }
        let start = Instant::now();
        let mut older_snapshots = 0;
        let mut maximum_snapshot_age = 0;
        for _ in 0..30 {
            scene.update(tick, true).unwrap();
            let age = scene
                .physics_world
                .terrain_bridge_statistics()
                .collision_snapshot_age;
            maximum_snapshot_age = maximum_snapshot_age.max(age);
            older_snapshots += u32::from(age > 1);
        }
        let elapsed = start.elapsed();
        let stats = scene.physics_world.terrain_bridge_statistics();
        assert_eq!(stats.dynamic_shape_rebuilds, 0);
        assert_eq!(stats.dynamic_cells_scanned, 0);
        eprintln!(
            "moving sand headless TPS: {:.1}, snapshot age max {}, ticks >1 {}",
            30.0 / elapsed.as_secs_f64(),
            maximum_snapshot_age,
            older_snapshots
        );
        drop(scene);
        std::fs::remove_dir_all(data_path).unwrap();
    }

    #[test]
    fn disconnected_static_component_becomes_one_falling_rigid_body() {
        let _gpu_test = crate::GPU_TEST_LOCK.lock().unwrap();
        let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new().unwrap());
        let mut materials: MaterialRegistry = MaterialRegistry::new();
        let stone: MaterialIdentifier = materials.register(Material::CellularStatic {
            name: "Stone".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(110, 105, 100)),
            mass: 1.0,
            pressure_ignore_threshold: 1.0,
            default_integrity: 1.0,
            minimum_rigid_body_cell_count: 4,
            debris_material: None,
            debris_yield_rate: 0.0,
            pressure_transmission: 0.5,
            friction: 0.7,
            restitution: 0.05,
        });
        let mut scene: Scene = Scene::new(
            &accelerator,
            materials,
            SceneSimulationConfiguration {
                gravity: [0.0, -8.0],
                ambient_temperature: 293.15,
                empty_space_thermal_conductivity: 0.0,
                empty_space_heat_capacity: 1.0,
                maximum_gas_concentration: 4.0,
                width: 1,
                height: 1,
                buffer_size: 2,
                streaming_batch_size: 1,
            },
        )
        .unwrap();
        let cells: Vec<CellCoordinates> = (2..=5)
            .flat_map(|x| (3..=4).map(move |y| CellCoordinates { x, y }))
            .chain((0..=2).map(|y| CellCoordinates { x: 3, y }))
            .collect();
        let mut edits = SceneEditBatch::new();
        edits.place_material(stone, CellularAppearance::NEUTRAL, cells.clone());
        scene.apply_edits_immediate(&mut edits).unwrap();
        let mut static_masks = vec![[0u32; 2]; 25];
        for cell in &cells {
            let tile_x = cell.x.div_euclid(8) + 2;
            let tile_y = cell.y.div_euclid(8) + 2;
            let tile = (tile_y * 5 + tile_x) as usize;
            let local = (cell.y.rem_euclid(8) * 8 + cell.x.rem_euclid(8)) as usize;
            static_masks[tile][local / 32] |= 1 << (local % 32);
        }
        let baseline = CollisionOccupancySnapshot {
            sequence: 0,
            origin: TileCoordinates { x: -2, y: -2 },
            width: 5,
            height: 5,
            static_masks: static_masks.into_boxed_slice(),
            dynamic_masks: vec![[0, 0]; 25].into_boxed_slice(),
        };
        scene
            .detach_unanchored_static_components(&mut baseline.clone())
            .unwrap();
        assert!(scene.rigid_cellular_bodies.is_empty());
        let mut separated = baseline.clone();
        separated.sequence = 1;
        separated.clear_static_cell(3, 2);
        scene
            .detach_unanchored_static_components(&mut separated)
            .unwrap();
        assert!(scene.rigid_cellular_bodies.len() == 1);
        assert!(scene.rigid_cellular_bodies[0].cells.len() == 8);
        let initial_y = scene
            .physics_world
            .rigid_cellular_body_state(&scene.rigid_cellular_bodies[0])
            .unwrap()
            .translation[1];
        scene.physics_world.update_cellular_snapshot(separated);
        for _ in 0..8 {
            scene
                .physics_world
                .step(scene.gravity, 1.0 / TICK_RATE as f32);
        }
        let state = scene
            .physics_world
            .rigid_cellular_body_state(&scene.rigid_cellular_bodies[0])
            .unwrap();
        assert!(state.translation[1] < initial_y);
        scene.cellular_physics_body_proxy.rasterize(
            accelerator.as_ref(),
            TileCoordinates { x: -2, y: -2 },
            5,
            5,
            0,
            0,
            scene.gravity,
            &[],
            &scene.rigid_cellular_bodies,
            &[state],
            scene.rigid_cellular_topology_revision,
        );
        accelerator.poll().unwrap();
    }

    #[test]
    fn queued_authored_rigid_body_is_atomic_and_body_local() {
        let _gpu_test = crate::GPU_TEST_LOCK.lock().unwrap();
        let accelerator = Arc::new(Accelerator::new().unwrap());
        let mut materials = MaterialRegistry::new();
        let stone = materials.register(Material::CellularStatic {
            name: "Stone".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(1, 1, 1)),
            mass: 2.0,
            pressure_ignore_threshold: 1.0,
            default_integrity: 1.0,
            minimum_rigid_body_cell_count: 1,
            debris_material: None,
            debris_yield_rate: 0.0,
            pressure_transmission: 1.0,
            friction: 0.7,
            restitution: 0.05,
        });
        let sand = materials.register(Material::CellularDynamic {
            name: "Sand".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(1, 1, 1)),
            mass: 1.0,
            pressure_transmission: 1.0,
            friction: 0.5,
            restitution: 0.0,
        });
        let mut scene = Scene::new(
            &accelerator,
            materials,
            SceneSimulationConfiguration {
                gravity: [0.0, -8.0],
                ambient_temperature: 293.15,
                empty_space_thermal_conductivity: 0.0,
                empty_space_heat_capacity: 1.0,
                maximum_gas_concentration: 4.0,
                width: 4,
                height: 4,
                buffer_size: 2,
                streaming_batch_size: 1,
            },
        )
        .unwrap();
        let mut edits = SceneEditBatch::new();
        edits.place_rigid_body(vec![
            SceneEditCellPlacement {
                coordinates: CellCoordinates { x: 10, y: 20 },
                material_identifier: stone,
                appearance: CellularAppearance(3),
            },
            SceneEditCellPlacement {
                coordinates: CellCoordinates { x: 11, y: 20 },
                material_identifier: stone,
                appearance: CellularAppearance(4),
            },
            SceneEditCellPlacement {
                coordinates: CellCoordinates { x: 10, y: 20 },
                material_identifier: stone,
                appearance: CellularAppearance(5),
            },
        ]);
        edits.place_rigid_body(vec![SceneEditCellPlacement {
            coordinates: CellCoordinates { x: 12, y: 20 },
            material_identifier: sand,
            appearance: CellularAppearance::NEUTRAL,
        }]);
        scene.queue_edits(edits);
        scene.update(Duration::ZERO, false).unwrap();
        assert_eq!(scene.rigid_cellular_bodies.len(), 1);
        let body = &scene.rigid_cellular_bodies[0];
        assert_eq!(body.cells.len(), 2);
        assert_eq!(body.cells[0].local, [0, 0]);
        assert_eq!(body.cells[1].local, [1, 0]);
        assert_eq!(body.cells[0].appearance.0, 5);
        assert_eq!(scene.rigid_cellular_topology_revision, 1);
    }
}
