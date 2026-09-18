// Copyright Rob Gage 2026

use super::*;

impl Scene {
    /// Queues mandatory runtime material edits for one coalesced update-time flush.
    pub fn queue_edits(&mut self, edits: SceneEditBatch) {
        self.pending_runtime_edits.append(edits);
    }

    /// Applies a scene-owned transaction immediately; nonresident requests remain queued.
    pub(super) fn apply_edits_immediate(
        &mut self,
        edits: &mut SceneEditBatch,
    ) -> Result<(), io::Error> {
        let mut cell_edits: HashMap<
            usize,
            (CellCoordinates, MaterialIdentifier, CellularAppearance, f32),
        > = HashMap::new();
        let mut fluid_edits: HashMap<usize, u32> = HashMap::new();
        let mut gas_edits: BTreeMap<(usize, u32), f32> = BTreeMap::new();
        let mut gas_clear_cells: HashSet<usize> = HashSet::new();
        let mut thermal_edits: BTreeMap<usize, f32> = BTreeMap::new();
        let mut rigid_destroy_indices = Vec::new();
        let mut deferred = SceneEditBatch::new();
        for edit in edits.drain() {
            let destroy_cells = matches!(&edit, SceneEdit::DestroyCells { .. });
            match edit {
                SceneEdit::PlaceRigidBody { cells } => {
                    if cells.is_empty() {
                        continue;
                    }
                    if !cells
                        .iter()
                        .all(|cell| self.cell_edit_index(cell.coordinates).is_some())
                    {
                        deferred.place_rigid_body(cells);
                    } else {
                        self.insert_authored_rigid_cellular_body(cells);
                    }
                }
                SceneEdit::PlaceCells { cells } => {
                    for SceneEditCellPlacement {
                        coordinates,
                        material_identifier,
                        appearance,
                    } in cells
                    {
                        if let Some(physical_index) = self.cell_edit_index(coordinates) {
                            match self.data.materials().get(material_identifier) {
                                Some(Material::CellularStatic {
                                    default_integrity, ..
                                }) => {
                                    cell_edits.insert(
                                        physical_index,
                                        (
                                            coordinates,
                                            material_identifier,
                                            appearance,
                                            *default_integrity,
                                        ),
                                    );
                                    fluid_edits.insert(physical_index, Fluids::erase_edit());
                                    if self.gases.gas_count() != 0 {
                                        gas_clear_cells.insert(physical_index);
                                        for species in 0..self.gases.gas_count() {
                                            gas_edits.remove(&(physical_index, species));
                                        }
                                    }
                                }
                                Some(Material::CellularDynamic { .. }) => {
                                    cell_edits.insert(
                                        physical_index,
                                        (coordinates, material_identifier, appearance, 0.0),
                                    );
                                    fluid_edits.insert(physical_index, Fluids::erase_edit());
                                    if self.gases.gas_count() != 0 {
                                        gas_clear_cells.insert(physical_index);
                                        for species in 0..self.gases.gas_count() {
                                            gas_edits.remove(&(physical_index, species));
                                        }
                                    }
                                }
                                Some(Material::Fluid { .. }) => {
                                    cell_edits.insert(
                                        physical_index,
                                        (
                                            coordinates,
                                            MaterialIdentifier::NULL,
                                            CellularAppearance::NEUTRAL,
                                            0.0,
                                        ),
                                    );
                                    fluid_edits
                                        .insert(physical_index, material_identifier.as_u32());
                                    if self.gases.gas_count() != 0 {
                                        gas_clear_cells.insert(physical_index);
                                        for species in 0..self.gases.gas_count() {
                                            gas_edits.remove(&(physical_index, species));
                                        }
                                    }
                                }
                                Some(Material::Gas { .. }) => {
                                    gas_edits.insert(
                                        (physical_index, material_identifier.index()),
                                        self.initial_temperature(material_identifier),
                                    );
                                }
                                None => {}
                            }
                        } else {
                            deferred.place_cells(vec![SceneEditCellPlacement {
                                coordinates,
                                material_identifier,
                                appearance,
                            }]);
                        }
                    }
                }
                SceneEdit::Erase { cells } | SceneEdit::DestroyCells { cells } => {
                    for coordinates in cells {
                        if let Some(physical_index) = self.cell_edit_index(coordinates) {
                            self.queue_resident_cell_clear(
                                coordinates,
                                physical_index,
                                &mut rigid_destroy_indices,
                                &mut cell_edits,
                                &mut fluid_edits,
                                &mut gas_clear_cells,
                                &mut gas_edits,
                            );
                        } else {
                            if destroy_cells {
                                deferred.destroy_cells(vec![coordinates]);
                            } else {
                                deferred.erase(vec![coordinates]);
                            }
                        }
                    }
                }
                SceneEdit::Thermal {
                    cells,
                    delta_temperature,
                } => {
                    for coordinates in cells {
                        if let Some(index) = self.cell_edit_index(coordinates) {
                            *thermal_edits.entry(index).or_default() += delta_temperature;
                        } else {
                            deferred.thermal(vec![coordinates], delta_temperature);
                        }
                    }
                }
            }
        }
        edits.append(deferred);
        if !thermal_edits.is_empty() {
            let physical_indices: Vec<u32> =
                thermal_edits.keys().map(|&index| index as u32).collect();
            // the current Accelerator request format has one delta per request; aggregate same-cell
            // edits on the CPU so this flush submits exactly once.
            self.thermal_edits.apply(
                self.accelerator.as_ref(),
                &physical_indices,
                &thermal_edits,
                [
                    self.origin.x - i32::from(self.simulation_buffer_size),
                    self.origin.y - i32::from(self.simulation_buffer_size),
                ],
                [
                    u32::from(self.simulation_width + u16::from(self.simulation_buffer_size) * 2),
                    u32::from(self.simulation_height + u16::from(self.simulation_buffer_size) * 2),
                ],
                [
                    u32::from(self.tiles_ring_offset_x),
                    u32::from(self.tiles_ring_offset_y),
                ],
            );
        }
        if !rigid_destroy_indices.is_empty() {
            rigid_destroy_indices.sort_unstable();
            rigid_destroy_indices.dedup();
            self.cellular_physics_body_proxy
                .resolve_destruction_requests(self.accelerator.as_ref(), &rigid_destroy_indices);
        }
        let mut cell_edits: Vec<(
            usize,
            CellCoordinates,
            MaterialIdentifier,
            CellularAppearance,
            f32,
        )> = cell_edits
            .into_iter()
            .map(
                |(index, (coordinates, material_identifier, appearance, integrity))| {
                    (
                        index,
                        coordinates,
                        material_identifier,
                        appearance,
                        integrity,
                    )
                },
            )
            .collect();
        cell_edits.sort_unstable_by_key(|(physical_index, ..)| *physical_index);
        for (_, coordinates, material_identifier, appearance, integrity) in &cell_edits {
            let tile_coordinates: TileCoordinates = coordinates.tile_coordinates();
            let [x, y]: [usize; 2] = coordinates.local_tile_coordinates();
            let Some(ChunkEntry::Active { chunk, is_dirty }) =
                self.chunks.get_mut(&tile_coordinates.chunk_coordinates())
            else {
                return Err(io::Error::other("Resident tile chunk is not active"));
            };
            chunk
                .set_cell_with_integrity(
                    tile_coordinates,
                    x,
                    y,
                    *material_identifier,
                    *appearance,
                    *integrity,
                )
                .map_err(|_| io::Error::other("Resident tile is not in its active chunk"))?;
            *is_dirty = true;
        }
        if !cell_edits.is_empty() {
            self.cellular_collision_dirty = true;
            self.write_cell_edits(&cell_edits);
        }
        if !fluid_edits.is_empty() {
            let mut fluid_edits: Vec<(usize, u32, f32, f32)> = fluid_edits
                .into_iter()
                .map(|(index, material)| {
                    let identifier = MaterialIdentifier::from_u32(material);
                    let temperature = if identifier == MaterialIdentifier::NULL {
                        0.0
                    } else {
                        self.initial_temperature(identifier)
                    };
                    (index, material, 1.0, temperature)
                })
                .collect();
            fluid_edits.sort_unstable_by_key(|(physical_index, ..)| *physical_index);
            let buffer_size: i32 = i32::from(self.simulation_buffer_size);
            let dimensions: u16 = u16::from(self.simulation_buffer_size) * 2;
            let fluid_active_area: TileArea = self.area_fluid_active();
            let fluid_active_dimensions: [u16; 2] = fluid_active_area.dimensions();
            self.fluids.apply_edits(
                self.accelerator.as_ref(),
                &fluid_edits,
                fluid_active_area.origin(),
                fluid_active_dimensions[0],
                fluid_active_dimensions[1],
                TileCoordinates {
                    x: self.origin.x - buffer_size,
                    y: self.origin.y - buffer_size,
                },
                self.simulation_width + dimensions,
                self.simulation_height + dimensions,
                self.tiles_ring_offset_x,
                self.tiles_ring_offset_y,
            );
        }
        if !gas_edits.is_empty() || !gas_clear_cells.is_empty() {
            let mut authored_temperature_by_cell = BTreeMap::<usize, (f32, usize)>::new();
            // BTreeMap ordering plus this average keeps shared-cell gas temperature deterministic.
            for (&(cell, _), &temperature) in &gas_edits {
                let entry = authored_temperature_by_cell.entry(cell).or_insert((0.0, 0));
                entry.0 += temperature;
                entry.1 += 1;
            }
            let gas_edits: Vec<(usize, u32, f32)> = gas_edits
                .into_iter()
                .map(|((cell, species), _)| {
                    let (sum, count) = authored_temperature_by_cell[&cell];
                    (cell, species, sum / count as f32)
                })
                .collect();
            let mut gas_clear_cells: Vec<usize> = gas_clear_cells.into_iter().collect();
            gas_clear_cells.sort_unstable();
            self.gases
                .apply_edits(self.accelerator.as_ref(), &gas_edits, &gas_clear_cells);
        }
        Ok(())
    }

    /// Applies one Accelerator radial cellular impulse without moving cells immediately.
    pub fn apply_cellular_radial_impulse(
        &mut self,
        center: CellCoordinates,
        radius_cells: f32,
        strength: f32,
    ) {
        let buffer_size: i32 = i32::from(self.simulation_buffer_size);
        let dimensions: u16 = u16::from(self.simulation_buffer_size) * 2;
        self.cellular_pressure.apply_radial_impulse(
            self.accelerator.as_ref(),
            TileCoordinates {
                x: self.origin.x - buffer_size,
                y: self.origin.y - buffer_size,
            },
            self.simulation_width + dimensions,
            self.simulation_height + dimensions,
            self.tiles_ring_offset_x,
            self.tiles_ring_offset_y,
            center,
            radius_cells.max(0.75),
            strength,
        );
    }
}
