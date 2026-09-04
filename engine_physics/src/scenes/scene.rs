// Copyright Rob Gage 2026

use super::{
    SceneConfiguration,
    SceneData,
    SceneGenerator,
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
    tiles::{
        Tile,
        TileArea,
        TileCoordinates,
    },
};
use engine_compute::{
    Accelerator,
    AcceleratorBuffer
};
use std::{
    collections::HashMap,
    error::Error,
    io,
    sync::{
        Arc,
        mpsc::{
            Receiver,
            SyncSender,
            sync_channel,
        }
    },
};

/// The capacity of the chunk streaming queue
pub const CHUNK_STREAMING_QUEUE_CAPACITY: usize = 64;

/// A scene that can be simulated by the engine
pub struct Scene {
    /// The `Accelerator` this `Scene` is running on
    accelerator: Arc<Accelerator>,
    /// The persistent `SceneData` backing this `Scene`
    data: SceneData,
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
    /// The width of the active tile area
    simulation_width: u16,
    /// The height of the active tile area
    simulation_height: u16,
    /// The current active tiles in this `Scene`
    tiles: Box<[Tile]>,
    /// The next active tiles in this `Scene`
    tiles_next: Box<[Tile]>,
    /// The streaming batch size for tiles
    tile_streaming_batch_size: u8,
    /// The `TilePosition` of the tile in `tiles` that is furthest to the left and bottom
    origin: TileCoordinates,
    /// The buffer containing `MaterialIdentifier`s for cellular particles
    cell_material_identifier_buffer: AcceleratorBuffer,
}

impl Scene {

    /// Creates a new `Scene` with provided dimensions
    pub fn new(
        accelerator: &Arc<Accelerator>,
        configuration: SceneConfiguration,
    ) -> Result<Self, Box<dyn Error>> {
        let accelerator: Arc<Accelerator> = accelerator.clone();
        let data: SceneData = SceneData::open(configuration.data_path.clone())?;
        let generator: Arc<dyn SceneGenerator> = Arc::new(());
        let active_tile_count: usize =
            configuration.simulation_width as usize * configuration.simulation_height as usize;
        let cellular_particle_material_identifier_buffer: AcceleratorBuffer =
            accelerator.allocate::<u32>(active_tile_count * 64);
        let (chunk_streaming_response_sender, chunk_streaming_responses) =
            sync_channel(CHUNK_STREAMING_QUEUE_CAPACITY);
        let mut scene: Self = Self {
            accelerator,
            data,
            generator,
            actor_registry: ActorRegistry::new(),
            possessed_actor: None,
            chunks: HashMap::new(),
            chunk_streaming_response_sender,
            chunk_streaming_responses,
            chunks_streaming_identifier_next: 0,
            tiles: vec![Tile(0); active_tile_count].into_boxed_slice(),
            tiles_next: vec![Tile(0); active_tile_count].into_boxed_slice(),
            tile_streaming_batch_size: configuration.tile_streaming_batch_size,
            simulation_width: configuration.simulation_width,
            simulation_height: configuration.simulation_height,
            origin: TileCoordinates { x: 0, y: 0 },
            cell_material_identifier_buffer: cellular_particle_material_identifier_buffer,
        };
        // load or generate every chunk needed by the initial area and prefetch buffer.
        for coordinates in scene.streaming_area().iterate_chunk_coordinates() {
            let chunk: Chunk = match scene.data.read_chunk(coordinates)? {
                Some(chunk) => chunk,
                None => scene.generator.generate_chunk(coordinates),
            };
            scene.chunks.insert(coordinates, ChunkEntry::Active {
                chunk,
                is_dirty: false,
            });
        }
        Ok(scene)
    }

    /// Sets the `SceneGenerator` of this `Scene` that will be used for generating new chunks
    pub fn with_generator(mut self, generator: impl SceneGenerator + 'static) -> Self {
        self.generator = Arc::new(generator);
        self
    }

    /// Returns the `ActorRegistry` for this `Scene`
    pub const fn actor_registry(&self) -> &ActorRegistry { &self.actor_registry }

    /// Returns mutable access to the `ActorRegistry` for this `Scene`
    pub const fn actor_registry_mutable(&mut self) -> &mut ActorRegistry
    { &mut self.actor_registry }

    /// Returns the currently possessed actor if one exists
    pub fn possessed_actor(&self) -> Option<Actor> {
        self.possessed_actor.filter(|actor| self.actor_registry.contains(*actor))
    }

    /// Possesses an actor if it exists in this `Scene`
    pub fn possess_actor(&mut self, identifier: Actor) -> bool {
        if !self.actor_registry.is_possessable(identifier) { return false; }
        self.possessed_actor = Some(identifier);
        true
    }

    /// Releases the currently possessed actor
    pub fn dispossess_actor(&mut self) {
        self.possessed_actor = None;
    }

    /// Returns the exact tile area currently being simulated
    const fn active_area(&self) -> TileArea {
        TileArea::new(
            self.origin,
            self.simulation_width,
            self.simulation_height,
        )
    }

