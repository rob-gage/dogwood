// Copyright Rob Gage 2026

use super::{
    cellular_tile_streaming_request::CellularTileStreamingRequest,
    cellular_tile_streaming_response::CellularTileStreamingResponse,
    CellularTileBuffers,
    CellularTileCoordinates,
    CellularTileIndex,
    CellularTileRegion,
    CELLULAR_TILE_AWAKE,
    CELLULAR_TILE_SIZE,
};
use crate::{
    materials::MaterialIdentifier,
    scenes::{
        chunks::{
            ChunkCoordinates,
            ChunkRegion,
            ChunkRegionIndex,
            ChunkStreamingState,
        },
        SceneError,
        SCENE_CELLULAR_TILE_STREAMING_REQUEST_COUNT_MAXIMUM,
    },
};
use engine_host::acceleration::Accelerator;
use std::{
    collections::BTreeMap,
    slice::ChunksExact,
    sync::mpsc::{
        Receiver,
        Sender,
        channel,
    },
};

/// Number of cells in one cellular tile
const CELLULAR_TILE_CELL_COUNT: usize = (CELLULAR_TILE_SIZE * CELLULAR_TILE_SIZE) as usize;

/// Bytes occupied by one cellular tile's cells in the material identifier and color buffers
const CELLULAR_TILE_BYTE_COUNT: u64 = CELLULAR_TILE_CELL_COUNT as u64 * 4;

/// Bytes occupied by one cellular tile's cells in the kinematics buffer
const CELLULAR_TILE_KINEMATICS_BYTE_COUNT: u64 = CELLULAR_TILE_CELL_COUNT as u64 * 8;

/// Bytes of one tile in a streaming request: its material identifiers, colors, and kinematics
const CELLULAR_TILE_STREAMING_BYTE_COUNT: u64 =
    CELLULAR_TILE_BYTE_COUNT * 2 + CELLULAR_TILE_KINEMATICS_BYTE_COUNT;

/// GPU-resident cellular tiles stored in a two-dimensional ring around the active region
pub struct CellularTileStreamingState {
    /// Active region whose tiles are resident, or `None` before the first request
    active_region: Option<CellularTileRegion>,
    /// Active region requested by the caller
    active_region_requested: CellularTileRegion,
    /// Physical ring column holding the active region's leftmost tiles
    ring_offset_x: u32,
    /// Physical ring row holding the active region's bottom tiles
    ring_offset_y: u32,
    /// Retained chunk region covering every chunk overlapped by the resident active region
    chunk_region_index: Option<ChunkRegionIndex>,
    /// Retained chunk region covering every chunk overlapped by the requested region
    requested_chunk_region_index: Option<ChunkRegionIndex>,
    /// Outgoing tile streaming requests awaiting application to their chunks in request order
    streaming_requests: BTreeMap<u64, CellularTileStreamingRequest>,
    /// Next identifier assigned to a streaming request
    next_streaming_request_identifier: u64,
    /// Whether every active tile must be returned to its chunk before the chunks are saved
    is_save_requested: bool,
    /// Streaming request returning every active tile for a requested save
    save_streaming_request_identifier: Option<u64>,
    /// Sender for staging buffer mapping responses
    response_sender: Sender<CellularTileStreamingResponse>,
    /// Mapping responses incorporated during tile streaming
    response_receiver: Receiver<CellularTileStreamingResponse>,
}

impl CellularTileStreamingState {

    /// Creates the GPU tile ring for an active region and retains its chunks
    pub fn new(
        cellular_tile_buffers: &CellularTileBuffers,
        chunk_streaming_state: &mut ChunkStreamingState,
        region: CellularTileRegion,
    ) -> Result<Self, SceneError> {
        if region.width() == 0 || region.height() == 0 {
            return Err(SceneError::InvalidCellularTileRegion);
        }
        // the ring holds exactly the active region
        let region_byte_count: u64 = (region.width() as u64)
            .saturating_mul(region.height() as u64)
            .saturating_mul(CELLULAR_TILE_BYTE_COUNT);
        if region_byte_count > cellular_tile_buffers.material_identifier_buffer().size() {
            return Err(SceneError::InvalidCellularTileRegion);
        }
        let (response_sender, response_receiver): (
            Sender<CellularTileStreamingResponse>,
            Receiver<CellularTileStreamingResponse>,
        ) = channel();
        let mut cellular_tile_streaming: Self = Self {
            active_region_requested: region,
            active_region: None,
            ring_offset_x: 0,
            ring_offset_y: 0,
            chunk_region_index: None,
            requested_chunk_region_index: None,
            streaming_requests: BTreeMap::new(),
            next_streaming_request_identifier: 1,
            is_save_requested: false,
            save_streaming_request_identifier: None,
            response_sender,
            response_receiver,
        };
        cellular_tile_streaming.requested_chunk_region_index =
            Some(chunk_streaming_state.retain_chunk_region(region.chunk_region())?);
        Ok(cellular_tile_streaming)
    }

