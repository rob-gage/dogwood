// Copyright Rob Gage 2026

use super::{
    FluidDownload,
    FluidUpload,
    SceneData,
    SceneEditCellPlacement,
    SceneEdit,
    SceneEditBatch,
    SceneGenerator,
    ScenePosition,
    SceneVelocity,
    TileDownload,
    TileUpload,
};
use crate::simulation::{
    CellularCollision,
    CellularPhysicsBodyProxy,
    CellularDynamic,
    CellularPressure,
    Fluids,
    SceneSimulationConfiguration,
    ScenePhysicsWorld,
};
use crate::{
    actors::{
        Actor,
        ActorRegistry,
    },
    chunks::{
        Chunk,
        ChunkEntry,
        ChunkStreamingResponse,
        ChunkFluidParticle,
    },
    materials::{
        Material,
        MaterialIdentifier,
        MaterialRegistry,
    },
    tiles::{
        CellCoordinates,
        CellularAppearance,
        Tile,
        TileArea,
        TileCoordinates,
        TileData,
    },
};
use engine_compute::{
    Accelerator,
    AcceleratorBuffer
};
use engine_graphics::{
    MaterialGraphics,
    SceneGraphics,
};
use std::{
    collections::HashMap,
    error::Error,
    future::poll_fn,
    io,
    sync::{
        Arc,
        Mutex,
        mpsc::{
            Receiver,
            SyncSender,
            sync_channel,
        }
    },
    time::Duration,
};

/// The capacity of the chunk streaming queue
pub const CHUNK_STREAMING_QUEUE_CAPACITY: usize = 64;

