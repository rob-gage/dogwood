// Copyright Rob Gage 2026

use super::{
    SceneData,
    SceneEditCellPlacement,
    SceneEdit,
    SceneEditBatch,
    SceneGenerator,
    ScenePosition,
    SceneVelocity,
};
use crate::simulation::{
    CellularCollision,
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
        TileDownload,
        TileUpload,
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
    /// The tile uploads pending processing by `tick`
    tile_uploads: Mutex<Vec<Arc<Mutex<TileUpload>>>>,
    /// The physical X slot containing the buffered area's leftmost tile
    tiles_ring_offset_x: u16,
    /// The physical Y slot containing the buffered area's bottommost tile
    tiles_ring_offset_y: u16,
    /// The buffer containing `MaterialIdentifier`s for GPU-resident tiles
    cellular_material_identifiers: AcceleratorBuffer,
    /// The parallel buffer containing persistent cell appearance samples
    cellular_appearances: AcceleratorBuffer,
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
        let cellular_collision: CellularCollision = CellularCollision::new(
            accelerator.as_ref(),
            &cellular_material_identifiers,
            simulation.width + buffer_size,
            simulation.height + buffer_size,
        );
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
            tile_uploads: Mutex::new(Vec::new()),
            cellular_material_identifiers,
            cellular_appearances,
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
        let mut cell_edits: HashMap<usize, (CellCoordinates, MaterialIdentifier, CellularAppearance)> =
            HashMap::new();
        for edit in edits.drain() {
            match edit {
                SceneEdit::PlaceCells { cells } => {
                    for SceneEditCellPlacement {
                        coordinates,
                        material_identifier,
                        appearance,
                    } in cells {
                        if !matches!(
                            self.data.materials().get(material_identifier),
                            Some(Material::CellularStatic { .. } | Material::CellularDynamic { .. }),
                        ) { continue; }
                        if let Some(physical_index) = self.cell_edit_index(coordinates) {
                            cell_edits.insert(physical_index, (
                                coordinates,
                                material_identifier,
                                appearance,
                            ));
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
                            ));
                        }
                    }
                }
            }
        }
        let mut cell_edits: Vec<(usize, CellCoordinates, MaterialIdentifier, CellularAppearance)> =
            cell_edits.into_iter().map(|(index, (coordinates, material_identifier, appearance))| {
                (index, coordinates, material_identifier, appearance)
            }).collect();
        cell_edits.sort_unstable_by_key(|(physical_index, ..)| *physical_index);
        for (_, coordinates, material_identifier, appearance) in &cell_edits {
            let tile_coordinates: TileCoordinates = coordinates.tile_coordinates();
            let [x, y]: [usize; 2] = coordinates.local_tile_coordinates();
            let Some(ChunkEntry::Active { chunk, is_dirty }) =
                self.chunks.get_mut(&tile_coordinates.chunk_coordinates())
            else { return Err(io::Error::other("Resident tile chunk is not active")); };
            chunk.set_cell(
                tile_coordinates,
                x,
                y,
                *material_identifier,
                *appearance,
            ).map_err(|_| io::Error::other("Resident tile is not in its active chunk"))?;
            *is_dirty = true;
        }
        if !cell_edits.is_empty() {
            self.cellular_collision_dirty = true;
            self.write_cell_edits(&cell_edits);
        }
        Ok(())
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
        self.accelerator.poll().map_err(|error| io::Error::other(error.to_string()))?;
        self.cellular_collision.collect_completed()?;
        self.tile_download_clean()?;
        self.tile_upload_clean()?;
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
        let possessed_position: Option<ScenePosition> = self.possessed_actor()
            .and_then(|actor| self.actor_registry.get_position(actor)).copied();
        if let Some(position) = possessed_position { self.follow_position(position); }
        if !self.cellular_collision_dirty { return Ok(()); }
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        if self.cellular_collision.extract(
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
        edits: &[(usize, CellCoordinates, MaterialIdentifier, CellularAppearance)],
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
            for edit in &edits[start..end] {
                material_identifiers.extend_from_slice(&edit.2.as_u32().to_le_bytes());
                appearances.extend_from_slice(&edit.3.0.to_le_bytes());
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
                Some(ChunkEntry::Generating { .. }) => { },
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
                ChunkEntry::Generating { .. } =>
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
                ChunkEntry::Active { .. } | ChunkEntry::Generating { .. } =>
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
                    ChunkEntry::Loading { .. } | ChunkEntry::Generating { .. },
                ) { None } else { Some(*coordinates) }
            }
        ).collect();
        for coordinates in coordinates {
            // save dirty active chunks before removing them from the resident map.
            if let Some(ChunkEntry::Active { chunk, is_dirty: true }) =
                self.chunks.get(&coordinates)
            {
                self.data.write_chunk(chunk)?;
            }
            self.chunks.remove(&coordinates);
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
            }
        }
        self.chunks_fetch(self.area_prefetching())?;
        // move by at most one batch
        let batch_size: i64 = self.tile_streaming_batch_size as i64;
        let x_difference: i64 = self.origin_target.x as i64 - self.origin.x as i64;
        let y_difference: i64 = self.origin_target.y as i64 - self.origin.y as i64;
        if x_difference >= batch_size { self.shift_right()?; }
        if x_difference <= -batch_size { self.shift_left()?; }
        if y_difference >= batch_size { self.shift_up()?; }
        if y_difference <= -batch_size { self.shift_down()?; }
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
        // return early if chunks are not available
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
        self.tile_downloads_submit()?; // streaming out must happen before streaming in
        // reuse the outgoing tiles' slots for incoming tiles
        let batch_size: u16 = u16::from(self.tile_streaming_batch_size);
        let tiles_upload_area: TileArea;
        if new_origin.x > self.origin.x {
            self.tiles_ring_offset_x = (self.tiles_ring_offset_x + batch_size) % width;
            tiles_upload_area = TileArea::new(TileCoordinates {
                x: new_origin.x - buffer_size + width as i32 - batch_size as i32,
                y: new_origin.y - buffer_size,
            }, batch_size, height);
        } else if new_origin.x < self.origin.x {
            self.tiles_ring_offset_x =
                (self.tiles_ring_offset_x + width - batch_size) % width;
            tiles_upload_area = TileArea::new(TileCoordinates {
                x: new_origin.x - buffer_size,
                y: new_origin.y - buffer_size,
            }, batch_size, height);
        } else if new_origin.y > self.origin.y {
            self.tiles_ring_offset_y = (self.tiles_ring_offset_y + batch_size) % height;
            tiles_upload_area = TileArea::new(TileCoordinates {
                x: new_origin.x - buffer_size,
                y: new_origin.y - buffer_size + height as i32 - batch_size as i32,
            }, width, batch_size);
        } else {
            self.tiles_ring_offset_y =
                (self.tiles_ring_offset_y + height - batch_size) % height;
            tiles_upload_area = TileArea::new(TileCoordinates {
                x: new_origin.x - buffer_size,
                y: new_origin.y - buffer_size,
            }, width, batch_size);
        }
        self.origin = new_origin;
        self.cellular_collision_dirty = true;
        let _ = self.tiles_upload(tiles_upload_area);
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

    /// Queues tile downloads from the `Accelerator`
    pub fn tiles_download(
        &self,
        area: TileArea,
    ) -> impl Future<Output = Result<HashMap<TileCoordinates, TileData>, io::Error>> + 'static {
        let downloads: Vec<Arc<Mutex<TileDownload>>> = area.iterate_tile_coordinates()
            .filter(|coordinates| self.tile_at(*coordinates).is_some())
            .map(|coordinates| Arc::new(Mutex::new(TileDownload::new(
                self.accelerator.as_ref(),
                coordinates,
            )))).collect();
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
                let Some(tile) = self.tile_at(state.coordinates) else {
                    state.result = Some(Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Tile is outside the GPU buffer",
                    )));
                    state.is_complete = true;
                    if let Some(waker) = state.waker.take() { waker.wake(); }
                    continue;
                };
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
                                        TileData::CELL_FIELD_SERIALIZED_SIZE..];
                                    let tile_data: Result<TileData, io::Error> =
                                        TileData::deserialize_fields(&mut material_data, &mut appearance_data);
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
            state.result = Some(Ok(()));
            state.is_complete = true;
            if let Some(waker) = state.waker.take() { waker.wake(); }
        }
        Ok(())
    }

    /// Removes completed GPU tile downloads
    fn tile_download_clean(&self) -> Result<(), io::Error> {
        self.tile_downloads.lock().map_err(|_| {
            io::Error::other("Tile download queue is unavailable")
        })?.retain(|download| !download.lock().unwrap().is_complete);
        Ok(())
    }

    /// Removes completed GPU tile uploads
    fn tile_upload_clean(&self) -> Result<(), io::Error> {
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
    }

}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::{
        materials::{
            Material,
            MaterialForm,
        },
        simulation::SceneSimulationConfiguration,
    };

    #[test]
    fn applies_resident_cellular_edits() {
        let accelerator: Arc<Accelerator> = Arc::new(Accelerator::new().unwrap());
        let mut materials: MaterialRegistry = MaterialRegistry::new();
        let static_material: MaterialIdentifier = materials.register(Material::CellularStatic {
            name: "Static".into(),
            graphics: engine_graphics::MaterialAppearance::from_color(
                engine_graphics::Color::new_rgb(1, 2, 3),
            ).with_variation([0.25, 0.0, 0.0, 0.0]),
        });
        let dynamic_material: MaterialIdentifier = materials.register(Material::CellularDynamic {
            name: "Dynamic".into(),
            graphics: engine_graphics::MaterialAppearance::from_color(
                engine_graphics::Color::new_rgb(4, 5, 6),
            ),
        });
        let fluid_material: MaterialIdentifier = materials.register(Material::Fluid {
            name: "Fluid".into(),
            graphics: engine_graphics::MaterialAppearance::from_color(
                engine_graphics::Color::new_rgb(7, 8, 9),
            ),
        });
        let mut scene: Scene = Scene::new(
            &accelerator,
            materials,
            SceneSimulationConfiguration {
                gravity: [0.0, 0.0],
                width: 2,
                height: 2,
                buffer_size: 1,
                streaming_batch_size: 1,
            },
        ).unwrap();
        let mut edits: SceneEditBatch = SceneEditBatch::new();
        edits.place_material(
            static_material,
            CellularAppearance::from_seed(
                CellCoordinates { x: 8, y: 0 }.appearance_seed(),
                [0.25, 0.0, 0.0, 0.0],
            ),
            vec![
                CellCoordinates { x: 7, y: 0 },
                CellCoordinates { x: 8, y: 0 },
                CellCoordinates { x: -1, y: -1 },
            ],
        );
        edits.place_material(
            fluid_material,
            CellularAppearance::NEUTRAL,
            vec![CellCoordinates { x: 0, y: 0 }],
        );
        edits.place_material(
            MaterialIdentifier::from_u32(u32::MAX),
            CellularAppearance::NEUTRAL,
            vec![CellCoordinates { x: 0, y: 0 }],
        );
        edits.place_material(
            static_material,
            CellularAppearance::NEUTRAL,
            vec![CellCoordinates { x: 100, y: 100 }],
        );
        edits.place_cells(vec![
            SceneEditCellPlacement {
                coordinates: CellCoordinates { x: 1, y: 1 },
                material_identifier: static_material,
                appearance: CellularAppearance::from_channels([0.25, 0.0, 0.0, 0.0]),
            },
            SceneEditCellPlacement {
                coordinates: CellCoordinates { x: 2, y: 1 },
                material_identifier: dynamic_material,
                appearance: CellularAppearance::from_channels([-0.25, 0.0, 0.0, 0.0]),
            },
        ]);
        edits.erase(vec![CellCoordinates { x: 7, y: 0 }]);
        scene.apply_edits(&mut edits).unwrap();

        let negative_tile: TileCoordinates = TileCoordinates { x: -1, y: -1 };
        let positive_tile: TileCoordinates = TileCoordinates { x: 1, y: 0 };
        let chunk: &Chunk = match scene.chunks.get(&TileCoordinates { x: 0, y: 0 }).unwrap() {
            ChunkEntry::Active { chunk, is_dirty } => {
                assert!(*is_dirty);
                chunk
            }
            _ => panic!("initial chunk must be active"),
        };
        assert!(chunk.get_tile(TileCoordinates { x: 0, y: 0 }).unwrap()
            .cell_material_identifier(7, 0).as_u32() == MaterialIdentifier::NULL.as_u32());
        assert!(chunk.get_tile(TileCoordinates { x: 0, y: 0 }).unwrap()
            .cell_appearance(7, 0).0 == CellularAppearance::NEUTRAL.0);
        assert!(chunk.get_tile(positive_tile).unwrap()
            .cell_material_identifier(0, 0).form() == MaterialForm::CellularStatic);
        assert!(chunk.get_tile(positive_tile).unwrap().cell_appearance(0, 0).0 ==
            CellularAppearance::from_seed(
                CellCoordinates { x: 8, y: 0 }.appearance_seed(),
                [0.25, 0.0, 0.0, 0.0],
            ).0);
        assert!(chunk.get_tile(TileCoordinates { x: 0, y: 0 }).unwrap()
            .cell_material_identifier(0, 0).as_u32() == MaterialIdentifier::NULL.as_u32());
        assert!(chunk.get_tile(TileCoordinates { x: 0, y: 0 }).unwrap()
            .cell_material_identifier(1, 1).form() == MaterialForm::CellularStatic);
        assert!(chunk.get_tile(TileCoordinates { x: 0, y: 0 }).unwrap()
            .cell_appearance(1, 1).channels()[0] > 0.0);
        assert!(chunk.get_tile(TileCoordinates { x: 0, y: 0 }).unwrap()
            .cell_material_identifier(2, 1).form() == MaterialForm::CellularDynamic);
        assert!(chunk.get_tile(TileCoordinates { x: 0, y: 0 }).unwrap()
            .cell_appearance(2, 1).channels()[0] < 0.0);
        let negative_chunk: &Chunk = match scene.chunks.get(&negative_tile.chunk_coordinates()).unwrap() {
            ChunkEntry::Active { chunk, is_dirty } => {
                assert!(*is_dirty);
                chunk
            }
            _ => panic!("negative chunk must be active"),
        };
        assert!(negative_chunk.get_tile(negative_tile).unwrap()
            .cell_material_identifier(7, 7).form() == MaterialForm::CellularStatic);
        assert!(scene.cellular_collision_dirty);

        scene.shift_right().unwrap();
        scene.shift_right().unwrap();
        scene.shift_right().unwrap();
        let wrapped_cell: CellCoordinates = CellCoordinates { x: 47, y: 0 };
        let mut wrapped_edits: SceneEditBatch = SceneEditBatch::new();
        wrapped_edits.place_material(
            dynamic_material,
            CellularAppearance::NEUTRAL,
            vec![wrapped_cell],
        );
        scene.apply_edits(&mut wrapped_edits).unwrap();
        let wrapped_tile: TileCoordinates = wrapped_cell.tile_coordinates();
        let physical_tile: Tile = scene.tile_at(wrapped_tile).unwrap();
        assert!(physical_tile.0 == 6);
        let chunk: &Chunk = match scene.chunks.get(&wrapped_tile.chunk_coordinates()).unwrap() {
            ChunkEntry::Active { chunk, is_dirty } => {
                assert!(*is_dirty);
                chunk
            }
            _ => panic!("wrapped chunk must be active"),
        };
        assert!(chunk.get_tile(wrapped_tile).unwrap()
            .cell_material_identifier(7, 0).form() == MaterialForm::CellularDynamic);

        let buffer: wgpu::Buffer = accelerator.wgpu_device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene edit test readback"),
            size: 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder: wgpu::CommandEncoder = accelerator.wgpu_device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("scene edit test copy") },
        );
        encoder.copy_buffer_to_buffer(
            scene.cellular_material_identifiers.wgpu_buffer(),
            u64::from(physical_tile.0) * 64 * 4 + 7 * 4,
            &buffer,
            0,
            4,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| sender.send(result).unwrap());
        let result: Result<(), wgpu::BufferAsyncError> = loop {
            accelerator.poll().unwrap();
            if let Ok(result) = receiver.try_recv() { break result; }
            std::thread::yield_now();
        };
        result.unwrap();
        let mapped = buffer.slice(..).get_mapped_range().unwrap();
        let identifier: u32 = u32::from_le_bytes(mapped[..4].try_into().unwrap());
        drop(mapped);
        buffer.unmap();
        assert!(identifier == dynamic_material.as_u32());

        scene.tick(false).unwrap();
        assert!(!scene.cellular_collision_dirty);
    }

}
