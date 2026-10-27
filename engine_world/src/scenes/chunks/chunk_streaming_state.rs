// Copyright Rob Gage 2026

use super::{
    chunk_resident::ChunkResident,
    chunk_streaming_request::ChunkStreamingRequest,
    chunk_streaming_response::ChunkStreamingResponse,
    Chunk,
    ChunkCoordinates,
    ChunkData,
    ChunkError,
    ChunkFluidParticle,
    ChunkRegion,
    ChunkRegionIndex,
    CHUNK_SIZE,
};
use crate::{
    materials::{
        MaterialIdentifier,
        MaterialTable,
    },
    scenes::{
        cellular_tiles::{
            CellCoordinates,
            CellularTileCoordinates,
            CELLULAR_TILE_SIZE,
        },
        SceneError,
        SceneGenerator,
        SCENE_CHUNK_REGION_COUNT_MAXIMUM,
        SCENE_CHUNK_REGION_SIZE_MAXIMUM,
        SCENE_CHUNK_RESIDENT_COUNT_MAXIMUM,
        SCENE_CHUNK_SAVE_ATTEMPT_COUNT_MAXIMUM,
    },
};
use std::{
    collections::{
        BTreeMap,
        HashMap,
        HashSet,
        VecDeque,
    },
    io,
    mem,
    panic::{
        AssertUnwindSafe,
        catch_unwind,
    },
    sync::{
        mpsc::{
            Receiver,
            Sender,
            TryRecvError,
            channel,
        },
        Arc,
    },
};

/// Scene-owned chunk storage and bounded background streaming state
pub struct ChunkStreamingState {
    /// Resident chunks and their current load or save states
    chunks: HashMap<ChunkCoordinates, ChunkResident>,
    /// Persistent chunk storage used by background loads and saves
    chunk_data: ChunkData,
    /// Next identifier assigned to a chunk streaming request
    next_streaming_request_identifier: u64,
    /// Next identifier assigned to a chunk region
    next_chunk_region_identifier: u64,
    /// Submitted chunk requests whose responses have not yet been incorporated
    pending_chunk_request_count: usize,
    /// Maximum pending requests, bounded by the shared pool's worker count
    pending_chunk_request_count_maximum: usize,
    /// Rotates to share limited slots among requests
    request_schedule_index: usize,
    /// Whether the next sole slot goes to a required load; `false` gives it to a dirty save
    requested_loads_prioritized: bool,
    /// Active rectangular residency requests
    retained_chunk_regions: BTreeMap<ChunkRegionIndex, ChunkRegion>,
    /// Whether every dirty chunk, including retained ones, must be saved
    is_save_requested: bool,
    /// Save request identifiers of copies of active chunks that remain resident while saved
    copy_save_streaming_request_identifiers: HashMap<ChunkCoordinates, u64>,
    /// Particle records copied into active chunks by the most recent resident save
    fluid_particle_snapshots: HashMap<ChunkCoordinates, Vec<ChunkFluidParticle>>,
    /// Sender for nonblocking load and save responses
    response_sender: Sender<ChunkStreamingResponse>,
    /// Responses incorporated during scene advancement
    response_receiver: Receiver<ChunkStreamingResponse>,
    /// Shared Rayon pool supplied by the caller
    thread_pool: Arc<rayon::ThreadPool>,
    /// Materials whose albedos color generated cells
    material_table: Arc<MaterialTable>,
}


impl ChunkStreamingState {

    /// Creates streaming state using the supplied storage, shared Rayon pool, and materials
    pub fn new(
        chunk_data: ChunkData,
        thread_pool: Arc<rayon::ThreadPool>,
        material_table: Arc<MaterialTable>,
    ) -> Self {
        let (completion_sender, completion_receiver): (
            Sender<ChunkStreamingResponse>,
            Receiver<ChunkStreamingResponse>,
        ) = channel();
        let pending_chunk_request_count_maximum: usize = thread_pool.current_num_threads();
        Self {
            chunks: HashMap::new(),
            chunk_data,
            thread_pool,
            material_table,
            response_sender: completion_sender,
            response_receiver: completion_receiver,
            retained_chunk_regions: BTreeMap::new(),
            is_save_requested: false,
            copy_save_streaming_request_identifiers: HashMap::new(),
            fluid_particle_snapshots: HashMap::new(),
            next_streaming_request_identifier: 1,
            next_chunk_region_identifier: 1,
            pending_chunk_request_count: 0,
            pending_chunk_request_count_maximum,
            request_schedule_index: 0,
            requested_loads_prioritized: true,
        }
    }

    /// Releases one persistent residency request
    pub fn release_chunk_region_request(
        &mut self,
        chunk_region_identifier: ChunkRegionIndex,
    ) -> Result<(), SceneError> {
        if self.retained_chunk_regions.remove(&chunk_region_identifier).is_some() { Ok(()) }
        else { Err(SceneError::ChunkRegionRequestNotFound { chunk_region_identifier }) }
    }

    /// Creates a persistent rectangular residency request and returns its identifier
    pub fn retain_chunk_region(
        &mut self,
        region: ChunkRegion,
    ) -> Result<ChunkRegionIndex, SceneError> {
        if self.retained_chunk_regions.len() >= SCENE_CHUNK_REGION_COUNT_MAXIMUM {
            return Err(SceneError::ChunkRegionRequestLimitReached);
        }
        if !Self::is_chunk_region_valid(region) {
            return Err(SceneError::InvalidChunkRegion);
        }
        if !self.can_retain_chunk_region(None, region) {
            return Err(SceneError::ChunkResidentLimitReached);
        }
        let chunk_region_identifier: ChunkRegionIndex = self.next_chunk_region_identifier();
        self.retained_chunk_regions.insert(chunk_region_identifier, region);
        Ok(chunk_region_identifier)
    }

    /// Updates one existing rectangular residency request
    pub fn update_retained_chunk_region(
        &mut self,
        chunk_region_identifier: ChunkRegionIndex,
        region: ChunkRegion,
    ) -> Result<(), SceneError> {
        if !self.retained_chunk_regions.contains_key(&chunk_region_identifier) {
            return Err(SceneError::ChunkRegionRequestNotFound { chunk_region_identifier });
        }
        if !Self::is_chunk_region_valid(region) { return Err(SceneError::InvalidChunkRegion) }
        if !self.can_retain_chunk_region(Some(chunk_region_identifier), region) {
            return Err(SceneError::ChunkResidentLimitReached);
        }
        self.retained_chunk_regions.insert(chunk_region_identifier, region);
        Ok(())
    }

    /// Returns whether the coordinate has a loaded CPU chunk
    pub fn is_chunk_active(&self, coordinates: ChunkCoordinates) -> bool {
        matches!(self.chunks.get(&coordinates), Some(ChunkResident::Active { .. }))
    }