    /// Requests a new active region with the same dimensions as the GPU tile ring
    pub fn set_cellular_tile_region(
        &mut self,
        region: CellularTileRegion,
    ) -> Result<(), SceneError> {
        if region.width() != self.active_region_requested.width()
            || region.height() != self.active_region_requested.height()
        { return Err(SceneError::InvalidCellularTileRegion); }
        self.active_region_requested = region;
        Ok(())
    }

    /// Returns whether a cellular tile is resident inside the active region
    pub fn is_cellular_tile_active(&self, coordinates: CellularTileCoordinates) -> bool {
        self.active_region.is_some_and(|region| region.contains(coordinates))
    }

    /// Returns whether the requested region is resident and active
    pub fn is_cellular_tile_region_active(&self) -> bool {
        self.active_region == Some(self.active_region_requested)
    }

    /// Requests that every active tile be returned to its chunk and every dirty chunk be saved
    pub fn save_cellular_tiles(&mut self) { self.is_save_requested = true; }

    /// Returns whether a requested save has not yet returned every active tile to its chunk
    pub fn is_cellular_tile_save_requested(&self) -> bool {
        self.is_save_requested || self.save_streaming_request_identifier.is_some()
    }

    /// Returns the resident active region, or `None` before the first request is resident
    pub fn active_cellular_tile_region(&self) -> Option<CellularTileRegion> { self.active_region }

    /// Returns the physical ring column and row holding the active region's bottom-left tile
    pub fn cellular_tile_ring_offset(&self) -> (u32, u32)
    { (self.ring_offset_x, self.ring_offset_y) }

    /// Returns the cellular tile buffer index of a resident active cellular tile
    pub fn cellular_tile_index(
        &self,
        coordinates: CellularTileCoordinates,
    ) -> Option<CellularTileIndex> {
        let active_region: CellularTileRegion = self.active_region?;
        if !active_region.contains(coordinates) { return None; }
        let logical_x: u32 = (coordinates.x - active_region.bottom_left.x) as u32;
        let logical_y: u32 = (coordinates.y - active_region.bottom_left.y) as u32;
        let width: u32 = active_region.width() as u32;
        let height: u32 = active_region.height() as u32;
        let physical_x: u32 = (logical_x + self.ring_offset_x) % width;
        let physical_y: u32 = (logical_y + self.ring_offset_y) % height;
        Some(CellularTileIndex::from(physical_y * width + physical_x))
    }

    /// Returns every active tile to its chunk, waiting for every response
    pub fn return_cellular_tiles(
        &mut self,
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        chunk_streaming_state: &mut ChunkStreamingState,
    ) -> Result<(), SceneError> {
        // copy every resident tile so the chunks match the authoritative GPU state
        if let Some(active_region) = self.active_region {
            let coordinates: Vec<CellularTileCoordinates> =
                active_region.cellular_tile_coordinates().collect();
            self.stream_cellular_tiles_request_return(
                accelerator,
                cellular_tile_buffers,
                coordinates,
                None,
            );
        }
        // wait until every response has been applied to its chunk
        while !self.streaming_requests.is_empty() {
            accelerator.device().poll(wgpu::PollType::wait_indefinitely())
                .map_err(|_| SceneError::CellularTileStreamingPoll)?;
            self.stream_cellular_tiles_apply_responses(chunk_streaming_state)?;
            let inactive_chunk: Option<ChunkCoordinates> = self.streaming_requests.values()
                .filter(|streaming_request| streaming_request.is_response_ready)
                .flat_map(|streaming_request| streaming_request.coordinates.iter())
                .map(|coordinates| coordinates.chunk_coordinates())
                .find(|coordinates| !chunk_streaming_state.is_chunk_active(*coordinates));
            if let Some(coordinates) = inactive_chunk {
                return Err(SceneError::CellularTileStreamingResponseChunkInactive { coordinates });
            }
        }
        Ok(())
    }

