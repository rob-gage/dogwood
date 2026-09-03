// Copyright Rob Gage 2026

use super::{
    SceneChunk,
    SceneChunkEntry,
    SceneConfiguration,
    SceneData,
    SceneGenerator
};
use crate::tiles::{
    Tile,
    TileCoordinates,
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
    /// The `SceneGenerator` generating new tiles for this `Scene`
    generator: Box<dyn SceneGenerator>,
    /// Chunks in this scene indexed by their `TilePosition`s
    chunks: HashMap<TileCoordinates, SceneChunkEntry>,
    /// The sender used by chunk streaming threads to return streamed chunks
    chunk_streaming_response_sender: SyncSender<(u64, Box<SceneChunk>)>,
    /// The chunks that are being streamed into the `Scene`
    chunk_streaming_responses: Receiver<(u64, Box<SceneChunk>)>,
    /// The next identifier to be used for streaming chunks
    chunks_streaming_identifier_next: u64,
    /// The current active tiles in this `Scene`
    tiles: Box<[Tile]>,
    /// The next active tiles in this `Scene`
    tiles_next: Box<[Tile]>,
    /// The streaming batch size for tiles
    tile_streaming_batch_size: u8,
    /// The `TilePosition` of the tile in `tiles` that is furthest to the left and bottom
    origin: TileCoordinates,
    /// The buffer containing `MaterialIdentifier`s for cellular particles
    cellular_particle_material_identifier_buffer: AcceleratorBuffer,
}

impl Scene {

    /// Creates a new `Scene` with provided dimensions
    pub fn new(
        accelerator: &Arc<Accelerator>,
        configuration: SceneConfiguration,
    ) -> Result<Self, Box<dyn Error>> {
        let accelerator: Arc<Accelerator> = accelerator.clone();
        let data: SceneData = SceneData::open(configuration.data_path.clone())?;
        let generator: Box<dyn SceneGenerator> = Box::new(());
        let active_tile_count: usize =
            configuration.width as usize * configuration.height as usize;
        let origin: TileCoordinates = TileCoordinates { x: 0, y: 0 };
        // expand the active area by one streaming batch in every direction.
        let streaming_batch_size: i32 = configuration.tile_streaming_batch_size as i32;
        let active_max: TileCoordinates = TileCoordinates {
            x: origin.x + configuration.width as i32 - 1,
            y: origin.y + configuration.height as i32 - 1,
        };
        let first_chunk: TileCoordinates = TileCoordinates {
            x: origin.x - streaming_batch_size,
            y: origin.y - streaming_batch_size,
        }.chunk_coordinates();
        let last_chunk: TileCoordinates = TileCoordinates {
            x: active_max.x + streaming_batch_size,
            y: active_max.y + streaming_batch_size,
        }.chunk_coordinates();
        // load or generate every chunk needed by the initial area and prefetch buffer
        let mut chunks: HashMap<TileCoordinates, SceneChunkEntry> = HashMap::new();
        for y in (first_chunk.y..=last_chunk.y).step_by(64) {
            for x in (first_chunk.x..=last_chunk.x).step_by(64) {
                let coordinates: TileCoordinates = TileCoordinates { x, y, };
                let chunk: SceneChunk = match data.load_chunk(coordinates)? {
                    Some(chunk) => chunk,
                    None => generator.generate_chunk(coordinates),
                };
                chunks.insert(coordinates, SceneChunkEntry::Active {
                    chunk,
                    is_dirty: false,
                });
            }
        }
        // create the completion channel for future streaming threads
        let cellular_particle_material_identifier_buffer: AcceleratorBuffer =
            accelerator.allocate::<u32>(active_tile_count * 64);
        let (chunk_streaming_response_sender, chunk_streaming_responses) =
            sync_channel(CHUNK_STREAMING_QUEUE_CAPACITY);
        Ok(Self {
            accelerator,
            data,
            generator,
            chunks,
            chunk_streaming_response_sender,
            chunk_streaming_responses,
            chunks_streaming_identifier_next: 0,
            tiles: vec![Tile(0); active_tile_count].into_boxed_slice(),
            tiles_next: vec![Tile(0); active_tile_count].into_boxed_slice(),
            tile_streaming_batch_size: configuration.tile_streaming_batch_size,
            origin,
            cellular_particle_material_identifier_buffer,
        })
    }

    /// Reads an unloaded chunk into this `Scene` or generates it if it does not exist
    fn chunk_read_or_generate(&mut self, chunk_coordinates: TileCoordinates) {
        todo!()
    }

    /// Writes a loaded chunk to the `Scene`'s persistent data
    fn chunk_write(&mut self, chunk_coordinates: TileCoordinates) -> Result<(), io::Error> {
        todo!()
    }

    /// Refreshes the active tile area towards the desired origin
    fn refresh(&mut self, desired_origin: TileCoordinates) -> Result<(), io::Error> {
        // ignore refreshes when streaming in batches has been disabled
        let batch_size: i64 = self.tile_streaming_batch_size as i64;
        if batch_size == 0 { return Ok(()); }
        // move by at most one configured batch on each axis per refresh
        let x_difference: i64 = i64::from(desired_origin.x) - i64::from(self.origin.x);
        let y_difference: i64 = i64::from(desired_origin.y) - i64::from(self.origin.y);
        if x_difference >= batch_size { self.shift_right()?; }
        if x_difference <= -batch_size { self.shift_left()?; }
        if y_difference >= batch_size { self.shift_up()?; }
        if y_difference <= -batch_size { self.shift_down()?; }
        Ok(())
    }

    /// Shifts the active tile area up by the configured streaming batch size
    fn shift_up(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        self.origin.y = self.origin.y.checked_add(batch_size).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Scene origin overflow")
        })?;
        Ok(())
    }

    /// Shifts the active tile area down by the configured streaming batch size
    fn shift_down(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        self.origin.y = self.origin.y.checked_sub(batch_size).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Scene origin overflow")
        })?;
        Ok(())
    }

    /// Shifts the active tile area right by the configured streaming batch size
    fn shift_right(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        self.origin.x = self.origin.x.checked_add(batch_size).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Scene origin overflow")
        })?;
        Ok(())
    }

    /// Shifts the active tile area left by the configured streaming batch size
    fn shift_left(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        self.origin.x = self.origin.x.checked_sub(batch_size).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Scene origin overflow")
        })?;
        Ok(())
    }

}

impl Drop for Scene {

    fn drop(&mut self) {
        self.cellular_particle_material_identifier_buffer.free()
    }

}
