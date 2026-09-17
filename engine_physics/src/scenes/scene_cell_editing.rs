// Copyright Rob Gage 2026

use super::*;

impl Scene {
    pub(super) fn apply_completed_rigid_cellular_reactions(&mut self) -> Result<(), io::Error> {
        let mut chemistry_removals: HashMap<usize, HashSet<[i32; 2]>> = HashMap::new();
        for event in self
            .material_reactions
            .take_rigid_removal_events(self.accelerator.as_ref())
            .into_iter()
            .flatten()
        {
            let body = event[3] as usize;
            let local = [event[4] as i32, event[5] as i32];
            if self
                .rigid_cell_state_generations
                .get(event[0] as usize)
                .copied()
                != Some(event[1])
                || self.rigid_cellular_bodies.get(body).is_none_or(|body| {
                    !body.cells.iter().any(|cell| {
                        cell.state_slot == event[0]
                            && cell.state_generation == event[1]
                            && cell.material.as_u32() == event[2]
                            && cell.local == local
                    })
                })
            {
                continue;
            }
            chemistry_removals.entry(body).or_default().insert(local);
        }
        let mut chemistry_removals: Vec<_> = chemistry_removals.into_iter().collect();
        chemistry_removals.sort_unstable_by_key(|(body, _)| std::cmp::Reverse(*body));
        for (body, cells) in chemistry_removals {
            self.remove_rigid_cellular_body_cells(body, &cells, RigidCellRemovalCause::Chemistry);
        }
        let destroyed: HashSet<[u32; 2]> = self
            .cellular_physics_body_proxy
            .take_destroyed_handles()
            .into_iter()
            .filter(|handle| handle[0] != u32::MAX)
            .collect();
        if !destroyed.is_empty() {
            let mut removals: Vec<(usize, HashSet<[i32; 2]>)> = self
                .rigid_cellular_bodies
                .iter()
                .enumerate()
                .filter_map(|(body, rigid)| {
                    let cells = rigid
                        .cells
                        .iter()
                        .filter(|cell| {
                            destroyed.contains(&[cell.state_slot, cell.state_generation])
                        })
                        .map(|cell| cell.local)
                        .collect::<HashSet<_>>();
                    (!cells.is_empty()).then_some((body, cells))
                })
                .collect();
            removals.sort_unstable_by_key(|(body, _)| std::cmp::Reverse(*body));
            for (body, cells) in removals {
                self.remove_rigid_cellular_body_cells(body, &cells, RigidCellRemovalCause::Erase);
            }
        }
        self.debug_assert_rigid_resident_invariants();
        let body_count: usize = self.rigid_cellular_bodies.len();
        let mut newest = None;
        for batch in self.cellular_pressure.collect_rigid_reactions()? {
            if batch.topology_revision != self.rigid_cellular_topology_revision
                || batch.body_count != body_count
            {
                continue;
            }
            for index in 0..batch.body_count {
                let reaction = batch.reactions[index];
                let granular_contacts = batch.granular_contact_counts[index] != 0;
                let contacts = granular_contacts || batch.contact_counts[index] != 0;
                let wake = contacts != self.rigid_cellular_contact_active[index]
                    || batch.moving_contact_counts[index] != 0;
                if !self.physics_world.apply_rigid_cellular_body_reaction(
                    &self.rigid_cellular_bodies[index],
                    [reaction[0], reaction[1]],
                    reaction[2],
                    batch.energy_budgets[index],
                    wake,
                ) {
                    return Err(io::Error::other("Rigid cellular body handle is missing"));
                }
            }
            newest = Some(batch);
        }
        if let Some(batch) = newest {
            for index in 0..body_count {
                let contacts = batch.contact_counts[index] != 0;
                let wake = contacts != self.rigid_cellular_contact_active[index]
                    || batch.moving_contact_counts[index] != 0;
                self.physics_world.apply_rigid_constraint(
                    &self.rigid_cellular_bodies[index],
                    batch.constraints[index],
                    batch.source_motion[index],
                    wake,
                );
                self.rigid_cellular_contact_active[index] = contacts;
                self.rigid_granular_contact_active[index] =
                    batch.granular_contact_counts[index] != 0;
            }
            if !batch.fractured_slots.is_empty() {
                let fractured: HashSet<u32> = batch.fractured_slots.iter().copied().collect();
                let mut removals: Vec<(usize, HashSet<[i32; 2]>)> = self
                    .rigid_cellular_bodies
                    .iter()
                    .enumerate()
                    .filter_map(|(body, rigid)| {
                        let cells: HashSet<[i32; 2]> = rigid
                            .cells
                            .iter()
                            .filter(|cell| fractured.contains(&cell.state_slot))
                            .map(|cell| cell.local)
                            .collect();
                        (!cells.is_empty()).then_some((body, cells))
                    })
                    .collect();
                removals.sort_unstable_by_key(|(body, _)| std::cmp::Reverse(*body));
                for (body, cells) in removals {
                    self.remove_rigid_cellular_body_cells(
                        body,
                        &cells,
                        RigidCellRemovalCause::Fracture,
                    );
                }
            }
        }
        Ok(())
    }

