// Copyright Rob Gage 2026

use super::*;

impl Scene {
    pub(super) fn fluid_downloads_queue(&mut self, area: TileArea) {
        let download: Arc<Mutex<FluidDownload>> =
            self.fluid_download_pool.pop().unwrap_or_else(|| {
                Arc::new(Mutex::new(FluidDownload::new(
                    self.accelerator.as_ref(),
                    area,
                    self.fluids.particle_capacity(),
                )))
            });
        download.lock().unwrap().reset(area);
        self.fluid_downloads.push(download);
    }

    /// Returns whether an incoming area overlaps unresolved exported fluid
    pub(super) fn fluid_download_pending_in(&self, area: TileArea) -> Result<bool, io::Error> {
        for download in &self.fluid_downloads {
            if download
                .lock()
                .map_err(|_| io::Error::other("Fluid download is unavailable"))?
                .area
                .intersects(area)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns whether an outgoing area overlaps unresolved imported fluid
    pub(super) fn fluid_upload_pending_in(&self, area: TileArea) -> Result<bool, io::Error> {
        for upload in &self.fluid_uploads {
            if upload
                .lock()
                .map_err(|_| io::Error::other("Fluid upload is unavailable"))?
                .area
                .intersects(area)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns whether a chunk is pinned by an unresolved fluid ownership transfer
    pub(super) fn fluid_transfer_pending_for_chunk(
        &self,
        coordinates: TileCoordinates,
    ) -> Result<bool, io::Error> {
        for download in &self.fluid_downloads {
            if download
                .lock()
                .map_err(|_| io::Error::other("Fluid download is unavailable"))?
                .area
                .chunk_area()
                .contains(coordinates)
            {
                return Ok(true);
            }
        }
        for upload in &self.fluid_uploads {
            if upload
                .lock()
                .map_err(|_| io::Error::other("Fluid upload is unavailable"))?
                .area
                .chunk_area()
                .contains(coordinates)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Removes incoming dormant records from chunks into a pending Accelerator transfer
    pub(super) fn fluid_uploads_queue(&mut self, area: TileArea) -> Result<(), io::Error> {
        let chunk_coordinates: Vec<TileCoordinates> =
            area.chunk_area().iterate_chunk_coordinates().collect();
        for coordinates in &chunk_coordinates {
            let Some(ChunkEntry::Active { .. }) = self.chunks.get(coordinates) else {
                return Err(io::Error::other("Incoming fluid chunk is not active"));
            };
        }
        let mut particles: Vec<ChunkFluidParticle> = Vec::new();
        for coordinates in chunk_coordinates {
            let Some(ChunkEntry::Active { chunk, is_dirty }) = self.chunks.get_mut(&coordinates)
            else {
                unreachable!();
            };
            let mut chunk_particles: Vec<ChunkFluidParticle> =
                chunk.take_dormant_fluid_particles(area);
            if !chunk_particles.is_empty() {
                *is_dirty = true;
            }
            particles.append(&mut chunk_particles);
        }
        if particles.is_empty() {
            return Ok(());
        }
        if particles.len() > self.fluids.particle_capacity() as usize {
            for particle in particles {
                let Some(ChunkEntry::Active { chunk, .. }) = self
                    .chunks
                    .get_mut(&particle.tile_coordinates().chunk_coordinates())
                else {
                    unreachable!();
                };
                chunk.insert_dormant_fluid_particle(particle).unwrap();
            }
            return Err(io::Error::other(
                "Incoming dormant fluid exceeds the Accelerator particle pool capacity",
            ));
        }
        for particle in &mut particles {
            if !particle.temperature.is_finite() {
                particle.temperature = self.initial_temperature(particle.material_identifier);
            }
        }
        if !particles.iter().all(|particle| {
            matches!(
                self.data.materials().get(particle.material_identifier),
                Some(Material::Fluid { .. }),
            )
        }) {
            for particle in particles {
                let Some(ChunkEntry::Active { chunk, .. }) = self
                    .chunks
                    .get_mut(&particle.tile_coordinates().chunk_coordinates())
                else {
                    unreachable!();
                };
                chunk.insert_dormant_fluid_particle(particle).unwrap();
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Dormant particle references an unregistered fluid material",
            ));
        }
        self.fluid_uploads
            .push(Arc::new(Mutex::new(FluidUpload::new(
                self.accelerator.as_ref(),
                area,
                particles,
            ))));
        Ok(())
    }

    /// Applies completed fluid exports to their current-position CPU chunks
    pub(super) fn fluid_downloads_apply_completed(&mut self) -> Result<(), io::Error> {
        let mut index: usize = 0;
        while index < self.fluid_downloads.len() {
            let mut download = self.fluid_downloads[index]
                .lock()
                .map_err(|_| io::Error::other("Fluid download is unavailable"))?;
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
                if !area.contains(coordinates)
                    || !matches!(
                        self.chunks.get(&coordinates.chunk_coordinates()),
                        Some(ChunkEntry::Active { .. }),
                    )
                {
                    return Err(io::Error::other(
                        "Exported fluid particle has no active destination chunk",
                    ));
                }
            }
            let particles: Vec<ChunkFluidParticle> = download.result.take().unwrap().unwrap();
            drop(download);
            for particle in particles {
                let Some(ChunkEntry::Active { chunk, is_dirty }) = self
                    .chunks
                    .get_mut(&particle.tile_coordinates().chunk_coordinates())
                else {
                    unreachable!();
                };
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
    pub(super) fn fluid_uploads_apply_completed(&mut self) -> Result<(), io::Error> {
        let mut index: usize = 0;
        while index < self.fluid_uploads.len() {
            let mut upload = self.fluid_uploads[index]
                .lock()
                .map_err(|_| io::Error::other("Fluid upload is unavailable"))?;
            let Some(result) = upload.result.as_ref() else {
                index += 1;
                continue;
            };
            if let Err(error) = result {
                return Err(io::Error::other(format!("Fluid upload failed: {error}")));
            }
            for particle in result.as_ref().unwrap() {
                if !matches!(
                    self.chunks
                        .get(&particle.tile_coordinates().chunk_coordinates()),
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
                let Some(ChunkEntry::Active { chunk, is_dirty }) = self
                    .chunks
                    .get_mut(&particle.tile_coordinates().chunk_coordinates())
                else {
                    unreachable!();
                };
                chunk
                    .insert_dormant_fluid_particle(*particle)
                    .map_err(|_| {
                        io::Error::other("Rejected fluid particle is outside its source chunk")
                    })?;
                *is_dirty = true;
            }
            self.fluid_uploads.swap_remove(index);
            if !failed.is_empty() {
                return Err(io::Error::other(format!(
                    "Accelerator fluid pool rejected {} dormant particles",
                    failed.len(),
                )));
            }
        }
        Ok(())
    }

    /// Applies a completed sample only to the actor for which it was dispatched
    pub(super) fn fluid_sample_apply_completed(&mut self) -> Result<(), io::Error> {
        let Some(result) = self
            .fluid_sample_result
            .lock()
            .map_err(|_| io::Error::other("Pawn fluid sample result is unavailable"))?
            .take()
        else {
            return Ok(());
        };
        let actor: Actor = self
            .fluid_sample_actor
            .take()
            .ok_or_else(|| io::Error::other("Completed pawn fluid sample has no actor"))?;
        let sample: [f32; 5] = result.map_err(io::Error::other)?;
        if self.possessed_actor() == Some(actor) {
            self.actor_registry.apply_swimming_sample(actor, sample);
        }
        Ok(())
    }

    /// Dispatches one tiny derived-cell sample without waiting for its readback
    pub(super) fn fluid_sample_submit(&mut self) -> Result<(), io::Error> {
        if self.fluid_sample_actor.is_some() {
            return Ok(());
        }
        let Some(actor) = self.possessed_actor() else {
            return Ok(());
        };
        let Some((center, shape)) = self.actor_registry.swimming_pawn_sample(actor) else {
            return Ok(());
        };
        let active_area: TileArea = self.area_fluid_active();
        let active_dimensions: [u16; 2] = active_area.dimensions();
        let buffered_area: TileArea = self.area_buffered();
        let buffered_dimensions: [u16; 2] = buffered_area.dimensions();
        self.fluids.sample_pawn(
            self.accelerator.as_ref(),
            &self.fluid_sample_buffer,
            center,
            shape,
            active_area.origin(),
            active_dimensions[0],
            active_dimensions[1],
            buffered_area.origin(),
            buffered_dimensions[0],
            buffered_dimensions[1],
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
            self.gravity,
        );
        self.fluid_sample_actor = Some(actor);
        let mapped_buffer: wgpu::Buffer = self.fluid_sample_buffer.clone();
        let result: Arc<Mutex<Option<Result<[f32; 5], String>>>> = self.fluid_sample_result.clone();
        self.fluid_sample_buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |mapping| {
                let sample: Result<[f32; 5], String> = mapping
                    .map_err(|_| "Pawn fluid sample readback failed".to_owned())
                    .and_then(|()| {
                        mapped_buffer
                            .slice(..)
                            .get_mapped_range()
                            .map_err(|error| error.to_string())
                            .and_then(|mapped| {
                                let sample: [f32; 5] = std::array::from_fn(|index| {
                                    f32::from_bits(u32::from_le_bytes(
                                        mapped[index * 4..index * 4 + 4].try_into().unwrap(),
                                    ))
                                });
                                drop(mapped);
                                if sample.into_iter().all(f32::is_finite) {
                                    Ok(sample)
                                } else {
                                    Err("Pawn fluid sample contains a non-finite value".to_owned())
                                }
                            })
                    });
                mapped_buffer.unmap();
                if let Ok(mut result) = result.lock() {
                    *result = Some(sample);
                }
            });
        Ok(())
    }

    /// Submits queued fluid exports and begins their asynchronous readbacks
    pub(super) fn fluid_downloads_submit(&self) -> Result<(), io::Error> {
        for download in &self.fluid_downloads {
            let mut state: std::sync::MutexGuard<FluidDownload> = download
                .lock()
                .map_err(|_| io::Error::other("Fluid download is unavailable"))?;
            if state.is_started {
                continue;
            }
            self.fluids.export(
                self.accelerator.as_ref(),
                &state,
                self.area_fluid_active().origin(),
                self.area_fluid_active().dimensions()[0],
                self.area_fluid_active().dimensions()[1],
                self.area_buffered().origin(),
                self.area_buffered().dimensions()[0],
                self.area_buffered().dimensions()[1],
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            );
            state.is_started = true;
            let buffer: wgpu::Buffer = state.buffer.clone();
            let mapped_buffer: wgpu::Buffer = buffer.clone();
            let download: Arc<Mutex<FluidDownload>> = download.clone();
            let particle_capacity: u32 = self.fluids.particle_capacity();
            drop(state);
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let bytes: Result<Vec<u8>, io::Error> = match result {
                        Ok(()) => match mapped_buffer.slice(..).get_mapped_range() {
                            Ok(mapped_data) => {
                                let count: usize =
                                    u32::from_le_bytes(mapped_data[0..4].try_into().unwrap())
                                        as usize;
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
                        let result: Result<Vec<ChunkFluidParticle>, io::Error> =
                            bytes.and_then(|bytes| {
                                FluidDownload::deserialize(&bytes, particle_capacity)
                            });
                        if let Ok(mut state) = download.lock() {
                            state.result = Some(result);
                        }
                    });
                });
        }
        Ok(())
    }

    /// Submits queued dormant-fluid reconstruction and begins result readback
    pub(super) fn fluid_uploads_submit(&self) -> Result<(), io::Error> {
        for upload in &self.fluid_uploads {
            let mut state: std::sync::MutexGuard<FluidUpload> = upload
                .lock()
                .map_err(|_| io::Error::other("Fluid upload is unavailable"))?;
            if state.is_started {
                continue;
            }
            self.fluids.import(
                self.accelerator.as_ref(),
                &state,
                self.area_fluid_active().origin(),
                self.area_fluid_active().dimensions()[0],
                self.area_fluid_active().dimensions()[1],
                self.area_buffered().origin(),
                self.area_buffered().dimensions()[0],
                self.area_buffered().dimensions()[1],
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            )?;
            state.is_started = true;
            let buffer: wgpu::Buffer = state.buffer.clone();
            let mapped_buffer: wgpu::Buffer = buffer.clone();
            let upload: Arc<Mutex<FluidUpload>> = upload.clone();
            drop(state);
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
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
                        if let Ok(mut state) = upload.lock() {
                            let result: Result<Vec<ChunkFluidParticle>, io::Error> =
                                bytes.and_then(|bytes| state.failed_particles(&bytes));
                            state.result = Some(result);
                        }
                    });
                });
        }
        Ok(())
    }
}
