// Copyright Rob Gage 2026

use super::*;

impl Scene {
    pub(super) fn gas_downloads_queue(&mut self, area: TileArea) {
        if self.gases.gas_count() == 0 {
            return;
        }
        let dimensions: u16 = (self.simulation_width + u16::from(self.simulation_buffer_size) * 2)
            .max(self.simulation_height + u16::from(self.simulation_buffer_size) * 2);
        let maximum_cell_count: u32 =
            u32::from(self.tile_streaming_batch_size) * u32::from(dimensions) * 64;
        let download: Arc<Mutex<GasDownload>> = self.gas_download_pool.pop().unwrap_or_else(|| {
            Arc::new(Mutex::new(GasDownload::new(
                self.accelerator.as_ref(),
                area,
                maximum_cell_count,
                self.gases.gas_count(),
            )))
        });
        download.lock().unwrap().reset(area);
        self.gas_downloads.push(download);
    }

    /// Returns whether an incoming area overlaps unresolved exported gas
    pub(super) fn gas_download_pending_in(&self, area: TileArea) -> Result<bool, io::Error> {
        for download in &self.gas_downloads {
            if download
                .lock()
                .map_err(|_| io::Error::other("Gas download is unavailable"))?
                .area
                .intersects(area)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns whether a chunk is pinned by an unresolved gas export
    pub(super) fn gas_transfer_pending_for_chunk(
        &self,
        coordinates: TileCoordinates,
    ) -> Result<bool, io::Error> {
        for download in &self.gas_downloads {
            if download
                .lock()
                .map_err(|_| io::Error::other("Gas download is unavailable"))?
                .area
                .chunk_area()
                .contains(coordinates)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Clears a world area through its current physical ring mapping
    pub(super) fn gas_clear_area(&self, area: TileArea) {
        let buffered_area: TileArea = self.area_buffered();
        let dimensions: [u16; 2] = buffered_area.dimensions();
        self.gases.clear_area(
            self.accelerator.as_ref(),
            area,
            buffered_area.origin(),
            dimensions[0],
            dimensions[1],
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
        );
    }

    /// Moves dormant sparse gas from CPU chunks into dense resident Accelerator fields
    pub(super) fn gas_upload_area(&mut self, area: TileArea) -> Result<(), io::Error> {
        if self.gases.gas_count() == 0 {
            return Ok(());
        }
        let chunk_coordinates: Vec<TileCoordinates> =
            area.chunk_area().iterate_chunk_coordinates().collect();
        for coordinates in &chunk_coordinates {
            if !matches!(
                self.chunks.get(coordinates),
                Some(ChunkEntry::Active { .. })
            ) {
                return Err(io::Error::other("Incoming gas chunk is not active"));
            }
        }
        let mut cells: Vec<ChunkGasCell> = Vec::new();
        for coordinates in chunk_coordinates {
            let Some(ChunkEntry::Active { chunk, is_dirty }) = self.chunks.get_mut(&coordinates)
            else {
                unreachable!();
            };
            let mut chunk_cells: Vec<ChunkGasCell> = chunk.take_dormant_gas_cells(area);
            if !chunk_cells.is_empty() {
                *is_dirty = true;
            }
            cells.append(&mut chunk_cells);
        }
        if cells.is_empty() {
            return Ok(());
        }
        let mut cells = cells;
        for cell in &mut cells {
            if !cell.temperature.is_finite() {
                cell.temperature = self.ambient_temperature;
            }
        }
        let upload: GasUpload = GasUpload::new(area, cells);
        if let Err(error) = upload.validate(self.data.materials()) {
            self.gas_cells_restore(upload.cells)?;
            return Err(error);
        }
        let physical_indices: Option<Vec<usize>> = upload
            .cells
            .iter()
            .map(|cell| self.cell_edit_index(cell.coordinates))
            .collect();
        let Some(physical_indices) = physical_indices else {
            self.gas_cells_restore(upload.cells)?;
            return Err(io::Error::other(
                "Incoming dormant gas cell is outside Accelerator residency",
            ));
        };
        self.gases
            .import(self.accelerator.as_ref(), &upload, &physical_indices);
        Ok(())
    }

    /// Returns sparse gas cells to their owning CPU chunks after a failed import validation
    pub(super) fn gas_cells_restore(&mut self, cells: Vec<ChunkGasCell>) -> Result<(), io::Error> {
        for cell in cells {
            let coordinates: TileCoordinates = cell.tile_coordinates().chunk_coordinates();
            let Some(ChunkEntry::Active { chunk, is_dirty }) = self.chunks.get_mut(&coordinates)
            else {
                return Err(io::Error::other("Dormant gas source chunk is not active"));
            };
            chunk
                .insert_dormant_gas_cell(cell)
                .map_err(|_| io::Error::other("Dormant gas cell is outside its source chunk"))?;
            *is_dirty = true;
        }
        Ok(())
    }

    /// Applies completed gas exports to their world-position CPU chunks
    pub(super) fn gas_downloads_apply_completed(&mut self) -> Result<(), io::Error> {
        let mut index: usize = 0;
        while index < self.gas_downloads.len() {
            let mut download = self.gas_downloads[index]
                .lock()
                .map_err(|_| io::Error::other("Gas download is unavailable"))?;
            let Some(result) = download.result.as_ref() else {
                index += 1;
                continue;
            };
            if let Err(error) = result {
                return Err(io::Error::other(format!("Gas download failed: {error}")));
            }
            let area: TileArea = download.area;
            for cell in result.as_ref().unwrap() {
                let coordinates: TileCoordinates = cell.tile_coordinates();
                if !area.contains(coordinates)
                    || !matches!(
                        self.chunks.get(&coordinates.chunk_coordinates()),
                        Some(ChunkEntry::Active { .. }),
                    )
                {
                    return Err(io::Error::other(
                        "Exported gas cell has no active destination chunk",
                    ));
                }
            }
            let cells: Vec<ChunkGasCell> = download.result.take().unwrap().unwrap();
            drop(download);
            self.gas_cells_restore(cells)?;
            let download: Arc<Mutex<GasDownload>> = self.gas_downloads.swap_remove(index);
            self.gas_download_pool.push(download);
        }
        Ok(())
    }

    /// Submits queued gas exports and begins their asynchronous readbacks
    pub(super) fn gas_downloads_submit(&self) -> Result<(), io::Error> {
        for download in &self.gas_downloads {
            let mut state = download
                .lock()
                .map_err(|_| io::Error::other("Gas download is unavailable"))?;
            if state.is_started {
                continue;
            }
            let buffered_area: TileArea = self.area_buffered();
            let buffered_dimensions: [u16; 2] = buffered_area.dimensions();
            self.gases.export(
                self.accelerator.as_ref(),
                &state,
                buffered_area.origin(),
                buffered_dimensions[0],
                buffered_dimensions[1],
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            );
            state.is_started = true;
            let area: TileArea = state.area;
            let dimensions: [u16; 2] = area.dimensions();
            let byte_count: usize = usize::from(dimensions[0])
                * usize::from(dimensions[1])
                * 64
                * (5 + self.gases.gas_count() as usize)
                * 4;
            let gas_identifiers: Vec<MaterialIdentifier> = self
                .data
                .materials()
                .iter()
                .filter_map(|(identifier, material)| {
                    matches!(material, Material::Gas { .. }).then_some(identifier)
                })
                .collect();
            let buffer: wgpu::Buffer = state.buffer.clone();
            let mapped_buffer: wgpu::Buffer = buffer.clone();
            let download: Arc<Mutex<GasDownload>> = download.clone();
            drop(state);
            buffer
                .slice(0..byte_count as u64)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let bytes: Result<Vec<u8>, io::Error> = match result {
                        Ok(()) => {
                            match mapped_buffer.slice(0..byte_count as u64).get_mapped_range() {
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
                            }
                        }
                        Err(_) => Err(io::Error::other("Gas download failed")),
                    };
                    std::thread::spawn(move || {
                        let result: Result<Vec<ChunkGasCell>, io::Error> =
                            bytes.and_then(|bytes| {
                                GasDownload::deserialize(&bytes, area, &gas_identifiers)
                            });
                        if let Ok(mut state) = download.lock() {
                            state.result = Some(result);
                        }
                    });
                });
        }
        Ok(())
    }
}
