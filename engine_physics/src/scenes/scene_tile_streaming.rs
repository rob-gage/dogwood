// Copyright Rob Gage 2026

use super::*;

impl Scene {
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

    /// Queues mandatory downloads for tiles leaving Accelerator residency
    pub(super) fn tile_downloads_queue(&mut self, area: TileArea) -> Result<(), io::Error> {
        // bind every world coordinate to its physical slot under the old ring mapping
        let mut downloads: Vec<Arc<Mutex<TileDownload>>> = Vec::new();
        for coordinates in area.iterate_tile_coordinates() {
            let tile: Tile = self.tile_at(coordinates).ok_or_else(|| {
                io::Error::other("Outgoing tile is outside the old Accelerator buffer")
            })?;
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
    pub(super) fn tile_download_pending_in(&self, area: TileArea) -> Result<bool, io::Error> {
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
    pub(super) fn tile_download_pending_for_chunk(
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
    pub(super) fn tile_downloads_apply_completed(&mut self) -> Result<(), io::Error> {
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

    /// Submits queued Accelerator tile downloads
    pub(super) fn tile_downloads_submit(&self) -> Result<(), io::Error> {
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

    /// Removes completed Accelerator tile downloads
    pub(super) fn tile_downloads_clean(&self) -> Result<(), io::Error> {
        self.tile_downloads
            .lock()
            .map_err(|_| io::Error::other("Tile download queue is unavailable"))?
            .retain(|download| !download.lock().unwrap().is_complete);
        Ok(())
    }

    /// Submits queued Accelerator tile uploads
    pub(super) fn tile_uploads_submit(&self) -> Result<(), io::Error> {
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
                    "Tile is outside the Accelerator buffer",
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
}
