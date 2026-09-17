// Copyright Rob Gage 2026

use super::*;

impl Scene {
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
}
