// Copyright Rob Gage 2026

use super::*;

impl Scene {
    /// Sets the automatic active-area target around a world position
    pub(super) fn follow_position(&mut self, position: ScenePosition) {
        self.origin_target = TileCoordinates {
            x: position.tile_coordinates.x - i32::from(self.simulation_width) / 2,
            y: position.tile_coordinates.y - i32::from(self.simulation_height) / 2,
        };
    }

    /// Returns the exact tile area currently being simulated
    pub(super) const fn area_active(&self) -> TileArea {
        TileArea::new(self.origin, self.simulation_width, self.simulation_height)
    }

    /// Returns the moving-fluid area including one camera-streaming batch outside the viewport
    pub(super) fn area_fluid_active(&self) -> TileArea {
        let padding: u16 = u16::from(self.tile_streaming_batch_size);
        self.area_active()
            .expanded(padding, padding, padding, padding)
    }

    /// Returns the tile area resident on the Accelerator
    pub(super) fn area_buffered(&self) -> TileArea {
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

    /// Returns the chunk-aligned area required by the current Accelerator buffer
    pub(super) fn area_streaming(&self) -> TileArea {
        self.area_buffered().chunk_area()
    }

    /// Returns the current chunk area plus one chunk in each movement direction
    pub(super) fn area_prefetching(&self) -> TileArea {
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
    pub(super) fn chunks_fetch(&mut self, streaming_area: TileArea) -> Result<(), io::Error> {
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
    pub(super) fn chunk_load(&mut self, coordinates: TileCoordinates) -> Result<(), io::Error> {
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
    pub(super) fn chunk_generate(&mut self, coordinates: TileCoordinates) -> Result<(), io::Error> {
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
    pub(super) fn chunks_save(&mut self) -> Result<(), io::Error> {
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
    pub(super) fn chunks_refresh(&mut self) -> Result<(), io::Error> {
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
                            let mut chunk = *chunk;
                            chunk.resolve_uninitialized_temperatures(|identifier| {
                                self.initial_temperature(identifier)
                            });
                            self.chunks.insert(
                                coordinates,
                                ChunkEntry::Active {
                                    chunk,
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
                            let mut chunk = *chunk;
                            chunk.resolve_uninitialized_temperatures(|identifier| {
                                self.initial_temperature(identifier)
                            });
                            self.chunks.insert(
                                coordinates,
                                ChunkEntry::Active {
                                    chunk,
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
            self.rigid_owner_load(coordinates);
            let _ = self.tiles_upload(TileArea::new(coordinates, Chunk::WIDTH, Chunk::WIDTH));
        }
        Ok(())
    }

    /// Shifts the active tile area up by the configured streaming batch size
    pub(super) fn shift_up(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        let mut origin: TileCoordinates = self.origin;
        origin.y += batch_size;
        self.shift_to(origin)
    }

    /// Shifts the active tile area down by the configured streaming batch size
    pub(super) fn shift_down(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        let mut origin: TileCoordinates = self.origin;
        origin.y -= batch_size;
        self.shift_to(origin)
    }

    /// Shifts the active tile area right by the configured streaming batch size
    pub(super) fn shift_right(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        let mut origin: TileCoordinates = self.origin;
        origin.x += batch_size;
        self.shift_to(origin)
    }

    /// Shifts the active tile area left by the configured streaming batch size
    pub(super) fn shift_left(&mut self) -> Result<(), io::Error> {
        let batch_size: i32 = self.tile_streaming_batch_size as i32;
        let mut origin: TileCoordinates = self.origin;
        origin.x -= batch_size;
        self.shift_to(origin)
    }

    /// Moves the origin, remaps ring slots, and streams tiles
    pub(super) fn shift_to(&mut self, new_origin: TileCoordinates) -> Result<(), io::Error> {
        // verify the incoming CPU state before reserving outgoing Accelerator state
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
        self.rigid_desired_owners = streaming_area
            .expanded(Chunk::WIDTH, Chunk::WIDTH, Chunk::WIDTH, Chunk::WIDTH)
            .chunk_area()
            .iterate_chunk_coordinates()
            .collect();
        self.chunks_fetch(streaming_area)?;
        for owner in streaming_area
            .expanded(Chunk::WIDTH, Chunk::WIDTH, Chunk::WIDTH, Chunk::WIDTH)
            .chunk_area()
            .iterate_chunk_coordinates()
        {
            self.rigid_owner_load(owner);
        }
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
        if !streaming_area.iterate_chunk_coordinates().all(|owner| {
            matches!(
                self.rigid_owner_loads.get(&owner),
                Some(RigidOwnerLoad::Ready(_))
            )
        }) {
            return Ok(());
        }
        // Capture under the old terrain interpretation.  If its bounded
        // staging frontier is full, retain the current valid area for a later
        // frame rather than letting bodies outrun their support.
        if !self.rigid_dormancy_begin(buffered_area)? {
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
}
