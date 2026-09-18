// Copyright Rob Gage 2026

use std::io;
use std::sync::Arc;
use std::sync::Mutex;

use super::Scene;
use crate::actors::Actor;
use crate::chunks::ChunkEntry;
use crate::chunks::ChunkFluidParticle;
use crate::materials::Material;
use crate::scenes::FluidDownload;
use crate::scenes::FluidUpload;
use crate::tiles::TileArea;
use crate::tiles::TileCoordinates;

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
        let mut fluid_download_index: usize = 0;
        while fluid_download_index < self.fluid_downloads.len() {
            let mut download: std::sync::MutexGuard<'_, FluidDownload> = self.fluid_downloads
                [fluid_download_index]
                .lock()
                .map_err(|_| io::Error::other("Fluid download is unavailable"))?;
            let Some(result) = download.result.as_ref() else {
                fluid_download_index += 1;
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
            let download: Arc<Mutex<FluidDownload>> =
                self.fluid_downloads.swap_remove(fluid_download_index);
            self.fluid_download_pool.push(download);
        }
        Ok(())
    }

    /// Restores failed imports to CPU ownership and completes successful transfers
    pub(super) fn fluid_uploads_apply_completed(&mut self) -> Result<(), io::Error> {
        let mut fluid_upload_index: usize = 0;
        while fluid_upload_index < self.fluid_uploads.len() {
            let mut upload: std::sync::MutexGuard<'_, FluidUpload> = self.fluid_uploads
                [fluid_upload_index]
                .lock()
                .map_err(|_| io::Error::other("Fluid upload is unavailable"))?;
            let Some(result) = upload.result.as_ref() else {
                fluid_upload_index += 1;
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
            self.fluid_uploads.swap_remove(fluid_upload_index);
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
        let mapped_fluid_sample_buffer: wgpu::Buffer = self.fluid_sample_buffer.clone();
        let fluid_sample_result_store: Arc<Mutex<Option<Result<[f32; 5], String>>>> =
            self.fluid_sample_result.clone();
        self.fluid_sample_buffer.slice(..).map_async(
            wgpu::MapMode::Read,
            move |fluid_sample_mapping_result| {
                let sample: Result<[f32; 5], String> = fluid_sample_mapping_result
                    .map_err(|_| "Pawn fluid sample readback failed".to_owned())
                    .and_then(|()| {
                        mapped_fluid_sample_buffer
                            .slice(..)
                            .get_mapped_range()
                            .map_err(|error| error.to_string())
                            .and_then(|mapped| {
                                let sample: [f32; 5] =
                                    std::array::from_fn(|fluid_sample_component_index| {
                                        f32::from_bits(u32::from_le_bytes(
                                            mapped[fluid_sample_component_index * 4
                                                ..fluid_sample_component_index * 4 + 4]
                                                .try_into()
                                                .unwrap(),
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
                mapped_fluid_sample_buffer.unmap();
                if let Ok(mut fluid_sample_result) = fluid_sample_result_store.lock() {
                    *fluid_sample_result = Some(sample);
                }
            },
        );
        Ok(())
    }

    /// Submits queued fluid exports and begins their asynchronous readbacks
    pub(super) fn fluid_downloads_submit(&self) -> Result<(), io::Error> {
        for download in &self.fluid_downloads {
            let mut fluid_transfer_state: std::sync::MutexGuard<FluidDownload> = download
                .lock()
                .map_err(|_| io::Error::other("Fluid download is unavailable"))?;
            if fluid_transfer_state.is_started {
                continue;
            }
            self.fluids.export(
                self.accelerator.as_ref(),
                &fluid_transfer_state,
                self.area_fluid_active().origin(),
                self.area_fluid_active().dimensions()[0],
                self.area_fluid_active().dimensions()[1],
                self.area_buffered().origin(),
                self.area_buffered().dimensions()[0],
                self.area_buffered().dimensions()[1],
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            );
            fluid_transfer_state.is_started = true;
            let fluid_download_buffer: wgpu::Buffer = fluid_transfer_state.buffer.clone();
            let mapped_fluid_download_buffer: wgpu::Buffer = fluid_download_buffer.clone();
            let download: Arc<Mutex<FluidDownload>> = download.clone();
            let particle_capacity: u32 = self.fluids.particle_capacity();
            drop(fluid_transfer_state);
            fluid_download_buffer.slice(..).map_async(
                wgpu::MapMode::Read,
                move |fluid_download_mapping_result| {
                    let fluid_download_bytes_result: Result<Vec<u8>, io::Error> =
                        match fluid_download_mapping_result {
                            Ok(()) => {
                                match mapped_fluid_download_buffer.slice(..).get_mapped_range() {
                                    Ok(mapped_data) => {
                                        let fluid_download_particle_count: usize =
                                            u32::from_le_bytes(
                                                mapped_data[0..4].try_into().unwrap(),
                                            ) as usize;
                                        let fluid_download_byte_count: usize =
                                            if fluid_download_particle_count
                                                <= particle_capacity as usize
                                            {
                                                16 + fluid_download_particle_count
                                                    * ChunkFluidParticle::GPU_SIZE
                                            } else {
                                                16
                                            };
                                        let fluid_download_bytes: Vec<u8> =
                                            mapped_data[..fluid_download_byte_count].to_vec();
                                        drop(mapped_data);
                                        mapped_fluid_download_buffer.unmap();
                                        Ok(fluid_download_bytes)
                                    }
                                    Err(error) => {
                                        mapped_fluid_download_buffer.unmap();
                                        Err(io::Error::other(error.to_string()))
                                    }
                                }
                            }
                            Err(_) => Err(io::Error::other("Fluid download failed")),
                        };
                    std::thread::spawn(move || {
                        let fluid_download_result: Result<Vec<ChunkFluidParticle>, io::Error> =
                            fluid_download_bytes_result.and_then(|fluid_download_bytes| {
                                FluidDownload::deserialize(&fluid_download_bytes, particle_capacity)
                            });
                        if let Ok(mut fluid_transfer_state) = download.lock() {
                            fluid_transfer_state.result = Some(fluid_download_result);
                        }
                    });
                },
            );
        }
        Ok(())
    }

    /// Submits queued dormant-fluid reconstruction and begins result readback
    pub(super) fn fluid_uploads_submit(&self) -> Result<(), io::Error> {
        for upload in &self.fluid_uploads {
            let mut fluid_transfer_state: std::sync::MutexGuard<FluidUpload> = upload
                .lock()
                .map_err(|_| io::Error::other("Fluid upload is unavailable"))?;
            if fluid_transfer_state.is_started {
                continue;
            }
            self.fluids.import(
                self.accelerator.as_ref(),
                &fluid_transfer_state,
                self.area_fluid_active().origin(),
                self.area_fluid_active().dimensions()[0],
                self.area_fluid_active().dimensions()[1],
                self.area_buffered().origin(),
                self.area_buffered().dimensions()[0],
                self.area_buffered().dimensions()[1],
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            )?;
            fluid_transfer_state.is_started = true;
            let fluid_upload_buffer: wgpu::Buffer = fluid_transfer_state.buffer.clone();
            let mapped_fluid_upload_buffer: wgpu::Buffer = fluid_upload_buffer.clone();
            let upload: Arc<Mutex<FluidUpload>> = upload.clone();
            drop(fluid_transfer_state);
            fluid_upload_buffer.slice(..).map_async(
                wgpu::MapMode::Read,
                move |fluid_upload_mapping_result| {
                    let fluid_upload_bytes_result: Result<Vec<u8>, io::Error> =
                        match fluid_upload_mapping_result {
                            Ok(()) => match mapped_fluid_upload_buffer.slice(..).get_mapped_range()
                            {
                                Ok(mapped_data) => {
                                    let fluid_upload_bytes: Vec<u8> = mapped_data.to_vec();
                                    drop(mapped_data);
                                    mapped_fluid_upload_buffer.unmap();
                                    Ok(fluid_upload_bytes)
                                }
                                Err(error) => {
                                    mapped_fluid_upload_buffer.unmap();
                                    Err(io::Error::other(error.to_string()))
                                }
                            },
                            Err(_) => Err(io::Error::other("Fluid upload result readback failed")),
                        };
                    std::thread::spawn(move || {
                        if let Ok(mut fluid_transfer_state) = upload.lock() {
                            let fluid_upload_result: Result<Vec<ChunkFluidParticle>, io::Error> =
                                fluid_upload_bytes_result.and_then(|fluid_upload_bytes| {
                                    fluid_transfer_state.failed_particles(&fluid_upload_bytes)
                                });
                            fluid_transfer_state.result = Some(fluid_upload_result);
                        }
                    });
                },
            );
        }
        Ok(())
    }
}