    /// Validates and inserts one direct, non-canonical rigid cellular body.
    pub(super) fn insert_authored_rigid_cellular_body(
        &mut self,
        placements: Vec<SceneEditCellPlacement>,
    ) {
        let mut unique: BTreeMap<(i32, i32), SceneEditCellPlacement> = BTreeMap::new();
        for placement in placements {
            unique.insert(
                (placement.coordinates.x, placement.coordinates.y),
                placement,
            );
        }
        if unique.is_empty()
            || !unique.values().all(|cell| {
                matches!(
                    self.data.materials().get(cell.material_identifier),
                    Some(Material::CellularStatic { .. })
                )
            })
        {
            return;
        }
        let used: usize = self
            .rigid_cellular_bodies
            .iter()
            .map(|body| body.cells.len())
            .sum();
        if used + unique.len() > self.cellular_physics_body_proxy.rigid_cell_capacity() {
            return;
        }
        let min_x = unique.keys().map(|(x, _)| *x).min().unwrap();
        let min_y = unique.keys().map(|(_, y)| *y).min().unwrap();
        let cells: Vec<_> = unique
            .into_values()
            .map(|cell| RigidCellularBodyCell {
                local: [cell.coordinates.x - min_x, cell.coordinates.y - min_y],
                material: cell.material_identifier,
                appearance: cell.appearance,
                state_slot: u32::MAX,
                state_generation: 0,
            })
            .collect();
        if cells.len() < self.rigid_component_minimum(&cells) {
            let debris = cells
                .into_iter()
                .filter_map(|cell| match self.data.materials().get(cell.material) {
                    Some(Material::CellularStatic {
                        debris_material: Some(material_identifier),
                        debris_yield_rate,
                        ..
                    }) if (((cell.local[0].wrapping_mul(31).wrapping_add(cell.local[1])) as u32
                        % 10_000) as f32)
                        < *debris_yield_rate * 10_000.0 =>
                    {
                        Some(SceneEditCellPlacement {
                            coordinates: CellCoordinates {
                                x: min_x + cell.local[0],
                                y: min_y + cell.local[1],
                            },
                            material_identifier: *material_identifier,
                            appearance: cell.appearance,
                        })
                    }
                    _ => None,
                })
                .collect();
            self.pending_runtime_edits.place_cells(debris);
            return;
        }
        let (friction, restitution) = self.rigid_cellular_material_response(&cells);
        self.insert_rigid_cellular_body(
            [min_x as f32 / 8.0, min_y as f32 / 8.0],
            cells,
            friction,
            restitution,
            None,
        );
    }