    /// Advances tile streaming after chunk streaming without waiting for the GPU
    pub fn stream_cellular_tiles(
        &mut self,
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        chunk_streaming_state: &mut ChunkStreamingState,
    ) -> Result<(), SceneError> {
        // incorporate mapped responses and return their chunks to normal streaming
        accelerator.device().poll(wgpu::PollType::Poll)
            .map_err(|_| SceneError::CellularTileStreamingPoll)?;
        let response_result: Result<(), SceneError> =
            self.stream_cellular_tiles_apply_responses(chunk_streaming_state);
        // save the chunks once every tile returned for a requested save has reached them
        if let Some(streaming_request_identifier) = self.save_streaming_request_identifier
            && !self.streaming_requests.contains_key(&streaming_request_identifier)
        {
            self.save_streaming_request_identifier = None;
            chunk_streaming_state.save_chunks();
        }
        // return every active tile for a requested save while keeping its chunks retained
        if self.is_save_requested {
            self.is_save_requested = false;
            match self.active_region {
                Some(active_region) => {
                    self.save_streaming_request_identifier =
                        Some(self.stream_cellular_tiles_request_return(
                            accelerator,
                            cellular_tile_buffers,
                            active_region.cellular_tile_coordinates().collect(),
                            None,
                        ));
                }
                None => chunk_streaming_state.save_chunks(),
            }
        }
        // move the ring once the requested region's chunks are active
        self.stream_cellular_tiles_shift(
            accelerator,
            cellular_tile_buffers,
            chunk_streaming_state,
        )?;
        response_result
    }