    /// Returns whether every required chunk for one request is resident and active
    pub fn is_chunk_region_active(
        &self,
        chunk_region_identifier: ChunkRegionIndex,
    ) -> bool {
        let region_request: Option<&ChunkRegion> =
            self.retained_chunk_regions.get(&chunk_region_identifier);
        let Some(region) = region_request else { return false; };
        let region_width: usize = region.width();
        let region_height: usize = region.height();
        if region_width == 0 || region_height == 0 { return false; }
        for y_offset in 0..region_height {
            for x_offset in 0..region_width {
                let coordinates: ChunkCoordinates = ChunkCoordinates {
                    x: region.bottom_left.x + x_offset as i32,
                    y: region.bottom_left.y + y_offset as i32,
                };
                if !matches!(self.chunks.get(&coordinates), Some(ChunkResident::Active { .. }))
                { return false; }
            }
        }
        true
    }

    /// Returns the most recent storage error for a coordinate
    pub fn chunk_error(&self, coordinates: ChunkCoordinates) -> Option<&SceneError> {
        match self.chunks.get(&coordinates) {
            Some(ChunkResident::Error { error, .. }) => Some(error),
            _ => None,
        }
    }

    /// Requests that every dirty chunk be saved, saving copies of chunks that remain retained
    pub fn save_chunks(&mut self) { self.is_save_requested = true; }

    /// Returns whether a requested save has not yet persisted every dirty chunk
    pub fn is_chunk_save_requested(&self) -> bool { self.is_save_requested }

    /// Returns one cellular tile's material identifiers from its active chunk
    pub fn cellular_tile_material_identifiers(
        &self,
        coordinates: CellularTileCoordinates,
    ) -> Option<[MaterialIdentifier; 64]> {
        match self.chunks.get(&coordinates.chunk_coordinates()) {
            Some(ChunkResident::Active { chunk, .. }) =>
                chunk.cellular_tile_material_identifiers(coordinates).ok(),
            _ => None,
        }
    }

    /// Returns one cellular tile's packed RGBA colors from its active chunk
    pub fn cellular_tile_colors(
        &self,
        coordinates: CellularTileCoordinates,
    ) -> Option<[u32; 64]> {
        match self.chunks.get(&coordinates.chunk_coordinates()) {
            Some(ChunkResident::Active { chunk, .. }) => chunk.cellular_tile_colors(coordinates).ok(),
            _ => None,
        }
    }

    /// Returns one cellular tile's packed kinematics from its active chunk
    pub fn cellular_tile_kinematics(
        &self,
        coordinates: CellularTileCoordinates,
    ) -> Option<[u64; 64]> {
        match self.chunks.get(&coordinates.chunk_coordinates()) {
            Some(ChunkResident::Active { chunk, .. }) =>
                chunk.cellular_tile_kinematics(coordinates).ok(),
            _ => None,
        }
    }

    /// Returns one cellular tile's gas from its active chunk
    pub fn cellular_tile_gas(
        &self,
        coordinates: CellularTileCoordinates,
    ) -> Option<Option<[u32; 512]>> {
        match self.chunks.get(&coordinates.chunk_coordinates()) {
            Some(ChunkResident::Active { chunk, .. }) => chunk.cellular_tile_gas(coordinates).ok(),
            _ => None,
        }
    }

    /// Returns the materials that color and give temperatures to generated cells
    pub fn material_table(&self) -> &MaterialTable { &self.material_table }

    /// Returns one cellular tile's matter from its active chunk
    pub fn cellular_tile_matter(
        &self,
        coordinates: CellularTileCoordinates,
    ) -> Option<Option<[u32; 128]>> {
        match self.chunks.get(&coordinates.chunk_coordinates()) {
            Some(ChunkResident::Active { chunk, .. }) =>
                chunk.cellular_tile_matter(coordinates).ok(),
            _ => None,
        }
    }

    /// Writes one cellular tile's cells into its active chunk, marking the chunk dirty on change
    pub fn set_cellular_tile_cells(
        &mut self,
        coordinates: CellularTileCoordinates,
        material_identifiers: &[MaterialIdentifier; 64],
        colors: &[u32; 64],
        kinematics: &[u64; 64],
        gas: Option<&[u32; 512]>,
        matter: Option<&[u32; 128]>,
    ) -> bool {
        let Some(ChunkResident::Active { chunk, is_dirty }) =
            self.chunks.get_mut(&coordinates.chunk_coordinates())
        else { return false; };
        if chunk.cellular_tile_material_identifiers(coordinates).ok().as_ref()
            == Some(material_identifiers)
            && chunk.cellular_tile_colors(coordinates).ok().as_ref() == Some(colors)
            && chunk.cellular_tile_kinematics(coordinates).ok().as_ref() == Some(kinematics)
            && chunk.cellular_tile_gas(coordinates).ok().as_ref() == Some(&gas.copied())
            && chunk.cellular_tile_matter(coordinates).ok().as_ref() == Some(&matter.copied())
        { return true; }
        if chunk.set_cellular_tile_material_identifiers(coordinates, material_identifiers).is_err()
            || chunk.set_cellular_tile_colors(coordinates, colors).is_err()
            || chunk.set_cellular_tile_kinematics(coordinates, kinematics).is_err()
            || chunk.set_cellular_tile_gas(coordinates, gas).is_err()
            || chunk.set_cellular_tile_matter(coordinates, matter).is_err()
        { return false; }
        *is_dirty = true;
        true
    }

    /// Removes and returns dormant particles whose cells belong to selected tiles
    pub fn take_fluid_particles_in_tiles(
        &mut self,
        coordinates: &[CellularTileCoordinates],
        maximum: usize,
    ) -> Vec<(ChunkCoordinates, ChunkFluidParticle)> {
        let mut particles: Vec<(ChunkCoordinates, ChunkFluidParticle)> = Vec::new();
        let mut chunk_tiles: BTreeMap<ChunkCoordinates, Vec<CellularTileCoordinates>> =
            BTreeMap::new();
        for tile_coordinates in coordinates {
            chunk_tiles.entry(tile_coordinates.chunk_coordinates())
                .or_default().push(*tile_coordinates);
        }
        for (chunk_coordinates, tile_coordinates) in chunk_tiles {
            if particles.len() >= maximum { break; }
            let Some(ChunkResident::Active { chunk, is_dirty }) =
                self.chunks.get_mut(&chunk_coordinates)
            else { continue; };
            // remove a prior resident save copy before treating chunk records as dormant
            let had_snapshot: bool = self.fluid_particle_snapshots
                .contains_key(&chunk_coordinates);
            if let Some(snapshot) = self.fluid_particle_snapshots.remove(&chunk_coordinates) {
                chunk.remove_fluid_particle_records(&snapshot);
            }
            if had_snapshot { *is_dirty = true; }
            let selected: Vec<ChunkFluidParticle> = chunk.take_fluid_particles_in_tiles(
                &tile_coordinates,
                maximum.saturating_sub(particles.len()),
            );
            if !selected.is_empty() { *is_dirty = true; }
            particles.extend(selected.into_iter().map(|particle| (chunk_coordinates, particle)));
        }
        particles
    }