    /// Returns the chunk-aligned area currently resident for prefetching
    const fn streaming_area(&self) -> TileArea {
        TileArea::new(
            TileCoordinates {
                x: self.origin.x - Chunk::WIDTH as i32,
                y: self.origin.y - Chunk::WIDTH as i32,
            },
            self.simulation_width + Chunk::WIDTH * 2,
            self.simulation_height + Chunk::WIDTH * 2,
        ).chunk_area()
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

    /// Saves dirty chunks and removes entries outside the current prefetch region
    fn chunks_prune(&mut self) -> Result<(), io::Error> {
        // get streaming `TileArea` so all chunks not in it can be pruned
        let streaming_area: TileArea = self.streaming_area();
        let coordinates: Vec<TileCoordinates> = self.chunks.keys().copied().filter(|coordinates| {
            !streaming_area.contains(*coordinates)
        }).collect();
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

    /// Refreshes the active tile area towards the desired origin, loading and generating
    /// chunks as necessary
    fn chunks_refresh(&mut self, desired_origin: TileCoordinates) -> Result<(), io::Error> {
        // apply completed background loads and generations before planning movement
        while let Ok(response) = self.chunk_streaming_responses.try_recv() {
            match response {
                ChunkStreamingResponse::Loaded {
                    streaming_identifier,
                    coordinates,
                    result,
                } => {
                    // ignore results for entries that were evicted or replaced meanwhile
                    let matches_request = matches!(
                        self.chunks.get(&coordinates),
                        Some(ChunkEntry::Loading { streaming_identifier: current })
                            if *current == streaming_identifier
                    );
                    if !matches_request { continue; }
                    // a missing saved chunk begins a separate generation operation
                    match result {
                        Ok(Some(chunk)) => {
                            self.chunks.insert(coordinates, ChunkEntry::Active {
                                chunk: *chunk,
                                is_dirty: false,
                            });
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
                    // ignore results for entries that were evicted or replaced meanwhile.
                    if !matches!(
                        self.chunks.get(&coordinates),
                        Some(ChunkEntry::Generating { streaming_identifier: current })
                            if *current == streaming_identifier
                    ) { continue; }
                    // promote successful generation and retain the original error otherwise.
                    match result {
                        Ok(chunk) => {
                            self.chunks.insert(coordinates, ChunkEntry::Active {
                                chunk: *chunk,
                                is_dirty: false,
                            });
                        }
                        Err(error) => {
                            let error: Box<dyn Error> = error;
                            self.chunks.insert(coordinates, ChunkEntry::Error(error));
                        }
                    }
                }
            }
        }
        // ignore refreshes when streaming in batches has been disabled
        let batch_size: i64 = self.tile_streaming_batch_size as i64;
        if batch_size == 0 { return Ok(()); }
        // request the desired prefetch region before the active origin reaches it
        self.chunks_request()?;
        // move by at most one configured batch on each axis per refresh
        let x_difference: i64 = i64::from(desired_origin.x) - i64::from(self.origin.x);
        let y_difference: i64 = i64::from(desired_origin.y) - i64::from(self.origin.y);
        if x_difference >= batch_size { self.shift_right()?; }
        if x_difference <= -batch_size { self.shift_left()?; }
        if y_difference >= batch_size { self.shift_up()?; }
        if y_difference <= -batch_size { self.shift_down()?; }
        self.chunks_prune()
    }

    /// Requests every chunk in the streaming area
    fn chunks_request(&mut self) -> Result<(), io::Error> {
        let streaming_area: TileArea = self.streaming_area();
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

    /// Shifts the active tile area up by the configured streaming batch size
    fn shift_up(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        let mut origin: TileCoordinates = self.origin;
        origin.y = origin.y.checked_add(batch_size).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Scene origin overflow")
        })?;
        self.shift_to(origin)
    }

    /// Shifts the active tile area down by the configured streaming batch size
    fn shift_down(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        let mut origin: TileCoordinates = self.origin;
        origin.y = origin.y.checked_sub(batch_size).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Scene origin overflow")
        })?;
        self.shift_to(origin)
    }

    /// Shifts the active tile area right by the configured streaming batch size
    fn shift_right(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        let mut origin: TileCoordinates = self.origin;
        origin.x = origin.x.checked_add(batch_size).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Scene origin overflow")
        })?;
        self.shift_to(origin)
    }

    /// Shifts the active tile area left by the configured streaming batch size
    fn shift_left(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        let mut origin: TileCoordinates = self.origin;
        origin.x = origin.x.checked_sub(batch_size).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Scene origin overflow")
        })?;
        self.shift_to(origin)
    }

    /// Moves the active origin if all chunks required at the new origin are ready
    fn shift_to(&mut self, new_origin: TileCoordinates) -> Result<(), io::Error> {
        self.chunks_request()?;
        let new_active_area: TileArea = TileArea::new(
            new_origin,
            self.simulation_width,
            self.simulation_height,
        ).chunk_area();
        if new_active_area.iterate_chunk_coordinates().all(|coordinates| {
            matches!(
                self.chunks.get(&coordinates),
                Some(ChunkEntry::Active { .. })
            )
        }) { self.origin = new_origin; }
        Ok(())
    }

}

impl Drop for Scene {

    fn drop(&mut self) {
        self.cell_material_identifier_buffer.free()
    }

}