    /// Applies mapped responses to their chunks and remaps failed staging buffers
    fn stream_cellular_tiles_apply_responses(
        &mut self,
        chunk_streaming_state: &mut ChunkStreamingState,
    ) -> Result<(), SceneError> {
        // record mapping results, keeping failed requests for another mapping attempt
        let mut first_error: Option<SceneError> = None;
        while let Ok(response) = self.response_receiver.try_recv() {
            let Some(streaming_request) =
                self.streaming_requests.get_mut(&response.streaming_request_identifier)
            else { continue; };
            streaming_request.is_response_requested = false;
            match response.result {
                Ok(()) => streaming_request.is_response_ready = true,
                Err(error) => {
                    first_error.get_or_insert(SceneError::CellularTileStreamingResponse { error });
                }
            }
        }
        // apply mapped responses in request order so an older copy never overwrites a newer one,
        // each only when all of its destination chunks are active
        let mut applied_streaming_request_identifiers: Vec<u64> = Vec::new();
        for (streaming_request_identifier, streaming_request) in &self.streaming_requests {
            if !streaming_request.is_response_ready
                || !streaming_request.coordinates.iter().all(|coordinates| {
                    chunk_streaming_state.is_chunk_active(coordinates.chunk_coordinates())
                })
            { break; }
            {
                let mapped_bytes: wgpu::BufferView = streaming_request.buffer
                    .get_mapped_range(..)
                    .expect("mapped cellular tile streaming buffer should be readable");
                // each staged tile holds its material identifiers, colors, then kinematics
                let mapped_tile_bytes: ChunksExact<u8> = mapped_bytes
                    .chunks_exact(CELLULAR_TILE_STREAMING_BYTE_COUNT as usize);
                for (coordinates, tile_bytes) in
                    streaming_request.coordinates.iter().zip(mapped_tile_bytes)
                {
                    let mut material_identifiers: [MaterialIdentifier; CELLULAR_TILE_CELL_COUNT] =
                        [MaterialIdentifier::EMPTY; CELLULAR_TILE_CELL_COUNT];
                    let mut colors: [u32; CELLULAR_TILE_CELL_COUNT] = [0; CELLULAR_TILE_CELL_COUNT];
                    let mut kinematics: [u64; CELLULAR_TILE_CELL_COUNT] =
                        [0; CELLULAR_TILE_CELL_COUNT];
                    let (material_identifier_bytes, tile_bytes): (&[u8], &[u8]) =
                        tile_bytes.split_at(CELLULAR_TILE_BYTE_COUNT as usize);
                    let (color_bytes, kinematics_bytes): (&[u8], &[u8]) =
                        tile_bytes.split_at(CELLULAR_TILE_BYTE_COUNT as usize);
                    for (material_identifier, cell_bytes) in material_identifiers.iter_mut()
                        .zip(material_identifier_bytes.chunks_exact(4))
                    {
                        *material_identifier = MaterialIdentifier::from_u32(u32::from_le_bytes([
                            cell_bytes[0],
                            cell_bytes[1],
                            cell_bytes[2],
                            cell_bytes[3],
                        ]));
                    }
                    for (color, cell_bytes) in colors.iter_mut().zip(color_bytes.chunks_exact(4)) {
                        *color = u32::from_le_bytes([
                            cell_bytes[0],
                            cell_bytes[1],
                            cell_bytes[2],
                            cell_bytes[3],
                        ]);
                    }
                    for (cell_kinematics, cell_bytes) in
                        kinematics.iter_mut().zip(kinematics_bytes.chunks_exact(8))
                    {
                        *cell_kinematics = u64::from_le_bytes([
                            cell_bytes[0],
                            cell_bytes[1],
                            cell_bytes[2],
                            cell_bytes[3],
                            cell_bytes[4],
                            cell_bytes[5],
                            cell_bytes[6],
                            cell_bytes[7],
                        ]);
                    }
                    chunk_streaming_state.set_cellular_tile_cells(
                        *coordinates,
                        &material_identifiers,
                        &colors,
                        &kinematics,
                    );
                }
            }
            streaming_request.buffer.unmap();
            applied_streaming_request_identifiers.push(*streaming_request_identifier);
        }
        // release chunks that no longer receive outgoing GPU data
        for streaming_request_identifier in applied_streaming_request_identifiers {
            let Some(streaming_request) =
                self.streaming_requests.remove(&streaming_request_identifier)
            else { continue; };
            if let Some(chunk_region_index) = streaming_request.chunk_region_index {
                chunk_streaming_state.release_chunk_region_request(chunk_region_index)?;
            }
        }
        // request another mapping for staging buffers whose previous mapping failed
        for (streaming_request_identifier, streaming_request) in &mut self.streaming_requests {
            if streaming_request.is_response_requested || streaming_request.is_response_ready
            { continue; }
            Self::map_cellular_tile_streaming_request(
                *streaming_request_identifier,
                streaming_request,
                &self.response_sender,
            );
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Moves the ring to the requested region, requesting departing tiles and writing entering ones
    fn stream_cellular_tiles_shift(
        &mut self,
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        chunk_streaming_state: &mut ChunkStreamingState,
    ) -> Result<(), SceneError> {
        // release a pending incoming retention when the request returns to the resident region
        if self.active_region == Some(self.active_region_requested) {
            if let Some(chunk_region_index) = self.requested_chunk_region_index.take() {
                chunk_streaming_state.release_chunk_region_request(chunk_region_index)?;
            }
            return Ok(());
        }
        // retain the incoming chunks without retaining any chunks between regions
        let requested_region: CellularTileRegion = self.active_region_requested;
        let requested_chunk_region: ChunkRegion = requested_region.chunk_region();
        match self.requested_chunk_region_index {
            Some(chunk_region_index) => chunk_streaming_state
                .update_retained_chunk_region(chunk_region_index, requested_chunk_region)?,
            None => self.requested_chunk_region_index =
                Some(chunk_streaming_state.retain_chunk_region(requested_chunk_region)?),
        }
        // defer movement while a streaming request slot is unavailable
        if self.active_region.is_some()
            && self.streaming_requests.len() >= SCENE_CELLULAR_TILE_STREAMING_REQUEST_COUNT_MAXIMUM
        { return Ok(()); }
        // re-entering tiles must wait until their earlier response reaches the chunk
        let is_streaming_request_pending_in_requested_region: bool = self.streaming_requests
            .values()
            .flat_map(|streaming_request| streaming_request.coordinates.iter())
            .any(|coordinates| requested_region.contains(*coordinates));
        if is_streaming_request_pending_in_requested_region { return Ok(()); }
        // read every entering tile first so movement waits for all of their chunks to be active
        let resident_region: Option<CellularTileRegion> = self.active_region;
        let incoming_coordinates: Vec<CellularTileCoordinates> = requested_region
            .cellular_tile_coordinates()
            .filter(|coordinates| {
                !resident_region.is_some_and(|resident_region| {
                    resident_region.contains(*coordinates)
                })
            })
            .collect();
        let mut incoming_cells: Vec<(
            [MaterialIdentifier; CELLULAR_TILE_CELL_COUNT],
            [u32; CELLULAR_TILE_CELL_COUNT],
            [u64; CELLULAR_TILE_CELL_COUNT],
        )> = Vec::with_capacity(incoming_coordinates.len());
        for coordinates in &incoming_coordinates {
            let (Some(material_identifiers), Some(colors), Some(kinematics)) = (
                chunk_streaming_state.cellular_tile_material_identifiers(*coordinates),
                chunk_streaming_state.cellular_tile_colors(*coordinates),
                chunk_streaming_state.cellular_tile_kinematics(*coordinates),
            ) else { return Ok(()); };
            incoming_cells.push((material_identifiers, colors, kinematics));
        }
        // capture departing tiles under the old ring mapping before their slots are reused
        if let Some(resident_region) = resident_region {
            let outgoing_coordinates: Vec<CellularTileCoordinates> = resident_region
                .cellular_tile_coordinates()
                .filter(|coordinates| !requested_region.contains(*coordinates))
                .collect();
            let chunk_region_index: Option<ChunkRegionIndex> =
                self.chunk_region_index.take();
            if outgoing_coordinates.is_empty() {
                if let Some(chunk_region_index) = chunk_region_index {
                    chunk_streaming_state.release_chunk_region_request(chunk_region_index)?;
                }
            } else {
                self.stream_cellular_tiles_request_return(
                    accelerator,
                    cellular_tile_buffers,
                    outgoing_coordinates,
                    chunk_region_index,
                );
            }
            // shift the ring so overlapping tiles keep their physical slots
            let width: i64 = requested_region.width() as i64;
            let height: i64 = requested_region.height() as i64;
            let origin_change_x: i64 =
                requested_region.bottom_left.x - resident_region.bottom_left.x;
            let origin_change_y: i64 =
                requested_region.bottom_left.y - resident_region.bottom_left.y;
            self.ring_offset_x =
                (i64::from(self.ring_offset_x) + origin_change_x).rem_euclid(width) as u32;
            self.ring_offset_y =
                (i64::from(self.ring_offset_y) + origin_change_y).rem_euclid(height) as u32;
        }
        self.active_region = Some(self.active_region_requested);
        self.chunk_region_index = self.requested_chunk_region_index.take();
        // queued writes execute after the submitted request copies of the same slots, waking each
        // entering tile so its moving cells continue and its neighbors notice it
        for (coordinates, (material_identifiers, colors, kinematics)) in
            incoming_coordinates.into_iter().zip(incoming_cells)
        {
            let Some(cellular_tile_index) = self.cellular_tile_index(coordinates)
            else { continue; };
            let mut material_identifier_bytes: [u8; CELLULAR_TILE_BYTE_COUNT as usize] =
                [0; CELLULAR_TILE_BYTE_COUNT as usize];
            let mut color_bytes: [u8; CELLULAR_TILE_BYTE_COUNT as usize] =
                [0; CELLULAR_TILE_BYTE_COUNT as usize];
            let mut kinematics_bytes: [u8; CELLULAR_TILE_KINEMATICS_BYTE_COUNT as usize] =
                [0; CELLULAR_TILE_KINEMATICS_BYTE_COUNT as usize];
            for (cell_index, ((material_identifier, color), cell_kinematics)) in
                material_identifiers.iter().zip(colors.iter()).zip(kinematics.iter()).enumerate()
            {
                material_identifier_bytes[cell_index * 4..cell_index * 4 + 4]
                    .copy_from_slice(&material_identifier.as_u32().to_le_bytes());
                color_bytes[cell_index * 4..cell_index * 4 + 4]
                    .copy_from_slice(&color.to_le_bytes());
                kinematics_bytes[cell_index * 8..cell_index * 8 + 8]
                    .copy_from_slice(&cell_kinematics.to_le_bytes());
            }
            let physical_tile_index: u32 = cellular_tile_index.into();
            let tile_offset: u64 =
                u64::from(physical_tile_index) * CELLULAR_TILE_BYTE_COUNT;
            accelerator.queue().write_buffer(
                cellular_tile_buffers.material_identifier_buffer(),
                tile_offset,
                &material_identifier_bytes,
            );
            accelerator.queue().write_buffer(
                cellular_tile_buffers.color_buffer(),
                tile_offset,
                &color_bytes,
            );
            accelerator.queue().write_buffer(
                cellular_tile_buffers.kinematics_buffer(),
                tile_offset * 2,
                &kinematics_bytes,
            );
            accelerator.queue().write_buffer(
                cellular_tile_buffers.awake_buffer(),
                u64::from(physical_tile_index) * 4,
                &CELLULAR_TILE_AWAKE.to_le_bytes(),
            );
        }
        Ok(())
    }

    /// Copies tiles into a new staging buffer, requests their response, and returns its identifier
    fn stream_cellular_tiles_request_return(
        &mut self,
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        coordinates: Vec<CellularTileCoordinates>,
        chunk_region_index: Option<ChunkRegionIndex>,
    ) -> u64 {
        let buffer: wgpu::Buffer = accelerator.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("Scene cellular tile streaming request"),
            size: coordinates.len() as u64 * CELLULAR_TILE_STREAMING_BYTE_COUNT,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // copy each tile's material identifiers, colors, then kinematics from its physical slot
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene cellular tile streaming request"),
            });
        for (staging_index, tile_coordinates) in coordinates.iter().enumerate() {
            let Some(cellular_tile_index) = self.cellular_tile_index(*tile_coordinates)
            else { continue; };
            let physical_tile_index: u32 = cellular_tile_index.into();
            let tile_offset: u64 = u64::from(physical_tile_index) * CELLULAR_TILE_BYTE_COUNT;
            let staging_offset: u64 = staging_index as u64 * CELLULAR_TILE_STREAMING_BYTE_COUNT;
            command_encoder.copy_buffer_to_buffer(
                cellular_tile_buffers.material_identifier_buffer(),
                tile_offset,
                &buffer,
                staging_offset,
                CELLULAR_TILE_BYTE_COUNT,
            );
            command_encoder.copy_buffer_to_buffer(
                cellular_tile_buffers.color_buffer(),
                tile_offset,
                &buffer,
                staging_offset + CELLULAR_TILE_BYTE_COUNT,
                CELLULAR_TILE_BYTE_COUNT,
            );
            command_encoder.copy_buffer_to_buffer(
                cellular_tile_buffers.kinematics_buffer(),
                tile_offset * 2,
                &buffer,
                staging_offset + CELLULAR_TILE_BYTE_COUNT * 2,
                CELLULAR_TILE_KINEMATICS_BYTE_COUNT,
            );
        }
        accelerator.queue().submit(Some(command_encoder.finish()));
        let streaming_request_identifier: u64 = self.next_streaming_request_identifier;
        self.next_streaming_request_identifier = self.next_streaming_request_identifier
            .wrapping_add(1);
        let mut streaming_request: CellularTileStreamingRequest =
            CellularTileStreamingRequest {
                coordinates,
                buffer,
                chunk_region_index,
                is_response_requested: false,
                is_response_ready: false,
            };
        Self::map_cellular_tile_streaming_request(
            streaming_request_identifier,
            &mut streaming_request,
            &self.response_sender,
        );
        self.streaming_requests.insert(streaming_request_identifier, streaming_request);
        streaming_request_identifier
    }

    /// Asks WGPU to send a response once the staging buffer is readable by the CPU
    fn map_cellular_tile_streaming_request(
        streaming_request_identifier: u64,
        streaming_request: &mut CellularTileStreamingRequest,
        response_sender: &Sender<CellularTileStreamingResponse>,
    ) {
        let response_sender: Sender<CellularTileStreamingResponse> = response_sender.clone();
        streaming_request.is_response_requested = true;
        streaming_request.buffer.map_async(
            wgpu::MapMode::Read,
            ..,
            move |result: Result<(), wgpu::BufferAsyncError>| {
                let _ = response_sender.send(CellularTileStreamingResponse {
                    streaming_request_identifier,
                    result,
                });
            },
        );
    }

}