    /// Appends exported particles to their owning active chunks in request order
    pub fn append_fluid_particles(
        &mut self,
        particles: Vec<(ChunkCoordinates, ChunkFluidParticle)>,
    ) -> Result<(), SceneError> {
        let mut grouped_particles: BTreeMap<ChunkCoordinates, Vec<ChunkFluidParticle>> =
            BTreeMap::new();
        for (coordinates, particle) in particles {
            grouped_particles.entry(coordinates).or_default().push(particle);
        }
        // validate every destination before changing any chunk
        if let Some(coordinates) = grouped_particles.keys()
            .find(|coordinates| !self.is_chunk_active(**coordinates))
        {
            return Err(SceneError::FluidParticleStreamingResponseChunkInactive {
                coordinates: *coordinates,
            });
        }
        for (coordinates, particles) in grouped_particles {
            let Some(ChunkResident::Active { chunk, is_dirty }) = self.chunks.get_mut(&coordinates)
            else { continue; };
            if let Some(snapshot) = self.fluid_particle_snapshots.remove(&coordinates) {
                chunk.remove_fluid_particle_records(&snapshot);
            }
            let mut chunk_particles: Vec<ChunkFluidParticle> = chunk.fluid_particles().to_vec();
            chunk_particles.extend(particles);
            chunk.set_fluid_particles(chunk_particles)
                .map_err(|_| SceneError::FluidParticleStreamingRecordInvalid)?;
            *is_dirty = true;
        }
        Ok(())
    }

    /// Copies resident particles into their chunks while retaining the live GPU particles
    pub fn replace_resident_fluid_particle_snapshot(
        &mut self,
        chunk_coordinates: &[ChunkCoordinates],
        particles: Vec<(ChunkCoordinates, ChunkFluidParticle)>,
    ) -> Result<(), SceneError> {
        let mut grouped_particles: BTreeMap<ChunkCoordinates, Vec<ChunkFluidParticle>> =
            BTreeMap::new();
        for (coordinates, particle) in particles {
            grouped_particles.entry(coordinates).or_default().push(particle);
        }
        // check residency before replacing any saved copies
        if let Some(coordinates) = chunk_coordinates.iter()
            .chain(grouped_particles.keys())
            .find(|coordinates| !self.is_chunk_active(**coordinates))
        {
            return Err(SceneError::FluidParticleStreamingResponseChunkInactive {
                coordinates: *coordinates,
            });
        }
        for coordinates in chunk_coordinates {
            let Some(ChunkResident::Active { chunk, is_dirty }) = self.chunks.get_mut(coordinates)
            else { continue; };
            let previous_particles: Vec<ChunkFluidParticle> = chunk.fluid_particles().to_vec();
            if let Some(snapshot) = self.fluid_particle_snapshots.remove(coordinates) {
                chunk.remove_fluid_particle_records(&snapshot);
            }
            let snapshot: Vec<ChunkFluidParticle> =
                grouped_particles.remove(coordinates).unwrap_or_default();
            let mut chunk_particles: Vec<ChunkFluidParticle> = chunk.fluid_particles().to_vec();
            chunk_particles.extend(snapshot.iter().copied());
            chunk.set_fluid_particles(chunk_particles)
                .map_err(|_| SceneError::FluidParticleStreamingRecordInvalid)?;
            if chunk.fluid_particles() != previous_particles { *is_dirty = true; }
            self.fluid_particle_snapshots.insert(*coordinates, snapshot);
        }
        if !grouped_particles.is_empty() {
            return Err(SceneError::FluidParticleStreamingRecordInvalid);
        }
        Ok(())
    }

    /// Retries a failed save or a failed load that remains inside a requested region
    pub fn retry_chunk_streaming_request(
        &mut self,
        coordinates: ChunkCoordinates,
    ) -> Result<(), SceneError> {
        // remove the error entry so its chunk can be transferred into a retry request
        let removed_entry: Option<ChunkResident> = self.chunks.remove(&coordinates);
        let Some(ChunkResident::Error { error, chunk, is_dirty, }) = removed_entry
        else { return Err(SceneError::ChunkRetryUnavailable { coordinates }); };
        match chunk {
            Some(chunk) if is_dirty => {
                if let Err(retry_error) = self.stream_chunks_request_save(coordinates, chunk) {
                    // restore the dirty chunk after the bounded scheduler rejects this retry
                    let failed_entry: Option<ChunkResident> = self.chunks.remove(&coordinates);
                    let failed_save_chunk: Option<Chunk> = match failed_entry {
                        Some(ChunkResident::Active { chunk, is_dirty: true }) => Some(chunk),
                        Some(entry) => {
                            self.chunks.insert(coordinates, entry);
                            None
                        }
                        None => None,
                    };
                    self.chunks.insert(coordinates, ChunkResident::Error {
                        error,
                        chunk: failed_save_chunk,
                        is_dirty,
                    });
                    Err(retry_error)
                } else { Ok(()) }
            }
            Some(chunk) => {
                self.chunks.insert(coordinates, ChunkResident::Active {
                    chunk,
                    is_dirty: false,
                });
                Ok(())
            }
            None => {
                if !self.is_chunk_requested(coordinates) {
                    self.chunks.insert(coordinates, ChunkResident::Error {
                        error,
                        chunk: None,
                        is_dirty,
                    });
                    return Err(SceneError::ChunkRetryUnavailable { coordinates });
                }
                if let Err(retry_error) = self.stream_chunks_request_load(coordinates) {
                    self.chunks.insert(coordinates, ChunkResident::Error {
                        error,
                        chunk: None,
                        is_dirty,
                    });
                    Err(retry_error)
                } else { Ok(()) }
            }
        }
    }

    /// Advances chunk streaming through its four scene-owned phases without waiting
    pub fn stream_chunks(
        &mut self,
        generator: &Arc<dyn SceneGenerator>,
        world_seed: u64,
    ) -> Result<(), SceneError> {
        // collect the exact coordinates requested by active regions
        let required_coordinates: HashSet<ChunkCoordinates> =
            self.stream_chunks_collect_coordinates();
        // apply completed loads and saves before deciding what remains resident
        self.stream_chunks_apply_responses(&required_coordinates)?;
        // unload clean chunks that are no longer required
        self.stream_chunks_unload(&required_coordinates);
        // submit bounded requests for required loads and dirty saves
        self.stream_chunks_schedule_requests(&required_coordinates, generator, world_seed)?;
        // save copies of retained dirty chunks while a save is requested
        if self.is_save_requested { self.stream_chunks_schedule_copy_saves(&required_coordinates); }
        Ok(())
    }