    /// Centralizes topology invalidation for every rigid-body insertion.
    pub(super) fn insert_rigid_cellular_body(
        &mut self,
        position: [f32; 2],
        mut cells: Vec<RigidCellularBodyCell>,
        friction: f32,
        restitution: f32,
        inherited_state: Option<&[(f32, f32, f32)]>,
    ) {
        let mut uploads = Vec::new();
        for (index, cell) in cells.iter_mut().enumerate() {
            if cell.state_slot != u32::MAX {
                continue;
            }
            let slot = self
                .rigid_cell_state_free
                .pop()
                .expect("rigid cell capacity checked");
            cell.state_slot = slot;
            cell.state_generation = self.rigid_cell_state_generations[slot as usize];
            let integrity = match self.data.materials().get(cell.material) {
                Some(Material::CellularStatic {
                    default_integrity, ..
                }) => *default_integrity,
                _ => 0.0,
            };
            let temperature = self.initial_temperature(cell.material);
            let (integrity, amount, temperature) = inherited_state
                .and_then(|state| state.get(index).copied())
                .unwrap_or((integrity, 1.0, temperature));
            uploads.push([
                slot,
                integrity.to_bits(),
                amount.to_bits(),
                temperature.to_bits(),
            ]);
        }
        self.rigid_cell_state_upload
            .apply(self.accelerator.as_ref(), &uploads);
        let mut body = self.physics_world.insert_rigid_cellular_body(
            position,
            0.0,
            self.data.materials(),
            cells,
            friction,
            restitution,
            [0.0; 2],
            0.0,
        );
        body.id = self.next_rigid_cellular_body_id();
        self.rigid_cellular_bodies.push(body);
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_support.clear();
        self.rigid_cellular_recovery.clear();
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.debug_assert_rigid_resident_invariants();
    }

    pub(super) fn next_rigid_cellular_body_id(&mut self) -> u64 {
        let id = self.rigid_cellular_body_id_next;
        self.rigid_cellular_body_id_next = self
            .rigid_cellular_body_id_next
            .checked_add(1)
            .expect("rigid body identity exhausted");
        id
    }

    /// Resolves one world cell to a resident physical Accelerator cell
    pub(super) fn cell_edit_index(&self, coordinates: CellCoordinates) -> Option<usize> {
        let tile_coordinates: TileCoordinates = coordinates.tile_coordinates();
        let tile: Tile = self.tile_at(tile_coordinates)?;
        let [x, y]: [usize; 2] = coordinates.local_tile_coordinates();
        if !matches!(
            self.chunks.get(&tile_coordinates.chunk_coordinates()),
            Some(ChunkEntry::Active { chunk, .. }) if chunk.get_tile(tile_coordinates).is_ok()
        ) {
            return None;
        }
        Some(tile.0 as usize * 64 + y * 8 + x)
    }

    /// Writes final contiguous cellular edits to the two authoritative Accelerator buffers
    pub(super) fn write_cell_edits(
        &self,
        edits: &[(
            usize,
            CellCoordinates,
            MaterialIdentifier,
            CellularAppearance,
            f32,
        )],
    ) {
        let mut start: usize = 0;
        while start < edits.len() {
            let mut end: usize = start + 1;
            while end < edits.len() && edits[end].0 == edits[end - 1].0 + 1 {
                end += 1;
            }
            let mut material_identifiers: Vec<u8> = Vec::with_capacity((end - start) * 4);
            let mut appearances: Vec<u8> = Vec::with_capacity((end - start) * 4);
            let mut integrities: Vec<u8> = Vec::with_capacity((end - start) * 4);
            let mut amounts: Vec<u8> = Vec::with_capacity((end - start) * 4);
            let mut temperatures: Vec<u8> = Vec::with_capacity((end - start) * 4);
            for edit in &edits[start..end] {
                material_identifiers.extend_from_slice(&edit.2.as_u32().to_le_bytes());
                appearances.extend_from_slice(&edit.3.0.to_le_bytes());
                integrities.extend_from_slice(&edit.4.to_bits().to_le_bytes());
                let (amount, temperature): (f32, f32) = if edit.2 == MaterialIdentifier::NULL {
                    (0.0, 0.0)
                } else {
                    (1.0, self.initial_temperature(edit.2))
                };
                amounts.extend_from_slice(&amount.to_bits().to_le_bytes());
                temperatures.extend_from_slice(&temperature.to_bits().to_le_bytes());
            }
            let offset: u64 = edits[start].0 as u64 * 4;
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_material_identifiers.wgpu_buffer(),
                offset,
                &material_identifiers,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_appearances.wgpu_buffer(),
                offset,
                &appearances,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_integrities.wgpu_buffer(),
                offset,
                &integrities,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_amounts.wgpu_buffer(),
                offset,
                &amounts,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_temperatures.wgpu_buffer(),
                offset,
                &temperatures,
            );
            self.cellular_dynamic.clear_cellular_dynamic_kinematics(
                self.accelerator.as_ref(),
                edits[start].0,
                end - start,
            );
            self.cellular_pressure.clear_transient_state(
                self.accelerator.as_ref(),
                edits[start].0,
                end - start,
            );
            start = end;
        }
    }
}
