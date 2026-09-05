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
        TileData,
        TileDownload,
        TileUpload,
    },
};
use engine_compute::{
    Accelerator,
    AcceleratorBuffer
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
    /// The size of the GPU tile buffer outside the active area
    simulation_buffer_size: u8,
    /// The current GPU-resident tiles in this `Scene`
    tiles: Box<[Tile]>,
    /// The next GPU-resident tiles in this `Scene`
    tiles_next: Box<[Tile]>,
    /// The streaming batch size for tiles
    tile_streaming_batch_size: u8,
    /// The `TilePosition` of the tile in `tiles` that is furthest to the left and bottom
    origin: TileCoordinates,
    /// The desired `origin` for the active tile area
    origin_target: TileCoordinates,
    /// The buffer containing `MaterialIdentifier`s for GPU-resident tiles
    tile_material_identifier_buffer: AcceleratorBuffer,
    /// The tile downloads pending processing by `tick`
    tile_downloads: Mutex<Vec<Arc<Mutex<TileDownload>>>>,
    /// The tile uploads pending processing by `tick`
    tile_uploads: Mutex<Vec<Arc<Mutex<TileUpload>>>>,
}

impl Scene {

    /// Creates a `Scene`, its buffered GPU tile storage, and every chunk initially covering it.
    ///
    /// The initial streaming area is synchronously loaded from disk or generated so the returned
    /// scene has data for its active area and its non-simulated GPU buffer. Tile uploads are
    /// queued here and submitted by the first `tick`.
    pub fn new(
        accelerator: &Arc<Accelerator>,
        configuration: SceneConfiguration,
    ) -> Result<Self, Box<dyn Error>> {
        // validate dimensions before deriving the size of the GPU tile buffer.
        configuration.validate()?;
        let accelerator: Arc<Accelerator> = accelerator.clone();
        let data: SceneData = SceneData::open(configuration.data_path.clone())?;
        let generator: Arc<dyn SceneGenerator> = Arc::new(());
        // allocate one material identifier for every cell in the buffered tile area.
        let buffer_size: u16 = u16::from(configuration.simulation_buffer_size) * 2;
        let buffered_tile_count: usize =
            (configuration.simulation_width + buffer_size) as usize *
            (configuration.simulation_height + buffer_size) as usize;
        let buffered_cell_count: usize = buffered_tile_count * 64;
        let tile_material_identifier_buffer: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_cell_count);
        let tile_count: u32 = buffered_tile_count as u32;
        let tiles: Box<[Tile]> = (0..tile_count).map(Tile).collect();
        let (chunk_streaming_response_sender, chunk_streaming_responses) =
            sync_channel(CHUNK_STREAMING_QUEUE_CAPACITY);
        // construct the scene before populating chunks so its area helpers can be reused.
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
            tiles_next: tiles.clone(),
            tiles,
            tile_streaming_batch_size: configuration.tile_streaming_batch_size,
            simulation_width: configuration.simulation_width,
            simulation_height: configuration.simulation_height,
            simulation_buffer_size: configuration.simulation_buffer_size,
            origin: TileCoordinates { x: 0, y: 0 },
            origin_target: TileCoordinates { x: 0, y: 0 },
            tile_material_identifier_buffer,
            tile_downloads: Mutex::new(Vec::new()),
            tile_uploads: Mutex::new(Vec::new()),
        };
        // load or generate every chunk needed by the initial area and GPU buffer.
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
        scene.refresh_tiles()?;
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

    /// Processes chunk streaming and pending nonblocking GPU tile transfers
    pub fn tick(&mut self) -> Result<(), io::Error> {
        self.refresh_chunks()?;
        self.tile_downloads_submit()?;
        self.tile_uploads_submit()?;
        self.accelerator.poll().map_err(|error| io::Error::other(error.to_string()))?;
        self.tile_download_clean()?;
        self.tile_upload_clean()?;
        Ok(())
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

    /// Returns the chunk-aligned area currently resident for prefetching
    fn area_streaming(&self) -> TileArea { self.area_buffered().chunk_area() }

    /// Requests every chunk in the streaming area
    fn chunks_fetch(&mut self) -> Result<(), io::Error> {
        let streaming_area: TileArea = self.area_streaming();
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

    /// Saves dirty chunks and removes entries outside the current prefetch region
    fn chunks_save(&mut self) -> Result<(), io::Error> {
        // get streaming `TileArea` so all chunks not in it can be pruned
        let streaming_area: TileArea = self.area_streaming();
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

    /// Attempts to move to `origin_target` and refreshes active chunks
    fn refresh_chunks(&mut self) -> Result<(), io::Error> {
        // apply completed background loads and generations before planning movement
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
                        }
                        Err(error) => {
                            let error: Box<dyn Error> = error;
                            self.chunks.insert(coordinates, ChunkEntry::Error(error));
                        }
                    }
                }
            }
        }
        self.chunks_fetch()?; // request prefetch region before active origin reaches it
        // move by at most one batch
        let batch_size: i64 = self.tile_streaming_batch_size as i64;
        let x_difference: i64 = self.origin_target.x as i64 - self.origin.x as i64;
        let y_difference: i64 = self.origin_target.y as i64 - self.origin.y as i64;
        if x_difference >= batch_size { self.shift_right()?; }
        if x_difference <= -batch_size { self.shift_left()?; }
        if y_difference >= batch_size { self.shift_up()?; }
        if y_difference <= -batch_size { self.shift_down()?; }
        self.chunks_save()?;
        self.refresh_tiles()
    }

    /// Attempts to move to `origin_target` and refreshes active tiles
    fn refresh_tiles(&self) -> Result<(), io::Error> {
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let width: i32 = i32::from(self.simulation_width) + buffer_size * 2;
        let height: i32 = i32::from(self.simulation_height) + buffer_size * 2;
        let origin: TileCoordinates = TileCoordinates {
            x: self.origin.x - buffer_size,
            y: self.origin.y - buffer_size,
        };
        for y in 0..height {
            for x in 0..width {
                let coordinates: TileCoordinates = TileCoordinates {
                    x: origin.x + x,
                    y: origin.y + y,
                };
                if self.tile_from_coordinates(coordinates).is_none() {
                    return Err(io::Error::other("GPU tile buffer is inconsistent"));
                }
                match self.chunks.get(&coordinates.chunk_coordinates()) {
                    Some(ChunkEntry::Active { chunk, .. }) if chunk.get_tile(coordinates).is_ok()
                        => (),
                    Some(ChunkEntry::Active { .. }) => return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Tile is not in its active chunk",
                    )),
                    _ => return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        "Tile chunk is not active",
                    )),
                }
                drop(self.tile_upload(coordinates));
            }
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

    /// Moves the active origin if all chunks required at the new origin are ready
    fn shift_to(&mut self, new_origin: TileCoordinates) -> Result<(), io::Error> {
        self.chunks_fetch()?;
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

    /// Returns the GPU tile assigned to a world tile coordinate
    pub fn tile_from_coordinates(&self, coordinates: TileCoordinates) -> Option<Tile> {
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let width: usize = self.simulation_width as usize + buffer_size as usize * 2;
        let x: usize = usize::try_from(coordinates.x - (self.origin.x - buffer_size)).ok()?;
        let y: usize = usize::try_from(coordinates.y - (self.origin.y - buffer_size)).ok()?;
        let height: usize = self.simulation_height as usize + buffer_size as usize * 2;
        if x >= width || y >= height { return None; }
        self.tiles.get(y * width + x).copied()
    }

    /// Queues a GPU tile download and returns a future resolved
    pub fn tile_download(
        &self,
        coordinates: TileCoordinates,
    ) -> impl Future<Output = Result<TileData, io::Error>> + 'static {
        // create the `TileDownload`
        let download: Arc<Mutex<TileDownload>> = Arc::new(Mutex::new(
            TileDownload::new(self.accelerator.as_ref(), coordinates)
        ));
        // return validation or queue failures
        if let Err(_) = self.tile_downloads.lock().map(|mut downloads| {
            downloads.push(download.clone());
        }) {
            let mut download: std::sync::MutexGuard<TileDownload> = download.lock().unwrap();
            download.result = Some(Err(io::Error::other("Tile download queue is unavailable")));
            download.is_complete = true;
        }
        // poll until result is ready
        poll_fn(move |context| {
            let mut download: std::sync::MutexGuard<TileDownload> = download.lock().unwrap();
            match download.result.take() {
                Some(result) => std::task::Poll::Ready(result),
                None => {
                    download.waker = Some(context.waker().clone());
                    std::task::Poll::Pending
                }
            }
        })
    }

    /// Queues a GPU tile upload and returns a future resolved
    pub fn tile_upload(
        &self,
        coordinates: TileCoordinates,
    ) -> impl Future<Output = Result<(), io::Error>> + 'static {
        // create the `TileUpload`
        let mut error: Option<io::Error> = None;
        let upload: Option<Arc<Mutex<TileUpload>>> = match self.chunks.get(
            &coordinates.chunk_coordinates()
        ) {
            Some(ChunkEntry::Active { chunk, .. }) => match chunk.get_tile(coordinates) {
                Ok(tile_data) => Some(Arc::new(Mutex::new(
                    TileUpload::new(coordinates, tile_data)
                ))),
                Err(()) => {
                    error = Some(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Tile is not in its active chunk",
                    ));
                    None
                }
            },
            _ => {
                error = Some(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Tile chunk is not active",
                ));
                None
            }
        };
        // return validation or queue failures
        if let Some(upload) = &upload {
            if let Err(_) = self.tile_uploads.lock().map(|mut uploads| {
                uploads.push(upload.clone());
            }) {
                let mut upload: std::sync::MutexGuard<TileUpload> = upload.lock().unwrap();
                upload.result = Some(Err(io::Error::other("Tile upload queue is unavailable")));
                upload.is_complete = true;
            }
        }
        // poll until the result is ready
        poll_fn(move |context| {
            if let Some(error) = error.take() { return std::task::Poll::Ready(Err(error)); }
            let upload: &Arc<Mutex<TileUpload>> = upload.as_ref().unwrap();
            let mut upload: std::sync::MutexGuard<TileUpload> = upload.lock().unwrap();
            match upload.result.take() {
                Some(result) => std::task::Poll::Ready(result),
                None => {
                    upload.waker = Some(context.waker().clone());
                    std::task::Poll::Pending
                }
            }
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
                let Some(tile) = self.tile_from_coordinates(state.coordinates) else {
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
                    self.tile_material_identifier_buffer.wgpu_buffer(),
                    tile.0 as u64 * TileData::SERIALIZED_SIZE as u64,
                    &state.buffer,
                    0,
                    TileData::SERIALIZED_SIZE as u64,
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
                                    let mut data: &[u8] = &mapped_data;
                                    let tile_data: Result<TileData, io::Error> =
                                        TileData::deserialize(&mut data);
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
            let Some(tile) = self.tile_from_coordinates(state.coordinates) else {
                state.result = Some(Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Tile is outside the GPU buffer",
                )));
                state.is_complete = true;
                if let Some(waker) = state.waker.take() { waker.wake(); }
                continue;
            };
            self.accelerator.wgpu_queue().write_buffer(
                self.tile_material_identifier_buffer.wgpu_buffer(),
                tile.0 as u64 * TileData::SERIALIZED_SIZE as u64,
                &state.data,
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

    fn drop(&mut self) { self.tile_material_identifier_buffer.free() }

}