    /// Collects the exact union of coordinates covered by active region requests
    fn stream_chunks_collect_coordinates(&self) -> HashSet<ChunkCoordinates> {
        // collect the exact union of requested rectangles without filling their gaps
        let mut required_coordinates: HashSet<ChunkCoordinates> = HashSet::new();
        for region in self.retained_chunk_regions.values() {
            Self::insert_chunk_region_coordinates(
                &mut required_coordinates,
                *region,
            );
        }
        required_coordinates
    }

    /// Applies completed load and save responses, rejecting responses for stale requests
    fn stream_chunks_apply_responses(
        &mut self,
        required_coordinates: &HashSet<ChunkCoordinates>,
    ) -> Result<(), SceneError> {
        // incorporate available responses and reject responses for obsolete requests
        loop {
            let response: ChunkStreamingResponse = match self.response_receiver.try_recv() {
                Ok(response) => response,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    return Err(SceneError::ChunkStreamingResponseChannelClosed);
                }
            };
            self.pending_chunk_request_count = self.pending_chunk_request_count.saturating_sub(1);
            match response {
                // queue generation for a missing file only while its coordinate remains requested
                ChunkStreamingResponse::Missing {
                    coordinates,
                    streaming_request_identifier,
                } => {
                    let is_current: bool = matches!(
                        self.chunks.get(&coordinates),
                        Some(ChunkResident::Loading {
                            streaming_request_identifier: current_identifier,
                        }) if *current_identifier == streaming_request_identifier
                    );
                    if !is_current { continue; }
                    if required_coordinates.contains(&coordinates) {
                        self.chunks.insert(coordinates, ChunkResident::Missing);
                    } else {
                        self.chunks.remove(&coordinates);
                    }
                }
                // accept a load or generation only while its coordinate remains requested
                ChunkStreamingResponse::Loaded {
                    coordinates,
                    streaming_request_identifier,
                    result,
                } | ChunkStreamingResponse::Generated {
                    coordinates,
                    streaming_request_identifier,
                    result,
                } => {
                    let is_current: bool = matches!(
                        self.chunks.get(&coordinates),
                        Some(ChunkResident::Loading {
                            streaming_request_identifier: current_identifier,
                        } | ChunkResident::Generating {
                            streaming_request_identifier: current_identifier,
                        }) if *current_identifier == streaming_request_identifier
                    );
                    if !is_current { continue; }
                    if !required_coordinates.contains(&coordinates) {
                        self.chunks.remove(&coordinates);
                        continue;
                    }
                    match result {
                        // install the loaded or generated chunk as clean scene storage
                        Ok(chunk) => {
                            self.chunks.insert(
                                coordinates,
                                ChunkResident::Active {
                                    chunk,
                                    is_dirty: false,
                                },
                            );
                        }
                        Err(error) => {
                            self.chunks.insert(
                                coordinates,
                                ChunkResident::Error {
                                    error,
                                    chunk: None,
                                    is_dirty: false,
                                },
                            );
                        }
                    }
                }
                // finish a save without overwriting a newer request on the coordinate
                ChunkStreamingResponse::Saved {
                    coordinates,
                    streaming_request_identifier,
                    chunk,
                    result,
                } => {
                    // a failed copy save retains the newer active chunk for an explicit retry
                    if self.copy_save_streaming_request_identifiers.get(&coordinates)
                        == Some(&streaming_request_identifier)
                    {
                        self.copy_save_streaming_request_identifiers.remove(&coordinates);
                        if let Err(error) = result
                            && let Some(ChunkResident::Active { chunk, .. }) =
                                self.chunks.remove(&coordinates)
                        {
                            self.chunks.insert(
                                coordinates,
                                ChunkResident::Error {
                                    error,
                                    chunk: Some(chunk),
                                    is_dirty: true,
                                },
                            );
                        }
                        continue;
                    }
                    let is_current: bool = matches!(
                        self.chunks.get(&coordinates),
                        Some(ChunkResident::Saving {
                            streaming_request_identifier: current_identifier,
                        }) if *current_identifier == streaming_request_identifier
                    );
                    if !is_current {
                        if let Err(error) = result && !self.chunks.contains_key(&coordinates) {
                            self.chunks.insert(
                                coordinates,
                                ChunkResident::Error {
                                    error,
                                    chunk: Some(chunk),
                                    is_dirty: true,
                                },
                            );
                        }
                        continue;
                    }
                    // keep successful results that are requested again and preserve failures
                    match result {
                        Ok(()) if required_coordinates.contains(&coordinates) => {
                            self.chunks.insert(
                                coordinates,
                                ChunkResident::Active {
                                    chunk,
                                    is_dirty: false,
                                },
                            );
                        }
                        Ok(()) => { self.chunks.remove(&coordinates); }
                        Err(error) => {
                            self.chunks.insert(
                                coordinates,
                                ChunkResident::Error {
                                    error,
                                    chunk: Some(chunk),
                                    is_dirty: true,
                                },
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Schedules bounded load and save requests according to their priority
    fn stream_chunks_schedule_requests(
        &mut self,
        required_chunk_coordinates: &HashSet<ChunkCoordinates>,
        generator: &Arc<dyn SceneGenerator>,
        world_seed: u64,
    ) -> Result<(), SceneError> {
        // calculate bounded capacity and whether required loads or dirty saves need service
        let available_chunk_request_count: usize = self.pending_chunk_request_count_maximum
            .saturating_sub(self.pending_chunk_request_count);
        if available_chunk_request_count == 0 { return Ok(()); }
        // detect required loads and dirty data that must be saved before eviction
        let required_chunk_coordinates_missing: bool = required_chunk_coordinates.iter()
            .any(|coordinates| {
                matches!(self.chunks.get(coordinates), None | Some(ChunkResident::Missing))
            });
        let dirty_chunk_count: usize = self.chunks.iter()
            .filter(|(coordinates, entry)| {
                !required_chunk_coordinates.contains(coordinates)
                    && matches!(entry, ChunkResident::Active { is_dirty: true, .. })
            }).count();
        // alternate a sole slot between required loading and dirty persistence
        if available_chunk_request_count == 1
            && required_chunk_coordinates_missing
            && dirty_chunk_count > 0
        {
            if self.requested_loads_prioritized {
                self.stream_chunks_schedule_loads(
                    &required_chunk_coordinates,
                    1,
                    generator,
                    world_seed,
                )?;
            } else {
                self.stream_chunks_schedule_saves(&required_chunk_coordinates, 1)?;
            }
            self.requested_loads_prioritized = !self.requested_loads_prioritized;
            return Ok(());
        }
        // reserve a slot for saves when required loads and dirty chunks compete
        let maximum_required_chunk_load_request_count: usize = if dirty_chunk_count > 0
            && required_chunk_coordinates_missing
            && available_chunk_request_count > 1
        { available_chunk_request_count - 1 } else { available_chunk_request_count };
        self.stream_chunks_schedule_loads(
            &required_chunk_coordinates,
            maximum_required_chunk_load_request_count,
            generator,
            world_seed,
        )?;
        let remaining_chunk_request_count: usize = self.pending_chunk_request_count_maximum
            .saturating_sub(self.pending_chunk_request_count);
        let maximum_chunk_save_request_count: usize = if required_chunk_coordinates_missing {
            remaining_chunk_request_count.min(1)
        } else { remaining_chunk_request_count };
        self.stream_chunks_schedule_saves(
            &required_chunk_coordinates,
            maximum_chunk_save_request_count
        )?;
        Ok(())
    }

    /// Checks the bounded union before creating or updating a region request
    fn can_retain_chunk_region(
        &self,
        excluded_chunk_region_identifier: Option<ChunkRegionIndex>,
        region: ChunkRegion,
    ) -> bool {
        // build the proposed retained union while excluding the region being updated
        let mut resident_coordinates: HashSet<ChunkCoordinates> = HashSet::new();
        for (chunk_region_identifier, requested_region) in &self.retained_chunk_regions {
            if Some(*chunk_region_identifier) == excluded_chunk_region_identifier { continue; }
            Self::insert_chunk_region_coordinates(&mut resident_coordinates, *requested_region);
            if resident_coordinates.len() > SCENE_CHUNK_RESIDENT_COUNT_MAXIMUM { return false; }
        }
        Self::insert_chunk_region_coordinates(&mut resident_coordinates, region);
        if resident_coordinates.len() > SCENE_CHUNK_RESIDENT_COUNT_MAXIMUM { return false; }
        // dirty chunks remain owned even when every region moves away
        for (coordinates, entry) in &self.chunks {
            let is_dirty: bool = matches!(
                entry,
                ChunkResident::Active { is_dirty: true, .. }
                | ChunkResident::Error { is_dirty: true, .. }
            );
            if is_dirty {
                resident_coordinates.insert(*coordinates);
                if resident_coordinates.len() > SCENE_CHUNK_RESIDENT_COUNT_MAXIMUM { return false; }
            }
        }
        true
    }

    /// Returns whether both inclusive dimensions fit within the scene request limit
    fn is_chunk_region_valid(region: ChunkRegion) -> bool {
        let region_width: usize = region.width();
        let region_height: usize = region.height();
        region_width > 0
            && region_height > 0
            && region_width <= usize::from(SCENE_CHUNK_REGION_SIZE_MAXIMUM)
            && region_height <= usize::from(SCENE_CHUNK_REGION_SIZE_MAXIMUM)
    }

    /// Returns whether a coordinate lies inside any requested region
    fn is_chunk_requested(&self, coordinates: ChunkCoordinates) -> bool {
        self.retained_chunk_regions.values().any(|region| {
            coordinates.x >= region.bottom_left.x
                && coordinates.x <= region.top_right.x
                && coordinates.y >= region.bottom_left.y
                && coordinates.y <= region.top_right.y
        })
    }

    /// Returns the next streaming request identifier
    fn next_streaming_request_identifier(&mut self) -> u64 {
        let streaming_request_identifier: u64 = self.next_streaming_request_identifier;
        self.next_streaming_request_identifier = self.next_streaming_request_identifier
            .wrapping_add(1);
        streaming_request_identifier
    }

    /// Returns the next retained chunk region identifier
    fn next_chunk_region_identifier(&mut self) -> ChunkRegionIndex {
        let chunk_region_identifier_value: u64 = self.next_chunk_region_identifier;
        self.next_chunk_region_identifier = self.next_chunk_region_identifier.wrapping_add(1);
        ChunkRegionIndex::from(chunk_region_identifier_value)
    }

    /// Requests one missing chunk load on the caller-supplied pool
    fn stream_chunks_request_load(
        &mut self,
        coordinates: ChunkCoordinates,
    ) -> Result<(), SceneError> {
        if self.chunks.contains_key(&coordinates) {
            return Err(SceneError::ChunkStreamingRequestAlreadyActive { coordinates });
        }
        if self.pending_chunk_request_count >= self.pending_chunk_request_count_maximum {
            return Err(SceneError::ChunkStreamingCapacityReached);
        }
        // mark the coordinate before submission so later advances cannot duplicate it
        let streaming_request_identifier: u64 = self.next_streaming_request_identifier();
        let chunk_data: ChunkData = self.chunk_data.clone();
        let stream_request: ChunkStreamingRequest = ChunkStreamingRequest::Load {
            coordinates,
            streaming_request_identifier,
        };
        let completion_sender: Sender<ChunkStreamingResponse> = self.response_sender.clone();
        self.chunks.insert(coordinates, ChunkResident::Loading { streaming_request_identifier });
        self.pending_chunk_request_count += 1;
        self.thread_pool.spawn(move || {
            let completion: ChunkStreamingResponse =
                Self::stream_chunks_handle_request(stream_request, chunk_data);
            let _ = completion_sender.send(completion);
        });
        Ok(())
    }

    /// Requests generation of one chunk that has no persisted file
    fn stream_chunks_request_generation(
        &mut self,
        coordinates: ChunkCoordinates,
        generator: &Arc<dyn SceneGenerator>,
        world_seed: u64,
    ) -> Result<(), SceneError> {
        if !matches!(self.chunks.get(&coordinates), Some(ChunkResident::Missing)) {
            return Err(SceneError::ChunkStreamingRequestAlreadyActive { coordinates });
        }
        if self.pending_chunk_request_count >= self.pending_chunk_request_count_maximum {
            return Err(SceneError::ChunkStreamingCapacityReached);
        }
        let streaming_request_identifier: u64 = self.next_streaming_request_identifier();
        let chunk_data: ChunkData = self.chunk_data.clone();
        let stream_request: ChunkStreamingRequest = ChunkStreamingRequest::Generate {
            coordinates,
            streaming_request_identifier,
            generator: generator.clone(),
            world_seed,
            material_table: self.material_table.clone(),
        };
        let completion_sender: Sender<ChunkStreamingResponse> = self.response_sender.clone();
        self.chunks.insert(coordinates, ChunkResident::Generating { streaming_request_identifier });
        self.pending_chunk_request_count += 1;
        self.thread_pool.spawn(move || {
            let completion: ChunkStreamingResponse =
                Self::stream_chunks_handle_request(stream_request, chunk_data);
            let _ = completion_sender.send(completion);
        });
        Ok(())
    }

    /// Starts one dirty save by transferring chunk ownership to a Rayon job
    fn stream_chunks_request_save(
        &mut self,
        coordinates: ChunkCoordinates,
        chunk: Chunk,
    ) -> Result<(), SceneError> {
        if self.pending_chunk_request_count >= self.pending_chunk_request_count_maximum {
            self.chunks.insert(coordinates, ChunkResident::Active { chunk, is_dirty: true, });
            return Err(SceneError::ChunkStreamingCapacityReached);
        }
        // move the authoritative chunk into the job and keep a saving marker in the scene
        let streaming_request_identifier: u64 = self.next_streaming_request_identifier();
        let chunk_data: ChunkData = self.chunk_data.clone();
        let stream_request: ChunkStreamingRequest = ChunkStreamingRequest::Save {
            streaming_request_identifier,
            chunk,
        };
        let completion_sender: Sender<ChunkStreamingResponse> = self.response_sender.clone();
        self.chunks.insert(coordinates, ChunkResident::Saving { streaming_request_identifier });
        self.pending_chunk_request_count += 1;
        self.thread_pool.spawn(move || {
            let completion: ChunkStreamingResponse =
                Self::stream_chunks_handle_request(stream_request, chunk_data);
            let _ = completion_sender.send(completion);
        });
        Ok(())
    }

    /// Adds one exact rectangle to a coordinate set
    fn insert_chunk_region_coordinates(
        chunk_coordinate_set: &mut HashSet<ChunkCoordinates>,
        chunk_region: ChunkRegion,
    ) {
        let minimum_chunk_x: i64 = i64::from(chunk_region.bottom_left.x);
        let minimum_chunk_y: i64 = i64::from(chunk_region.bottom_left.y);
        let maximum_chunk_x: i64 = i64::from(chunk_region.top_right.x);
        let maximum_chunk_y: i64 = i64::from(chunk_region.top_right.y);
        // keep coordinate arithmetic wide until bounds are checked
        for chunk_y in minimum_chunk_y..=maximum_chunk_y {
            for chunk_x in minimum_chunk_x..=maximum_chunk_x {
                if chunk_x < i64::from(i32::MIN)
                    || chunk_x > i64::from(i32::MAX)
                    || chunk_y < i64::from(i32::MIN)
                    || chunk_y > i64::from(i32::MAX)
                { continue; }
                chunk_coordinate_set.insert(ChunkCoordinates {
                    x: chunk_x as i32,
                    y: chunk_y as i32
                });
                if chunk_coordinate_set.len() > SCENE_CHUNK_RESIDENT_COUNT_MAXIMUM { return; }
            }
        }
    }

    /// Schedules required loads in a rotating order across independent requests
    fn stream_chunks_schedule_loads(
        &mut self,
        required_chunk_coordinates: &HashSet<ChunkCoordinates>,
        maximum_new_chunk_load_request_count: usize,
        generator: &Arc<dyn SceneGenerator>,
        world_seed: u64,
    ) -> Result<(), SceneError> {
        // prepare one missing-coordinate queue per region so requests share slots fairly
        if maximum_new_chunk_load_request_count == 0 || self.retained_chunk_regions.is_empty() {
            return Ok(());
        }
        let chunk_region_identifiers: Vec<ChunkRegionIndex> =
            self.retained_chunk_regions.keys().copied().collect();
        // rotate the first request so independent regions receive slots fairly
        let chunk_region_request_count: usize = chunk_region_identifiers.len();
        let first_chunk_region_request_index: usize =
            self.request_schedule_index % chunk_region_request_count;
        let mut chunk_region_load_coordinate_queues: Vec<VecDeque<ChunkCoordinates>> =
            Vec::with_capacity(chunk_region_request_count);
        let mut queued_chunk_load_coordinates: HashSet<ChunkCoordinates> = HashSet::new();
        for chunk_region_identifier in &chunk_region_identifiers {
            let requested_chunk_region: Option<&ChunkRegion> =
                self.retained_chunk_regions.get(chunk_region_identifier);
            let Some(chunk_region) = requested_chunk_region
            else {
                chunk_region_load_coordinate_queues.push(VecDeque::new());
                continue;
            };
            let mut required_chunk_coordinates_for_region: Vec<ChunkCoordinates> = Vec::new();
            // collect required coordinates belonging to this request for fair round-robin schedule
            for chunk_y in i64::from(chunk_region.bottom_left.y)
                ..=i64::from(chunk_region.top_right.y) {
                    for chunk_x in i64::from(chunk_region.bottom_left.x)
                        ..=i64::from(chunk_region.top_right.x) {
                            let chunk_coordinates: ChunkCoordinates = ChunkCoordinates {
                                x: chunk_x as i32,
                                y: chunk_y as i32,
                            };
                            if required_chunk_coordinates.contains(&chunk_coordinates) {
                                required_chunk_coordinates_for_region.push(chunk_coordinates);
                            }
                        }
                }
            // deduplicate overlapping regions and omit coordinates with an existing request
            let unique_chunk_load_coordinates: VecDeque<ChunkCoordinates> =
                required_chunk_coordinates_for_region.into_iter().filter(|chunk_coordinates| {
                    matches!(self.chunks.get(chunk_coordinates), None | Some(ChunkResident::Missing))
                        && queued_chunk_load_coordinates.insert(*chunk_coordinates)
                }).collect();
            chunk_region_load_coordinate_queues.push(unique_chunk_load_coordinates);
        }
        // take turns across region queues, beginning at the next request each advance
        let mut started_chunk_load_request_count: usize = 0;
        let mut chunk_region_queues_with_loads_remaining: usize = chunk_region_request_count;
        while started_chunk_load_request_count < maximum_new_chunk_load_request_count
            && chunk_region_queues_with_loads_remaining > 0
        {
            chunk_region_queues_with_loads_remaining = 0;
            for chunk_region_request_offset in 0..chunk_region_request_count {
                let chunk_region_request_index: usize =
                    (first_chunk_region_request_index + chunk_region_request_offset)
                        % chunk_region_request_count;
                let next_chunk_load_coordinate: Option<ChunkCoordinates> =
                    chunk_region_load_coordinate_queues[chunk_region_request_index].pop_front();
                let Some(chunk_coordinates) = next_chunk_load_coordinate else { continue; };
                chunk_region_queues_with_loads_remaining += 1;
                if matches!(self.chunks.get(&chunk_coordinates), Some(ChunkResident::Missing)) {
                    self.stream_chunks_request_generation(
                        chunk_coordinates,
                        generator,
                        world_seed,
                    )?;
                } else {
                    self.stream_chunks_request_load(chunk_coordinates)?;
                }
                started_chunk_load_request_count += 1;
                if started_chunk_load_request_count >= maximum_new_chunk_load_request_count
                { break; }
            }
        }
        self.request_schedule_index = (first_chunk_region_request_index + 1)
            % chunk_region_request_count;
        Ok(())
    }

    /// Schedules dirty authoritative chunks outside every requested region
    fn stream_chunks_schedule_saves(
        &mut self,
        requested_chunk_coordinates: &HashSet<ChunkCoordinates>,
        maximum_new_chunk_save_request_count: usize,
    ) -> Result<(), SceneError> {
        // select dirty chunks that are outside every active request
        if maximum_new_chunk_save_request_count == 0 {
            return Ok(());
        }
        // sort dirty chunk coordinates so save selection stays deterministic between advances
        let mut dirty_chunk_coordinates: Vec<ChunkCoordinates> = self
            .chunks
            .iter()
            .filter_map(|(chunk_coordinates, entry)| {
                if requested_chunk_coordinates.contains(chunk_coordinates)
                    || self.copy_save_streaming_request_identifiers.contains_key(chunk_coordinates)
                    || !matches!(entry, ChunkResident::Active { is_dirty: true, .. })
                { return None; }
                Some(*chunk_coordinates)
            }).collect();
        dirty_chunk_coordinates.sort();
        // transfer each selected chunk into a bounded asynchronous save
        for chunk_coordinates in dirty_chunk_coordinates
            .into_iter()
            .take(maximum_new_chunk_save_request_count)
        {
            let removed_chunk_entry: Option<ChunkResident> = self.chunks.remove(&chunk_coordinates);
            let Some(ChunkResident::Active {
                chunk,
                is_dirty: true,
            }) = removed_chunk_entry
            else {
                continue;
            };
            self.stream_chunks_request_save(chunk_coordinates, chunk)?;
        }
        Ok(())
    }

    /// Saves copies of retained dirty chunks and completes the requested save once all are saved
    fn stream_chunks_schedule_copy_saves(
        &mut self,
        required_coordinates: &HashSet<ChunkCoordinates>,
    ) {
        // sort retained dirty chunks so copy selection stays deterministic between advances
        let mut dirty_chunk_coordinates: Vec<ChunkCoordinates> = self.chunks.iter()
            .filter_map(|(coordinates, entry)| {
                if !required_coordinates.contains(coordinates)
                    || self.copy_save_streaming_request_identifiers.contains_key(coordinates)
                    || !matches!(entry, ChunkResident::Active { is_dirty: true, .. })
                { return None; }
                Some(*coordinates)
            }).collect();
        dirty_chunk_coordinates.sort();
        // save a copy so the chunk stays active, marking it clean until tiles change it again
        for coordinates in dirty_chunk_coordinates {
            if self.pending_chunk_request_count >= self.pending_chunk_request_count_maximum {
                break;
            }
            let streaming_request_identifier: u64 = self.next_streaming_request_identifier();
            let Some(ChunkResident::Active { chunk, is_dirty }) = self.chunks.get_mut(&coordinates)
            else { continue; };
            *is_dirty = false;
            let stream_request: ChunkStreamingRequest = ChunkStreamingRequest::Save {
                streaming_request_identifier,
                chunk: chunk.clone(),
            };
            let chunk_data: ChunkData = self.chunk_data.clone();
            let completion_sender: Sender<ChunkStreamingResponse> = self.response_sender.clone();
            self.copy_save_streaming_request_identifiers
                .insert(coordinates, streaming_request_identifier);
            self.pending_chunk_request_count += 1;
            self.thread_pool.spawn(move || {
                let completion: ChunkStreamingResponse =
                    Self::stream_chunks_handle_request(stream_request, chunk_data);
                let _ = completion_sender.send(completion);
            });
        }
        // the save completes once no chunk awaits persistence
        let is_dirty_chunk_remaining: bool = self.chunks.values().any(|entry| matches!(
            entry,
            ChunkResident::Active { is_dirty: true, .. }
            | ChunkResident::Error { is_dirty: true, .. }
            | ChunkResident::Saving { .. }
        ));
        if !is_dirty_chunk_remaining && self.copy_save_streaming_request_identifiers.is_empty() {
            self.is_save_requested = false;
        }
    }

    /// Executes one chunk request on a Rayon worker and returns its response
    fn stream_chunks_handle_request(
        chunk_streaming_request: ChunkStreamingRequest,
        chunk_data: ChunkData,
    ) -> ChunkStreamingResponse {
        match chunk_streaming_request {
            // read stored chunks and report coordinates without a persisted file
            ChunkStreamingRequest::Load { coordinates, streaming_request_identifier } => {
                let result: Result<Chunk, SceneError> = match catch_unwind(
                    AssertUnwindSafe(|| chunk_data.load(coordinates)),
                ) {
                    Ok(Ok(Some(chunk))) => Ok(chunk),
                    Ok(Ok(None)) => return ChunkStreamingResponse::Missing {
                        coordinates,
                        streaming_request_identifier,
                    },
                    Ok(Err(error)) => Err(SceneError::ChunkLoad {
                        coordinates,
                        error: Arc::new(error),
                    }),
                    Err(_) => Err(SceneError::ChunkLoad {
                        coordinates,
                        error: Arc::new(ChunkError::Io(io::Error::other("Chunk load panicked"))),
                    }),
                };
                ChunkStreamingResponse::Loaded {
                    coordinates,
                    streaming_request_identifier,
                    result,
                }
            }
            // generate chunks without persisted files, treating panics as generation errors, then
            // give each generated cell its material's color once
            ChunkStreamingRequest::Generate {
                coordinates,
                streaming_request_identifier,
                generator,
                world_seed,
                material_table,
            } => {
                let result: Result<Chunk, SceneError> = match catch_unwind(AssertUnwindSafe(|| {
                    generator.generate_chunk(coordinates, world_seed)
                        .map(|chunk| Self::stream_chunks_color_cells(chunk, &material_table))
                })) {
                    Ok(Ok(chunk)) if chunk.coordinates == coordinates => Ok(chunk),
                    Ok(Ok(_)) => Err(SceneError::ChunkGeneration {
                        coordinates,
                        error: Arc::new(io::Error::other("Chunk generator returned another chunk")),
                    }),
                    Ok(Err(error)) => Err(SceneError::ChunkGeneration {
                        coordinates,
                        error: Arc::from(error),
                    }),
                    Err(_) => Err(SceneError::ChunkGeneration {
                        coordinates,
                        error: Arc::new(io::Error::other("Chunk generator panicked")),
                    }),
                };
                ChunkStreamingResponse::Generated {
                    coordinates,
                    streaming_request_identifier,
                    result,
                }
            }
            // preserve chunk ownership in the response so failed saves can be retried
            ChunkStreamingRequest::Save { streaming_request_identifier, chunk } => {
                let coordinates: ChunkCoordinates = chunk.coordinates;
                let result: Result<(), SceneError> = match catch_unwind(AssertUnwindSafe(|| {
                    chunk_data.save(&chunk)
                })) {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(error)) => Err(SceneError::ChunkSave {
                        coordinates,
                        error: Arc::new(error),
                    }),
                    Err(_) => Err(SceneError::ChunkSave {
                        coordinates,
                        error: Arc::new(ChunkError::Io(io::Error::other("Chunk save panicked"))),
                    }),
                };
                ChunkStreamingResponse::Saved {
                    coordinates,
                    streaming_request_identifier,
                    chunk,
                    result,
                }
            }
        }
    }

    /// Replaces the color of every generated cell with its material's color at the cell
    fn stream_chunks_color_cells(mut chunk: Chunk, material_table: &MaterialTable) -> Chunk {
        let cellular_tiles_per_chunk_side: i64 = CHUNK_SIZE / CELLULAR_TILE_SIZE;
        let first_tile_x: i64 = i64::from(chunk.coordinates.x) * cellular_tiles_per_chunk_side;
        let first_tile_y: i64 = i64::from(chunk.coordinates.y) * cellular_tiles_per_chunk_side;
        for tile_y in first_tile_y..first_tile_y + cellular_tiles_per_chunk_side {
            for tile_x in first_tile_x..first_tile_x + cellular_tiles_per_chunk_side {
                let coordinates: CellularTileCoordinates = CellularTileCoordinates {
                    x: tile_x,
                    y: tile_y,
                };
                let (Ok(material_identifiers), Ok(mut colors)) = (
                    chunk.cellular_tile_material_identifiers(coordinates),
                    chunk.cellular_tile_colors(coordinates),
                ) else { continue; };
                for (cell_index, (material_identifier, color)) in
                    material_identifiers.iter().zip(colors.iter_mut()).enumerate()
                {
                    if *material_identifier == MaterialIdentifier::EMPTY { continue; }
                    let cell: CellCoordinates = CellCoordinates {
                        x: tile_x * CELLULAR_TILE_SIZE + cell_index as i64 % CELLULAR_TILE_SIZE,
                        y: tile_y * CELLULAR_TILE_SIZE + cell_index as i64 / CELLULAR_TILE_SIZE,
                    };
                    *color = material_table.cellular_color(*material_identifier, cell, *color);
                }
                let _ = chunk.set_cellular_tile_colors(coordinates, &colors);
            }
        }
        chunk
    }

    /// Unloads clean chunks outside every active region request
    fn stream_chunks_unload(
        &mut self,
        required_coordinates: &HashSet<ChunkCoordinates>,
    ) {
        // remove unrequested clean chunks while keeping dirty data and active requests
        let unneeded_coordinates: Vec<ChunkCoordinates> = self
            .chunks
            .iter()
            .filter_map(|(coordinates, entry)| {
                if required_coordinates.contains(coordinates)
                    || self.copy_save_streaming_request_identifiers.contains_key(coordinates)
                    || matches!(
                        entry,
                        ChunkResident::Loading { .. }
                        | ChunkResident::Generating { .. }
                        | ChunkResident::Saving { .. }
                    )
                { return None; }
                match entry {
                    ChunkResident::Active { is_dirty: false, .. }
                    | ChunkResident::Missing
                    | ChunkResident::Error { is_dirty: false, .. } => Some(*coordinates),
                    _ => None,
                }
            }).collect();
        for coordinates in unneeded_coordinates { self.chunks.remove(&coordinates); }
    }

}

impl Drop for ChunkStreamingState {

    fn drop(&mut self) {
        // wait for active saves and copy saves so none can finish after the final saves
        let mut pending_save_identifiers: HashSet<(ChunkCoordinates, u64)> = self
            .chunks
            .iter()
            .filter_map(|(coordinates, entry)| match entry {
                ChunkResident::Saving {
                    streaming_request_identifier,
                } => Some((*coordinates, *streaming_request_identifier)),
                _ => None,
            })
            .chain(self.copy_save_streaming_request_identifiers.iter()
                .map(|(coordinates, identifier)| (*coordinates, *identifier)))
            .collect();
        while !pending_save_identifiers.is_empty() {
            let response: ChunkStreamingResponse = match self.response_receiver.recv() {
                Ok(completion) => completion,
                Err(_) => break,
            };
            if let ChunkStreamingResponse::Saved {
                coordinates,
                streaming_request_identifier,
                chunk,
                result,
            } = response {
                pending_save_identifiers.remove(&(coordinates, streaming_request_identifier));
                // a failed copy leaves its newer active chunk dirty for the final save
                if self.copy_save_streaming_request_identifiers.get(&coordinates)
                    == Some(&streaming_request_identifier)
                {
                    if result.is_err()
                        && let Some(ChunkResident::Active { is_dirty, .. }) =
                            self.chunks.get_mut(&coordinates)
                    { *is_dirty = true; }
                    continue;
                }
                // return failed chunks to scene ownership
                if let Err(error) = result {
                    self.chunks.insert(
                        coordinates,
                        ChunkResident::Error {
                            error,
                            chunk: Some(chunk),
                            is_dirty: true,
                        },
                    );
                }
            }
        }
        // persist remaining dirty chunks, retrying each before treating the failure as fatal
        let chunks: HashMap<ChunkCoordinates, ChunkResident> = mem::take(&mut self.chunks);
        let mut save_errors: Vec<SceneError> = Vec::new();
        for (coordinates, entry) in chunks {
            let dirty_chunk: Option<Chunk> = match entry {
                ChunkResident::Active { chunk, is_dirty: true, } | ChunkResident::Error {
                    chunk: Some(chunk),
                    is_dirty: true,
                    ..
                } => Some(chunk),
                _ => None,
            };
            let Some(chunk) = dirty_chunk else { continue; };
            let mut save_result: Result<(), SceneError> = Ok(());
            for _ in 0..SCENE_CHUNK_SAVE_ATTEMPT_COUNT_MAXIMUM {
                save_result =
                    match catch_unwind(AssertUnwindSafe(|| self.chunk_data.save(&chunk))) {
                        Ok(Ok(())) => Ok(()),
                        Ok(Err(error)) => Err(SceneError::ChunkSave {
                            coordinates,
                            error: Arc::new(error),
                        }),
                        Err(_) => Err(SceneError::ChunkSave {
                            coordinates,
                            error: Arc::new(ChunkError::Io(
                                io::Error::other("Chunk save panicked"),
                            )),
                        }),
                    };
                if save_result.is_ok() { break; }
            }
            if let Err(error) = save_result { save_errors.push(error); }
        }
        if !save_errors.is_empty() { panic!("Scene chunks could not be saved: {save_errors:?}"); }
    }

}