/// The fixed scene tick rate
const TICK_RATE: u32 = 60;

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
    /// The tile uploads pending processing by `tick`
    tile_uploads: Mutex<Vec<Arc<Mutex<TileUpload>>>>,
    /// Fluid imports that own dormant records until GPU reconstruction is confirmed
    fluid_uploads: Vec<Arc<Mutex<FluidUpload>>>,
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
    /// Transient rasterized possessed-pawn interaction geometry
    cellular_physics_body_proxy: CellularPhysicsBodyProxy,
    /// GPU-authoritative fluid particles and their transient cellular representation
    fluids: Fluids,
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
    /// CPU collision and rigid-body world, including terrain derived from cellular occupancy
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
        let material_graphics: MaterialGraphics =
            data.materials().build_material_graphics(accelerator.as_ref());
        let generator: Arc<dyn SceneGenerator> = Arc::new(generator);
        let buffer_size: u16 = u16::from(simulation.buffer_size) * 2;
        let buffered_tile_count: usize =
            (simulation.width + buffer_size) as usize *
            (simulation.height + buffer_size) as usize;
        let buffered_cell_count: usize = buffered_tile_count * 64;
        let cellular_material_identifiers: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_cell_count);
        let cellular_appearances: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_cell_count);
        let cellular_integrities: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count);
        let cellular_physics_body_proxy = CellularPhysicsBodyProxy::new(accelerator.as_ref(), buffered_cell_count);
        let fluids: Fluids = Fluids::new(
            accelerator.as_ref(),
            &cellular_material_identifiers,
            cellular_physics_body_proxy.occupancy_buffer(),
            cellular_physics_body_proxy.velocity_buffer(),
            &material_graphics.fluid_properties,
            simulation.width + buffer_size,
            simulation.height + buffer_size,
        );
        let cellular_dynamic: CellularDynamic = CellularDynamic::new(
            accelerator.as_ref(),
            &cellular_material_identifiers,
            &cellular_appearances,
            cellular_physics_body_proxy.occupancy_buffer(),
            buffered_tile_count,
        );
        let cellular_pressure: CellularPressure = CellularPressure::new(
            accelerator.as_ref(),
            data.materials(),
            &cellular_material_identifiers,
            &cellular_appearances,
            &cellular_integrities,
            cellular_dynamic.kinematics_buffer(),
            cellular_physics_body_proxy.occupancy_buffer(),
            cellular_physics_body_proxy.velocity_buffer(),
            cellular_physics_body_proxy.count_buffer(),
            buffered_cell_count,
        );
        let cellular_collision: CellularCollision = CellularCollision::new(
            accelerator.as_ref(),
            &cellular_material_identifiers,
            cellular_physics_body_proxy.occupancy_buffer(),
            simulation.width + buffer_size,
            simulation.height + buffer_size,
        );
        let fluid_download_pool: Vec<Arc<Mutex<FluidDownload>>> = vec![Arc::new(Mutex::new(
            FluidDownload::new(
                accelerator.as_ref(),
                TileArea::new(TileCoordinates { x: 0, y: 0 }, 1, 1),
                fluids.particle_capacity(),
            ),
        ))];
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
            tile_uploads: Mutex::new(Vec::new()),
            fluid_uploads: Vec::new(),
            cellular_material_identifiers,
            cellular_appearances,
            cellular_integrities,
            cellular_physics_body_proxy,
            fluids,
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
            scene.chunks.insert(coordinates, ChunkEntry::Active {
                chunk,
                is_dirty: false,
            });
        }
        drop(scene.tiles_upload(scene.area_buffered()));
        scene.fluid_uploads_queue(scene.area_buffered())?;
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
        let walking_pawn: Option<([f32; 2], [f32; 2])> = self.actor_registry
            .first_walking_pawn_graphics(self.tick_interpolation());
        SceneGraphics {
            material_graphics: &self.material_graphics,
            cellular_material_identifiers: &self.cellular_material_identifiers,
            cellular_appearances: &self.cellular_appearances,
            fluid_material_identifiers: self.fluids.material_identifiers_buffer(),
            fluid_coverage: self.fluids.coverage_buffer(),
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
    pub fn materials(&self) -> &MaterialRegistry { self.data.materials() }

    /// Returns the `ActorRegistry` for this `Scene`
    pub const fn actor_registry(&self) -> &ActorRegistry { &self.actor_registry }

    /// Returns mutable access to the `ActorRegistry` for this `Scene`
    pub const fn actor_registry_mutable(&mut self) -> &mut ActorRegistry
    { &mut self.actor_registry }

    /// Returns the currently possessed actor if one exists
    pub fn possessed_actor(&self) -> Option<Actor> {
        self.possessed_actor.filter(|actor| self.actor_registry.contains(*actor))
    }

    /// Returns an actor's position interpolated between its latest fixed ticks
    pub fn actor_render_position(&self, actor: Actor) -> Option<ScenePosition> {
        self.actor_registry.get_render_position(actor, self.tick_interpolation())
    }

    /// Possesses an actor if it exists in this `Scene`
    pub fn possess_actor(&mut self, identifier: Actor) -> bool {
        if !self.actor_registry.is_possessable(identifier) { return false; }
        if self.possessed_actor != Some(identifier) && let Some(possessed) = self.possessed_actor {
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

    /// Applies queued material edits to resident cellular world state
    pub fn apply_edits(&mut self, edits: &mut SceneEditBatch) -> Result<(), io::Error> {
        let mut cell_edits: HashMap<usize, (CellCoordinates, MaterialIdentifier, CellularAppearance, f32)> =
            HashMap::new();
        let mut fluid_edits: HashMap<usize, u32> = HashMap::new();
        for edit in edits.drain() {
            match edit {
                SceneEdit::PlaceCells { cells } => {
                    for SceneEditCellPlacement {
                        coordinates,
                        material_identifier,
                        appearance,
                    } in cells {
                        if let Some(physical_index) = self.cell_edit_index(coordinates) {
                            match self.data.materials().get(material_identifier) {
                                Some(Material::CellularStatic { default_integrity, .. }) => {
                                    cell_edits.insert(physical_index, (
                                        coordinates, material_identifier, appearance, *default_integrity,
                                    ));
                                    fluid_edits.insert(physical_index, Fluids::erase_edit());
                                }
                                Some(Material::CellularDynamic { .. }) => {
                                    cell_edits.insert(physical_index, (
                                        coordinates, material_identifier, appearance, 0.0,
                                    ));
                                    fluid_edits.insert(physical_index, Fluids::erase_edit());
                                }
                                Some(Material::Fluid { .. }) => {
                                    cell_edits.insert(physical_index, (
                                        coordinates, MaterialIdentifier::NULL,
                                        CellularAppearance::NEUTRAL, 0.0,
                                    ));
                                    fluid_edits.insert(physical_index, material_identifier.as_u32());
                                }
                                None => { }
                            }
                        }
                    }
                }
                SceneEdit::Erase { cells } => {
                    for coordinates in cells {
                        if let Some(physical_index) = self.cell_edit_index(coordinates) {
                            cell_edits.insert(physical_index, (
                                coordinates,
                                MaterialIdentifier::NULL,
                                CellularAppearance::NEUTRAL,
                                0.0,
                            ));
                            fluid_edits.insert(physical_index, Fluids::erase_edit());
                        }
                    }
                }
            }
        }
        let mut cell_edits: Vec<(usize, CellCoordinates, MaterialIdentifier, CellularAppearance, f32)> =
            cell_edits.into_iter().map(|(index, (coordinates, material_identifier, appearance, integrity))| {
                (index, coordinates, material_identifier, appearance, integrity)
            }).collect();
        cell_edits.sort_unstable_by_key(|(physical_index, ..)| *physical_index);
        for (_, coordinates, material_identifier, appearance, integrity) in &cell_edits {
            let tile_coordinates: TileCoordinates = coordinates.tile_coordinates();
            let [x, y]: [usize; 2] = coordinates.local_tile_coordinates();
            let Some(ChunkEntry::Active { chunk, is_dirty }) =
                self.chunks.get_mut(&tile_coordinates.chunk_coordinates())
            else { return Err(io::Error::other("Resident tile chunk is not active")); };
            chunk.set_cell_with_integrity(
                tile_coordinates,
                x,
                y,
                *material_identifier,
                *appearance,
                *integrity,
            ).map_err(|_| io::Error::other("Resident tile is not in its active chunk"))?;
            *is_dirty = true;
        }
        if !cell_edits.is_empty() {
            self.cellular_collision_dirty = true;
            self.write_cell_edits(&cell_edits);
        }
        if !fluid_edits.is_empty() {
            let mut fluid_edits: Vec<(usize, u32)> = fluid_edits.into_iter().collect();
            fluid_edits.sort_unstable_by_key(|(physical_index, _)| *physical_index);
            let buffer_size: i32 = i32::from(self.simulation_buffer_size);
            let dimensions: u16 = u16::from(self.simulation_buffer_size) * 2;
            let fluid_active_area: TileArea = self.area_fluid_active();
            let fluid_active_dimensions: [u16; 2] = fluid_active_area.dimensions();
            self.fluids.apply_edits(
                self.accelerator.as_ref(), &fluid_edits,
                fluid_active_area.origin(),
                fluid_active_dimensions[0], fluid_active_dimensions[1],
                TileCoordinates { x: self.origin.x - buffer_size, y: self.origin.y - buffer_size },
                self.simulation_width + dimensions, self.simulation_height + dimensions,
                self.tiles_ring_offset_x, self.tiles_ring_offset_y,
            );
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
            TileCoordinates { x: self.origin.x - buffer_size, y: self.origin.y - buffer_size },
            self.simulation_width + dimensions,
            self.simulation_height + dimensions,
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
            center,
            radius_cells.max(0.75),
            strength,
        );
    }

    /// Handles `Scene` streaming and fixed-rate simulation
    pub fn update(
        &mut self,
        elapsed: Duration,
        is_simulation_active: bool,
    ) -> Result<(), io::Error> {
        if let Some(origin_target) = self.area_request.take() {
            self.origin_target = origin_target;
        } else {
            let possessed_position: Option<ScenePosition> = self.possessed_actor()
                .and_then(|actor| self.actor_registry.get_position(actor)).copied();
            if let Some(position) = possessed_position { self.follow_position(position); }
        }
        self.chunks_refresh()?;
        self.tile_downloads_submit()?;
        self.tile_uploads_submit()?;
        self.fluid_downloads_submit()?;
        self.fluid_uploads_submit()?;
        self.accelerator.poll().map_err(|error| io::Error::other(error.to_string()))?;
        self.cellular_collision.collect_collision()?;
        self.tile_downloads_apply_completed()?;
        self.fluid_downloads_apply_completed()?;
        self.fluid_uploads_apply_completed()?;
        self.tile_downloads_clean()?;
        self.tile_uploads_clean()?;
        self.tick_time += elapsed;
        let tick_time: Duration = Duration::from_secs(1) / TICK_RATE;
        while self.tick_time >= tick_time {
            self.tick(is_simulation_active)?;
            self.tick_time -= tick_time;
        }
        Ok(())
    }

    /// Returns progress from the previous fixed tick to the current fixed tick
    fn tick_interpolation(&self) -> f32 {
        (self.tick_time.as_secs_f32() * TICK_RATE as f32).clamp(0.0, 1.0)
    }

    /// Runs one fixed-rate physics simulation tick
    fn tick(&mut self, is_simulation_active: bool) -> Result<(), io::Error> {
        if let Some(snapshot) = self.cellular_collision.latest.take() {
            self.physics_world.update_cellular_terrain(snapshot);
        }
        if is_simulation_active {
            self.physics_world.step(self.gravity, 1.0 / TICK_RATE as f32);
        }
        self.actor_registry.simulate_actor_pawns(
            1.0 / TICK_RATE as f32,
            is_simulation_active,
            self.gravity,
            &self.physics_world,
        );
        let current_walking_pawn: Option<([f32; 2], [f32; 2], [f32; 2], [f32; 2])> =
            self.possessed_actor().and_then(|actor| {
                self.actor_registry.walking_pawn_physics(actor)
            });
        let possessed_position: Option<ScenePosition> = self.possessed_actor()
            .and_then(|actor| self.actor_registry.get_position(actor)).copied();
        if let Some(position) = possessed_position { self.follow_position(position); }
        if is_simulation_active {
            let buffer_size: i32 = i32::from(self.simulation_buffer_size);
            let dimensions: u16 = u16::from(self.simulation_buffer_size) * 2;
            let fluid_active_area: TileArea = self.area_fluid_active();
            let fluid_active_dimensions: [u16; 2] = fluid_active_area.dimensions();
            self.cellular_physics_body_proxy.rasterize(
                self.accelerator.as_ref(), TileCoordinates { x: self.origin.x - buffer_size, y: self.origin.y - buffer_size },
                self.simulation_width + dimensions, self.simulation_height + dimensions, self.tiles_ring_offset_x,
                self.tiles_ring_offset_y, self.gravity, current_walking_pawn,
            );
            self.cellular_pressure.simulate(
                self.accelerator.as_ref(), TileCoordinates { x: self.origin.x - buffer_size, y: self.origin.y - buffer_size },
                self.simulation_width + dimensions, self.simulation_height + dimensions, self.tiles_ring_offset_x,
                self.tiles_ring_offset_y, 1.0 / TICK_RATE as f32,
            );
            self.cellular_dynamic.simulate_cellular_dynamic_tick(
                self.accelerator.as_ref(),
                &self.cellular_material_identifiers,
                &self.cellular_appearances,
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
            self.cellular_collision_dirty = true;
        }
        if !self.cellular_collision_dirty { return Ok(()); }
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
        )? { self.cellular_collision_dirty = false; }
        Ok(())
    }

    /// Resolves one world cell to a resident physical GPU cell
    fn cell_edit_index(&self, coordinates: CellCoordinates) -> Option<usize> {
        let tile_coordinates: TileCoordinates = coordinates.tile_coordinates();
        let tile: Tile = self.tile_at(tile_coordinates)?;
        let [x, y]: [usize; 2] = coordinates.local_tile_coordinates();
        if !matches!(
            self.chunks.get(&tile_coordinates.chunk_coordinates()),
            Some(ChunkEntry::Active { chunk, .. }) if chunk.get_tile(tile_coordinates).is_ok()
        ) { return None; }
        Some(tile.0 as usize * 64 + y * 8 + x)
    }

    /// Writes final contiguous cellular edits to the two authoritative GPU buffers
    fn write_cell_edits(
        &self,
        edits: &[(usize, CellCoordinates, MaterialIdentifier, CellularAppearance, f32)],
    ) {
        let mut start: usize = 0;
        while start < edits.len() {
            let mut end: usize = start + 1;
            while end < edits.len() &&
                    edits[end].0 == edits[end - 1].0 + 1 {
                end += 1;
            }
            let mut material_identifiers: Vec<u8> = Vec::with_capacity((end - start) * 4);
            let mut appearances: Vec<u8> = Vec::with_capacity((end - start) * 4);
            let mut integrities: Vec<u8> = Vec::with_capacity((end - start) * 4);
            for edit in &edits[start..end] {
                material_identifiers.extend_from_slice(&edit.2.as_u32().to_le_bytes());
                appearances.extend_from_slice(&edit.3.0.to_le_bytes());
                integrities.extend_from_slice(&edit.4.to_bits().to_le_bytes());
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

    /// Sets the automatic active-area target around a world position
    fn follow_position(&mut self, position: ScenePosition) {
        self.origin_target = TileCoordinates {
            x: position.tile_coordinates.x - i32::from(self.simulation_width) / 2,
            y: position.tile_coordinates.y - i32::from(self.simulation_height) / 2,
        };
    }

    /// Returns the exact tile area currently being simulated
    const fn area_active(&self) -> TileArea {
        TileArea::new(
            self.origin,
            self.simulation_width,
            self.simulation_height,
        )
    }

    /// Returns the moving-fluid area including one camera-streaming batch outside the viewport
    fn area_fluid_active(&self) -> TileArea {
        let padding: u16 = u16::from(self.tile_streaming_batch_size);
        self.area_active().expanded(padding, padding, padding, padding)
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
    fn area_streaming(&self) -> TileArea { self.area_buffered().chunk_area() }

    /// Returns the current chunk area plus one chunk in each movement direction
    fn area_prefetching(&self) -> TileArea {
        let velocity: Option<SceneVelocity> = self.possessed_actor()
            .and_then(|actor| self.actor_registry.get_velocity(actor)).copied();
        let left: bool = self.origin_target.x < self.origin.x ||
            velocity.is_some_and(|velocity| velocity.x < 0.0);
        let bottom: bool = self.origin_target.y < self.origin.y ||
            velocity.is_some_and(|velocity| velocity.y < 0.0);
        let right: bool = self.origin_target.x > self.origin.x ||
            velocity.is_some_and(|velocity| velocity.x > 0.0);
        let top: bool = self.origin_target.y > self.origin.y ||
            velocity.is_some_and(|velocity| velocity.y > 0.0);
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
                Some(ChunkEntry::Active { .. }) |
                Some(ChunkEntry::Loading { .. }) |
                Some(ChunkEntry::Generating { .. }) |
                Some(ChunkEntry::Saving { .. }) => { },
            };
        }
        Ok(())
    }

    /// Reads an unloaded chunk into this `Scene`
    fn chunk_load(&mut self, coordinates: TileCoordinates) -> Result<(), io::Error> {
        // avoid duplicate work
        if let Some(entry) = self.chunks.get(&coordinates) {
            match entry {
                ChunkEntry::Active { .. } |
                ChunkEntry::Loading { .. } |
                ChunkEntry::Generating { .. } |
                ChunkEntry::Saving { .. } =>
                    return Ok(()),
                _ => (),
            }
        }
        let streaming_identifier: u64 = self.chunks_streaming_identifier_next;
        self.chunks_streaming_identifier_next =
            self.chunks_streaming_identifier_next.wrapping_add(1);
        self.chunks.insert(coordinates, ChunkEntry::Loading { streaming_identifier, });
        let data: SceneData = self.data.clone();
        let sender: SyncSender<ChunkStreamingResponse> =
            self.chunk_streaming_response_sender.clone();
        // spawn new thread to attempt read
        std::thread::spawn(move || {
            let result: Result<Option<Box<Chunk>>, Box<dyn Error + Send + Sync>> =
                data.read_chunk(coordinates)
                    .map(|chunk| chunk.map(Box::new))
                    .map_err(|error| Box::new(error).into());
            sender.send(ChunkStreamingResponse::Loaded {
                streaming_identifier,
                coordinates,
                result,
            }).unwrap();
        });
        Ok(())
    }

    /// Generates a new chunk for this `Scene`
    fn chunk_generate(&mut self, coordinates: TileCoordinates) -> Result<(), io::Error> {
        // avoid duplicate work
        if let Some(entry) = self.chunks.get(&coordinates) {
            match entry {
                ChunkEntry::Active { .. } |
                ChunkEntry::Generating { .. } |
                ChunkEntry::Saving { .. } =>
                    return Ok(()),
                _ => (),
            }
        }
        let streaming_identifier: u64 = self.chunks_streaming_identifier_next;
        self.chunks_streaming_identifier_next =
            self.chunks_streaming_identifier_next.wrapping_add(1);
        self.chunks.insert(coordinates, ChunkEntry::Generating {
            streaming_identifier,
        });
        let generator: Arc<dyn SceneGenerator> = self.generator.clone();
        let sender: SyncSender<ChunkStreamingResponse> =
            self.chunk_streaming_response_sender.clone();
        // spawn new thread for generation
        std::thread::spawn(move || {
            let result: Result<Box<Chunk>, Box<dyn Error + Send + Sync>> =
                Ok(Box::new(generator.generate_chunk(coordinates)));
            sender.send(ChunkStreamingResponse::Generated {
                streaming_identifier,
                coordinates,
                result,
            }).unwrap();
        });
        Ok(())
    }

    /// Saves dirty chunks and removes entries outside the one-chunk retention region
    fn chunks_save(&mut self) -> Result<(), io::Error> {
        let retention_area: TileArea = self.area_streaming().expanded(
            Chunk::WIDTH,
            Chunk::WIDTH,
            Chunk::WIDTH,
            Chunk::WIDTH,
        );
        let coordinates: Vec<TileCoordinates> = self.chunks.iter().filter_map(
            |(coordinates, entry)| {
                if retention_area.contains(*coordinates) || matches!(
                    entry,
                    ChunkEntry::Loading { .. } |
                    ChunkEntry::Generating { .. } |
                    ChunkEntry::Saving { .. },
                ) { None } else { Some(*coordinates) }
            }
        ).collect();
        for coordinates in coordinates {
            // keep stale CPU chunks unavailable to save or removal until downloads are applied
            if self.tile_download_pending_for_chunk(coordinates)? ||
                    self.fluid_transfer_pending_for_chunk(coordinates)? { continue; }
            let Some(entry) = self.chunks.remove(&coordinates) else { continue; };
            let ChunkEntry::Active { chunk, is_dirty: true } = entry else { continue; };
            let streaming_identifier: u64 = self.chunks_streaming_identifier_next;
            self.chunks_streaming_identifier_next =
                self.chunks_streaming_identifier_next.wrapping_add(1);
            self.chunks.insert(coordinates, ChunkEntry::Saving { streaming_identifier });
            let data: SceneData = self.data.clone();
            let sender: SyncSender<ChunkStreamingResponse> =
                self.chunk_streaming_response_sender.clone();
            std::thread::spawn(move || {
                let result: Result<Box<Chunk>, (Box<Chunk>, io::Error)> =
                    match data.write_chunk(&chunk) {
                        Ok(()) => Ok(Box::new(chunk)),
                        Err(error) => Err((Box::new(chunk), error)),
                    };
                sender.send(ChunkStreamingResponse::Saved {
                    streaming_identifier,
                    coordinates,
                    result,
                }).unwrap();
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
                    if !matches_request { continue; }
                    match result { // missing chunk begins a separate generation operation
                        Ok(Some(chunk)) => {
                            self.chunks.insert(coordinates, ChunkEntry::Active {
                                chunk: *chunk,
                                is_dirty: false,
                            });
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
                    ) { continue; }
                    match result { // retain the successfully generated chunk
                        Ok(chunk) => {
                            self.chunks.insert(coordinates, ChunkEntry::Active {
                                chunk: *chunk,
                                is_dirty: false,
                            });
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
                    ) { continue; }
                    match result {
                        Ok(chunk) => {
                            if self.area_prefetching().contains(coordinates) {
                                self.chunks.insert(coordinates, ChunkEntry::Active {
                                    chunk: *chunk,
                                    is_dirty: false,
                                });
                                chunks_available.push(coordinates);
                            } else {
                                self.chunks.remove(&coordinates);
                            }
                        }
                        Err((chunk, error)) => {
                            self.chunks.insert(coordinates, ChunkEntry::Active {
                                chunk: *chunk,
                                is_dirty: true,
                            });
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
        let buffered_area: TileArea = TileArea::new(TileCoordinates {
            x: new_origin.x - buffer_size,
            y: new_origin.y - buffer_size,
        }, width, height);
        let streaming_area: TileArea = buffered_area.chunk_area();
        self.chunks_fetch(streaming_area)?;
        if !streaming_area.iterate_chunk_coordinates().all(|coordinates| { matches!(
            self.chunks.get(&coordinates),
            Some(ChunkEntry::Active { .. })
        ) }) { return Ok(()); }
        let batch_size: u16 = u16::from(self.tile_streaming_batch_size);
        let old_buffered_origin: TileCoordinates = TileCoordinates {
            x: self.origin.x - buffer_size,
            y: self.origin.y - buffer_size,
        };
        let tiles_download_area: TileArea;
        let tiles_upload_area: TileArea;
        if new_origin.x > self.origin.x {
            tiles_download_area = TileArea::new(
                old_buffered_origin,
                batch_size,
                height,
            );
            tiles_upload_area = TileArea::new(TileCoordinates {
                x: new_origin.x - buffer_size + width as i32 - batch_size as i32,
                y: new_origin.y - buffer_size,
            }, batch_size, height);
        } else if new_origin.x < self.origin.x {
            tiles_download_area = TileArea::new(TileCoordinates {
                x: old_buffered_origin.x + width as i32 - batch_size as i32,
                y: old_buffered_origin.y,
            }, batch_size, height);
            tiles_upload_area = TileArea::new(TileCoordinates {
                x: new_origin.x - buffer_size,
                y: new_origin.y - buffer_size,
            }, batch_size, height);
        } else if new_origin.y > self.origin.y {
            tiles_download_area = TileArea::new(
                old_buffered_origin,
                width,
                batch_size,
            );
            tiles_upload_area = TileArea::new(TileCoordinates {
                x: new_origin.x - buffer_size,
                y: new_origin.y - buffer_size + height as i32 - batch_size as i32,
            }, width, batch_size);
        } else if new_origin.y < self.origin.y {
            tiles_download_area = TileArea::new(TileCoordinates {
                x: old_buffered_origin.x,
                y: old_buffered_origin.y + height as i32 - batch_size as i32,
            }, width, batch_size);
            tiles_upload_area = TileArea::new(TileCoordinates {
                x: new_origin.x - buffer_size,
                y: new_origin.y - buffer_size,
            }, width, batch_size);
        } else {
            return Ok(());
        }

        // defer rapid re-entry until the prior download has reached its CPU chunk
        if self.tile_download_pending_in(tiles_upload_area)? ||
                self.fluid_download_pending_in(tiles_upload_area)? ||
                self.fluid_upload_pending_in(tiles_download_area)? { return Ok(()); }

        // materialize queued CPU state before capturing the old physical slots
        self.tile_uploads_submit()?;
        self.fluid_uploads_submit()?;

        // capture and submit old physical slots before changing their world interpretation
        self.tile_downloads_queue(tiles_download_area)?;
        self.tile_downloads_submit()?;
        self.fluid_downloads_queue(tiles_download_area);
        self.fluid_downloads_submit()?;

        // remap only the reused edge; retained tiles keep their physical kinematic slots
        if new_origin.x > self.origin.x {
            self.tiles_ring_offset_x = (self.tiles_ring_offset_x + batch_size) % width;
        } else if new_origin.x < self.origin.x {
            self.tiles_ring_offset_x =
                (self.tiles_ring_offset_x + width - batch_size) % width;
        } else if new_origin.y > self.origin.y {
            self.tiles_ring_offset_y = (self.tiles_ring_offset_y + batch_size) % height;
        } else {
            self.tiles_ring_offset_y =
                (self.tiles_ring_offset_y + height - batch_size) % height;
        }
        self.origin = new_origin;
        self.cellular_collision_dirty = true;
        self.fluids.refresh(
            self.accelerator.as_ref(),
            self.area_fluid_active().origin(),
            self.area_fluid_active().dimensions()[0],
            self.area_fluid_active().dimensions()[1],
            TileCoordinates { x: new_origin.x - buffer_size, y: new_origin.y - buffer_size },
            width,
            height,
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
        );
        let _ = self.tiles_upload(tiles_upload_area);
        self.fluid_uploads_queue(tiles_upload_area)?;
        Ok(())
    }

    /// Returns the active tile at a provided `TileCoordinates` if one exists
    pub fn tile_at(&self, coordinates: TileCoordinates) -> Option<Tile> {
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let width: usize = self.simulation_width as usize + buffer_size as usize * 2;
        let x: usize = usize::try_from(coordinates.x - (self.origin.x - buffer_size)).ok()?;
        let y: usize = usize::try_from(coordinates.y - (self.origin.y - buffer_size)).ok()?;
        let height: usize = self.simulation_height as usize + buffer_size as usize * 2;
        if x >= width || y >= height { return None; }
        let x: usize = (x + self.tiles_ring_offset_x as usize) % width;
        let y: usize = (y + self.tiles_ring_offset_y as usize) % height;
        self.tiles.get(y * width + x).copied()
    }

    /// Queues one authoritative fluid export under the current ring interpretation
    fn fluid_downloads_queue(&mut self, area: TileArea) {
        let download: Arc<Mutex<FluidDownload>> = self.fluid_download_pool.pop().unwrap_or_else(
            || Arc::new(Mutex::new(FluidDownload::new(
                self.accelerator.as_ref(), area, self.fluids.particle_capacity(),
            ))),
        );
        download.lock().unwrap().reset(area);
        self.fluid_downloads.push(download);
    }

    /// Returns whether an incoming area overlaps unresolved exported fluid
    fn fluid_download_pending_in(&self, area: TileArea) -> Result<bool, io::Error> {
        for download in &self.fluid_downloads {
            if download.lock().map_err(|_| {
                io::Error::other("Fluid download is unavailable")
            })?.area.intersects(area) { return Ok(true); }
        }
        Ok(false)
    }

    /// Returns whether an outgoing area overlaps unresolved imported fluid
    fn fluid_upload_pending_in(&self, area: TileArea) -> Result<bool, io::Error> {
        for upload in &self.fluid_uploads {
            if upload.lock().map_err(|_| {
                io::Error::other("Fluid upload is unavailable")
            })?.area.intersects(area) { return Ok(true); }
        }
        Ok(false)
    }

    /// Returns whether a chunk is pinned by an unresolved fluid ownership transfer
    fn fluid_transfer_pending_for_chunk(
        &self,
        coordinates: TileCoordinates,
    ) -> Result<bool, io::Error> {
        for download in &self.fluid_downloads {
            if download.lock().map_err(|_| {
                io::Error::other("Fluid download is unavailable")
            })?.area.chunk_area().contains(coordinates) { return Ok(true); }
        }
        for upload in &self.fluid_uploads {
            if upload.lock().map_err(|_| {
                io::Error::other("Fluid upload is unavailable")
            })?.area.chunk_area().contains(coordinates) { return Ok(true); }
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
            let Some(ChunkEntry::Active { chunk, is_dirty }) =
                self.chunks.get_mut(&coordinates)
            else { unreachable!(); };
            let mut chunk_particles: Vec<ChunkFluidParticle> =
                chunk.take_dormant_fluid_particles(area);
            if !chunk_particles.is_empty() { *is_dirty = true; }
            particles.append(&mut chunk_particles);
        }
        if particles.is_empty() { return Ok(()); }
        if particles.len() > self.fluids.particle_capacity() as usize {
            for particle in particles {
                let Some(ChunkEntry::Active { chunk, .. }) =
                    self.chunks.get_mut(&particle.tile_coordinates().chunk_coordinates())
                else { unreachable!(); };
                chunk.insert_dormant_fluid_particle(particle).unwrap();
            }
            return Err(io::Error::other(
                "Incoming dormant fluid exceeds the GPU particle pool capacity",
            ));
        }
        if !particles.iter().all(|particle| matches!(
            self.data.materials().get(particle.material_identifier),
            Some(Material::Fluid { .. }),
        )) {
            for particle in particles {
                let Some(ChunkEntry::Active { chunk, .. }) =
                    self.chunks.get_mut(&particle.tile_coordinates().chunk_coordinates())
                else { unreachable!(); };
                chunk.insert_dormant_fluid_particle(particle).unwrap();
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Dormant particle references an unregistered fluid material",
            ));
        }
        self.fluid_uploads.push(Arc::new(Mutex::new(FluidUpload::new(
            self.accelerator.as_ref(), area, particles,
        ))));
        Ok(())
    }

    /// Applies completed fluid exports to their current-position CPU chunks
    fn fluid_downloads_apply_completed(&mut self) -> Result<(), io::Error> {
        let mut index: usize = 0;
        while index < self.fluid_downloads.len() {
            let mut download = self.fluid_downloads[index].lock().map_err(|_| {
                io::Error::other("Fluid download is unavailable")
            })?;
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
                if !area.contains(coordinates) || !matches!(
                    self.chunks.get(&coordinates.chunk_coordinates()),
                    Some(ChunkEntry::Active { .. }),
                ) {
                    return Err(io::Error::other(
                        "Exported fluid particle has no active destination chunk",
                    ));
                }
            }
            let particles: Vec<ChunkFluidParticle> = download.result.take().unwrap().unwrap();
            drop(download);
            for particle in particles {
                let Some(ChunkEntry::Active { chunk, is_dirty }) =
                    self.chunks.get_mut(&particle.tile_coordinates().chunk_coordinates())
                else { unreachable!(); };
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
            let mut upload = self.fluid_uploads[index].lock().map_err(|_| {
                io::Error::other("Fluid upload is unavailable")
            })?;
            let Some(result) = upload.result.as_ref() else {
                index += 1;
                continue;
            };
            if let Err(error) = result {
                return Err(io::Error::other(format!("Fluid upload failed: {error}")));
            }
            for particle in result.as_ref().unwrap() {
                if !matches!(
                    self.chunks.get(&particle.tile_coordinates().chunk_coordinates()),
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
                let Some(ChunkEntry::Active { chunk, is_dirty }) =
                    self.chunks.get_mut(&particle.tile_coordinates().chunk_coordinates())
                else { unreachable!(); };
                chunk.insert_dormant_fluid_particle(*particle).map_err(|_| {
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

    /// Submits queued fluid exports and begins their asynchronous readbacks
    fn fluid_downloads_submit(&self) -> Result<(), io::Error> {
        for download in &self.fluid_downloads {
            let mut state = download.lock().map_err(|_| {
                io::Error::other("Fluid download is unavailable")
            })?;
            if state.is_started { continue; }
            self.fluids.export(
                self.accelerator.as_ref(), &state,
                self.area_fluid_active().origin(),
                self.area_fluid_active().dimensions()[0],
                self.area_fluid_active().dimensions()[1],
                self.area_buffered().origin(),
                self.area_buffered().dimensions()[0], self.area_buffered().dimensions()[1],
                self.tiles_ring_offset_x, self.tiles_ring_offset_y,
            );
            state.is_started = true;
            let buffer: wgpu::Buffer = state.buffer.clone();
            let mapped_buffer: wgpu::Buffer = buffer.clone();
            let download: Arc<Mutex<FluidDownload>> = download.clone();
            let particle_capacity: u32 = self.fluids.particle_capacity();
            drop(state);
            buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| {
                let bytes: Result<Vec<u8>, io::Error> = match result {
                    Ok(()) => match mapped_buffer.slice(..).get_mapped_range() {
                        Ok(mapped_data) => {
                            let count: usize = u32::from_le_bytes(
                                mapped_data[0..4].try_into().unwrap(),
                            ) as usize;
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
                    let result: Result<Vec<ChunkFluidParticle>, io::Error> = bytes.and_then(
                        |bytes| FluidDownload::deserialize(&bytes, particle_capacity),
                    );
                    download.lock().unwrap().result = Some(result);
                });
            });
        }
        Ok(())
    }

    /// Submits queued dormant-fluid reconstruction and begins result readback
    fn fluid_uploads_submit(&self) -> Result<(), io::Error> {
        for upload in &self.fluid_uploads {
            let mut state = upload.lock().map_err(|_| {
                io::Error::other("Fluid upload is unavailable")
            })?;
            if state.is_started { continue; }
            self.fluids.import(
                self.accelerator.as_ref(), &state,
                self.area_fluid_active().origin(),
                self.area_fluid_active().dimensions()[0],
                self.area_fluid_active().dimensions()[1],
                self.area_buffered().origin(),
                self.area_buffered().dimensions()[0], self.area_buffered().dimensions()[1],
                self.tiles_ring_offset_x, self.tiles_ring_offset_y,
            );
            state.is_started = true;
            let buffer: wgpu::Buffer = state.buffer.clone();
            let mapped_buffer: wgpu::Buffer = buffer.clone();
            let upload: Arc<Mutex<FluidUpload>> = upload.clone();
            drop(state);
            buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| {
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
                    let result: Result<Vec<ChunkFluidParticle>, io::Error> = bytes.and_then(
                        |bytes| upload.lock().unwrap().failed_particles(&bytes),
                    );
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
        let downloads: Vec<Arc<Mutex<TileDownload>>> = area.iterate_tile_coordinates()
            .filter_map(|coordinates| self.tile_at(coordinates).map(|tile| {
                Arc::new(Mutex::new(TileDownload::new(
                    self.accelerator.as_ref(),
                    coordinates,
                    tile,
                )))
            })).collect();
        let mut error: Option<io::Error> = None;
        if let Err(_) = self.tile_downloads.lock().map(|mut tile_downloads| {
            tile_downloads.extend(downloads.iter().cloned());
        }) { error = Some(io::Error::other("Tile download queue is unavailable")); }
        let mut downloads: Vec<Arc<Mutex<TileDownload>>> = downloads;
        let mut tile_data: HashMap<TileCoordinates, TileData> = HashMap::new();
        poll_fn(move |context| {
            if let Some(error) = error.take() { return std::task::Poll::Ready(Err(error)); }
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
        &self,
        area: TileArea,
    ) -> impl Future<Output = Result<(), io::Error>> + 'static {
        let mut error: Option<io::Error> = None;
        let mut uploads: Vec<Arc<Mutex<TileUpload>>> = Vec::new();
        for coordinates in area.iterate_tile_coordinates() {
            if self.tile_at(coordinates).is_none() { continue; }
            match self.chunks.get(&coordinates.chunk_coordinates()) {
                Some(ChunkEntry::Active { chunk, .. }) => match chunk.get_tile(coordinates) {
                    Ok(tile_data) => uploads.push(Arc::new(Mutex::new(
                        TileUpload::new(coordinates, tile_data)
                    ))),
                    Err(()) => {
                        error = Some(io::Error::new(io::ErrorKind::InvalidInput,
                                                    "Tile is not in its active chunk",
                        ));
                        break;
                    }
                },
                _ => {
                    error = Some(io::Error::new(io::ErrorKind::NotFound,
                                                "Tile chunk is not active",
                    ));
                    break;
                }
            }
        }
        if error.is_none() && let Err(_) = self.tile_uploads.lock().map(|mut tile_uploads| {
            tile_uploads.extend(uploads.iter().cloned());
        }) { error = Some(io::Error::other("Tile upload queue is unavailable")); }
        poll_fn(move |context| {
            if let Some(error) = error.take() { return std::task::Poll::Ready(Err(error)); }
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
            let tile: Tile = self.tile_at(coordinates).ok_or_else(|| {
                io::Error::other("Outgoing tile is outside the old GPU buffer")
            })?;
            downloads.push(Arc::new(Mutex::new(TileDownload::new(
                self.accelerator.as_ref(),
                coordinates,
                tile,
            ))));
        }
        // share the existing copy and deserialization path while retaining internal ownership
        self.tile_downloads.lock().map_err(|_| {
            io::Error::other("Tile download queue is unavailable")
        })?.extend(downloads.iter().cloned());
        self.outgoing_tile_downloads.extend(downloads);
        Ok(())
    }

    /// Returns whether an area contains an outgoing tile awaiting download
    fn tile_download_pending_in(&self, area: TileArea) -> Result<bool, io::Error> {
        for download in &self.outgoing_tile_downloads {
            let coordinates: TileCoordinates = download.lock().map_err(|_| {
                io::Error::other("Outgoing tile download is unavailable")
            })?.coordinates;
            if area.contains(coordinates) { return Ok(true); }
        }
        Ok(false)
    }

    /// Returns whether a chunk contains an outgoing tile awaiting download
    fn tile_download_pending_for_chunk(
        &self,
        coordinates: TileCoordinates,
    ) -> Result<bool, io::Error> {
        for download in &self.outgoing_tile_downloads {
            let tile_coordinates: TileCoordinates = download.lock().map_err(|_| {
                io::Error::other("Outgoing tile download is unavailable")
            })?.coordinates;
            if tile_coordinates.chunk_coordinates() == coordinates { return Ok(true); }
        }
        Ok(false)
    }

    /// Applies completed outgoing tile downloads to persistent CPU chunks
    fn tile_downloads_apply_completed(&mut self) -> Result<(), io::Error> {
        let mut index: usize = 0;
        while index < self.outgoing_tile_downloads.len() {
            // leave unfinished and failed jobs pinned so stale chunks cannot be saved
            let mut download: std::sync::MutexGuard<TileDownload> =
                self.outgoing_tile_downloads[index].lock().map_err(|_| {
                    io::Error::other("Outgoing tile download is unavailable")
                })?;
            let Some(result) = download.result.as_ref() else {
                index += 1;
                continue;
            };
            if let Err(error) = result {
                return Err(io::Error::other(format!("Outgoing tile download failed: {error}")));
            }
            let coordinates: TileCoordinates = download.coordinates;
            if !matches!(
                self.chunks.get(&coordinates.chunk_coordinates()),
                Some(ChunkEntry::Active { .. }),
            ) {
                return Err(io::Error::other("Outgoing tile download chunk is not active"));
            }
            let tile_data: TileData = download.result.take().unwrap().unwrap();
            drop(download);

            // replace the stale persistence copy and route saving through normal dirty handling
            let Some(ChunkEntry::Active { chunk, is_dirty }) =
                self.chunks.get_mut(&coordinates.chunk_coordinates())
            else { unreachable!(); };
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
            let downloads = self.tile_downloads.lock().map_err(|_| {
                io::Error::other("Tile download queue is unavailable")
            })?;
            // process each incomplete download
            for download in downloads.iter() {
                let mut state: std::sync::MutexGuard<TileDownload> = download.lock().unwrap();
                if state.result.is_some() { continue; }
                if state.is_started { continue; }
                let tile: Tile = state.physical_tile;
                let command_encoder: &mut wgpu::CommandEncoder = command_encoder.get_or_insert_with(
                    || self.accelerator.wgpu_device().create_command_encoder(
                        &wgpu::CommandEncoderDescriptor { label: Some("tile_downloads_submit") },
                    )
                );
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
                state.is_started = true;
                downloads_started.push((download.clone(), state.buffer.clone()));
            }
        }
        if let Some(command_encoder) = command_encoder {
            self.accelerator.wgpu_queue().submit(Some(command_encoder.finish()));
            // submit the encoded copies and register their readback callbacks
            for (download, buffer) in downloads_started {
                let mapped_buffer: wgpu::Buffer = buffer.clone();
                buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| {
                    let result: Result<TileData, io::Error> = match result {
                        Ok(()) => {
                            match mapped_buffer.slice(..).get_mapped_range() {
                                Ok(mapped_data) => {
                                    let mut material_data: &[u8] = &mapped_data[..
                                        TileData::CELL_FIELD_SERIALIZED_SIZE];
                                    let mut appearance_data: &[u8] = &mapped_data[
                                        TileData::CELL_FIELD_SERIALIZED_SIZE..
                                            TileData::CELL_FIELD_SERIALIZED_SIZE * 2];
                                    let mut integrity_data: &[u8] = &mapped_data[
                                        TileData::CELL_FIELD_SERIALIZED_SIZE * 2..];
                                    let tile_data: Result<TileData, io::Error> =
                                        TileData::deserialize_fields(
                                            &mut material_data,
                                            &mut appearance_data,
                                            &mut integrity_data,
                                        );
                                    drop(mapped_data);
                                    mapped_buffer.unmap();
                                    tile_data
                                }
                                Err(error) => {
                                    mapped_buffer.unmap();
                                    Err(io::Error::other(error.to_string()))
                                }
                            }
                        }
                        Err(_) => Err(io::Error::other("Tile download failed")),
                    };
                    let mut download: std::sync::MutexGuard<TileDownload> =
                        download.lock().unwrap();
                    download.result = Some(result);
                    download.is_complete = true;
                    if let Some(waker) = download.waker.take() { waker.wake(); }
                });
            }
        }
        Ok(())
    }

    /// Removes completed GPU tile downloads
    fn tile_downloads_clean(&self) -> Result<(), io::Error> {
        self.tile_downloads.lock().map_err(|_| {
            io::Error::other("Tile download queue is unavailable")
        })?.retain(|download| !download.lock().unwrap().is_complete);
        Ok(())
    }

    /// Submits queued GPU tile uploads
    fn tile_uploads_submit(&self) -> Result<(), io::Error> {
        // acquire the pending upload queue
        let uploads = self.tile_uploads.lock().map_err(|_| {
            io::Error::other("Tile upload queue is unavailable")
        })?;
        // process each incomplete upload
        for upload in uploads.iter() {
            let mut state: std::sync::MutexGuard<TileUpload> = upload.lock().unwrap();
            if state.result.is_some() { continue; }
            let Some(tile) = self.tile_at(state.coordinates) else {
                state.result = Some(Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Tile is outside the GPU buffer",
                )));
                state.is_complete = true;
                if let Some(waker) = state.waker.take() { waker.wake(); }
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
            if let Some(waker) = state.waker.take() { waker.wake(); }
        }
        Ok(())
    }

    /// Removes completed GPU tile uploads
    fn tile_uploads_clean(&self) -> Result<(), io::Error> {
        self.tile_uploads.lock().map_err(|_| {
            io::Error::other("Tile upload queue is unavailable")
        })?.retain(|upload| !upload.lock().unwrap().is_complete);
        Ok(())
    }

}

impl Drop for Scene {

    fn drop(&mut self) {
        self.cellular_material_identifiers.free();
        self.cellular_appearances.free();
        self.cellular_integrities.free();
    }

}