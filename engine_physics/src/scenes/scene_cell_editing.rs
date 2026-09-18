// Copyright Rob Gage 2026

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io;

use super::Scene;
use super::SceneRigidCellRemovalCause;
use crate::chunks::ChunkEntry;
use crate::materials::Material;
use crate::materials::MaterialIdentifier;
use crate::scene_editing::SceneEditCellPlacement;
use crate::simulation::RigidCellularBodyCell;
use crate::simulation_fluids::Fluids;
use crate::tiles::CellCoordinates;
use crate::tiles::CellularAppearance;
use crate::tiles::Tile;
use crate::tiles::TileCoordinates;

impl Scene {
    pub(super) fn queue_resident_cell_clear(
        &self,
        coordinates: CellCoordinates,
        physical_index: usize,
        rigid_destroy_indices: &mut Vec<usize>,
        cell_edits: &mut HashMap<
            usize,
            (CellCoordinates, MaterialIdentifier, CellularAppearance, f32),
        >,
        fluid_edits: &mut HashMap<usize, u32>,
        gas_clear_cells: &mut HashSet<usize>,
        gas_edits: &mut BTreeMap<(usize, u32), f32>,
    ) {
        rigid_destroy_indices.push(physical_index);
        cell_edits.insert(
            physical_index,
            (
                coordinates,
                MaterialIdentifier::NULL,
                CellularAppearance::NEUTRAL,
                0.0,
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

    pub(super) fn apply_completed_rigid_cellular_reactions(&mut self) -> Result<(), io::Error> {
        let mut chemistry_removals: HashMap<usize, HashSet<[i32; 2]>> = HashMap::new();
        for event in self
            .material_reactions
            .take_rigid_removal_events(self.accelerator.as_ref())
            .into_iter()
            .flatten()
        {
            let rigid_body_index: usize = event[3] as usize;
            let rigid_cell_local_coordinates: [i32; 2] = [event[4] as i32, event[5] as i32];
            if self
                .rigid_cell_state_generations
                .get(event[0] as usize)
                .copied()
                != Some(event[1])
                || self
                    .rigid_cellular_bodies
                    .get(rigid_body_index)
                    .is_none_or(|rigid_body| {
                        !rigid_body.cells.iter().any(|cell| {
                            cell.state_slot == event[0]
                                && cell.state_generation == event[1]
                                && cell.material.as_u32() == event[2]
                                && cell.local == rigid_cell_local_coordinates
                        })
                    })
            {
                continue;
            }
            chemistry_removals
                .entry(rigid_body_index)
                .or_default()
                .insert(rigid_cell_local_coordinates);
        }
        let mut chemistry_removals: Vec<(usize, HashSet<[i32; 2]>)> =
            chemistry_removals.into_iter().collect();
        chemistry_removals
            .sort_unstable_by_key(|(rigid_body_index, _)| std::cmp::Reverse(*rigid_body_index));
        for (rigid_body_index, chemistry_removed_cell_coordinates) in chemistry_removals {
            self.remove_rigid_cellular_body_cells(
                rigid_body_index,
                &chemistry_removed_cell_coordinates,
                SceneRigidCellRemovalCause::Chemistry,
            );
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
                .filter_map(|(rigid_body_index, rigid_body)| {
                    let destroyed_rigid_cell_coordinates: HashSet<[i32; 2]> = rigid_body
                        .cells
                        .iter()
                        .filter(|cell| {
                            destroyed.contains(&[cell.state_slot, cell.state_generation])
                        })
                        .map(|cell| cell.local)
                        .collect::<HashSet<_>>();
                    (!destroyed_rigid_cell_coordinates.is_empty())
                        .then_some((rigid_body_index, destroyed_rigid_cell_coordinates))
                })
                .collect();
            removals
                .sort_unstable_by_key(|(rigid_body_index, _)| std::cmp::Reverse(*rigid_body_index));
            for (rigid_body_index, destroyed_rigid_cell_coordinates) in removals {
                self.remove_rigid_cellular_body_cells(
                    rigid_body_index,
                    &destroyed_rigid_cell_coordinates,
                    SceneRigidCellRemovalCause::Erase,
                );
            }
        }
        self.debug_assert_rigid_resident_invariants();
        let body_count: usize = self.rigid_cellular_bodies.len();
        let mut newest_rigid_reaction_batch: Option<crate::simulation::RigidGranularReactionBatch> =
            None;
        for batch in self.cellular_pressure.collect_rigid_reactions()? {
            if batch.topology_revision != self.rigid_cellular_topology_revision
                || batch.body_count != body_count
            {
                continue;
            }
            for rigid_body_index in 0..batch.body_count {
                let rigid_reaction: [f32; 3] = batch.reactions[rigid_body_index];
                let has_granular_contacts: bool =
                    batch.granular_contact_counts[rigid_body_index] != 0;
                let has_any_contacts: bool =
                    has_granular_contacts || batch.contact_counts[rigid_body_index] != 0;
                let should_wake_rigid_body: bool = has_any_contacts
                    != self.rigid_cellular_contact_active[rigid_body_index]
                    || batch.moving_contact_counts[rigid_body_index] != 0;
                if !self.physics_world.apply_rigid_cellular_body_reaction(
                    &self.rigid_cellular_bodies[rigid_body_index],
                    [rigid_reaction[0], rigid_reaction[1]],
                    rigid_reaction[2],
                    batch.energy_budgets[rigid_body_index],
                    should_wake_rigid_body,
                ) {
                    return Err(io::Error::other("Rigid cellular body handle is missing"));
                }
            }
            newest_rigid_reaction_batch = Some(batch);
        }
        if let Some(batch) = newest_rigid_reaction_batch {
            for rigid_body_index in 0..body_count {
                let has_any_contacts: bool = batch.contact_counts[rigid_body_index] != 0;
                let should_wake_rigid_body: bool = has_any_contacts
                    != self.rigid_cellular_contact_active[rigid_body_index]
                    || batch.moving_contact_counts[rigid_body_index] != 0;
                self.physics_world.apply_rigid_constraint(
                    &self.rigid_cellular_bodies[rigid_body_index],
                    batch.constraints[rigid_body_index],
                    batch.source_motion[rigid_body_index],
                    should_wake_rigid_body,
                );
                self.rigid_cellular_contact_active[rigid_body_index] = has_any_contacts;
                self.rigid_granular_contact_active[rigid_body_index] =
                    batch.granular_contact_counts[rigid_body_index] != 0;
            }
            if !batch.fractured_slots.is_empty() {
                let fractured: HashSet<u32> = batch.fractured_slots.iter().copied().collect();
                let mut removals: Vec<(usize, HashSet<[i32; 2]>)> = self
                    .rigid_cellular_bodies
                    .iter()
                    .enumerate()
                    .filter_map(|(rigid_body_index, rigid_body)| {
                        let fractured_rigid_cell_coordinates: HashSet<[i32; 2]> = rigid_body
                            .cells
                            .iter()
                            .filter(|cell| fractured.contains(&cell.state_slot))
                            .map(|cell| cell.local)
                            .collect();
                        (!fractured_rigid_cell_coordinates.is_empty())
                            .then_some((rigid_body_index, fractured_rigid_cell_coordinates))
                    })
                    .collect();
                removals.sort_unstable_by_key(|(rigid_body_index, _)| {
                    std::cmp::Reverse(*rigid_body_index)
                });
                for (rigid_body_index, fractured_rigid_cell_coordinates) in removals {
                    self.remove_rigid_cellular_body_cells(
                        rigid_body_index,
                        &fractured_rigid_cell_coordinates,
                        SceneRigidCellRemovalCause::Fracture,
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
        let resident_rigid_cell_count: usize = self
            .rigid_cellular_bodies
            .iter()
            .map(|body| body.cells.len())
            .sum();
        if resident_rigid_cell_count + unique.len()
            > self.cellular_physics_body_proxy.rigid_cell_capacity()
        {
            return;
        }
        let minimum_local_x: i32 = unique.keys().map(|(x, _)| *x).min().unwrap();
        let minimum_local_y: i32 = unique.keys().map(|(_, y)| *y).min().unwrap();
        let authored_rigid_cell_cells: Vec<RigidCellularBodyCell> = unique
            .into_values()
            .map(|cell| RigidCellularBodyCell {
                local: [
                    cell.coordinates.x - minimum_local_x,
                    cell.coordinates.y - minimum_local_y,
                ],
                material: cell.material_identifier,
                appearance: cell.appearance,
                state_slot: u32::MAX,
                state_generation: 0,
            })
            .collect();
        if authored_rigid_cell_cells.len()
            < self.rigid_component_minimum(&authored_rigid_cell_cells)
        {
            let debris: Vec<SceneEditCellPlacement> = authored_rigid_cell_cells
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
                                x: minimum_local_x + cell.local[0],
                                y: minimum_local_y + cell.local[1],
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
        let (friction, restitution) =
            self.rigid_cellular_material_response(&authored_rigid_cell_cells);
        self.insert_rigid_cellular_body(
            [minimum_local_x as f32 / 8.0, minimum_local_y as f32 / 8.0],
            authored_rigid_cell_cells,
            friction,
            restitution,
            None,
        );
    }

    /// Centralizes topology invalidation for every rigid-body insertion.
    pub(super) fn insert_rigid_cellular_body(
        &mut self,
        position: [f32; 2],
        mut rigid_cellular_body_cells: Vec<RigidCellularBodyCell>,
        friction: f32,
        restitution: f32,
        inherited_state: Option<&[(f32, f32, f32)]>,
    ) {
        let mut uploads: Vec<[u32; 4]> = Vec::new();
        for (rigid_cell_index, cell) in rigid_cellular_body_cells.iter_mut().enumerate() {
            if cell.state_slot != u32::MAX {
                continue;
            }
            let rigid_cell_state_slot: u32 = self
                .rigid_cell_state_free
                .pop()
                .expect("rigid cell capacity checked");
            cell.state_slot = rigid_cell_state_slot;
            cell.state_generation =
                self.rigid_cell_state_generations[rigid_cell_state_slot as usize];
            let integrity: f32 = match self.data.materials().get(cell.material) {
                Some(Material::CellularStatic {
                    default_integrity, ..
                }) => *default_integrity,
                _ => 0.0,
            };
            let temperature: f32 = self.initial_temperature(cell.material);
            let (integrity, amount, temperature): (f32, f32, f32) = inherited_state
                .and_then(|state| state.get(rigid_cell_index).copied())
                .unwrap_or((integrity, 1.0, temperature));
            uploads.push([
                rigid_cell_state_slot,
                integrity.to_bits(),
                amount.to_bits(),
                temperature.to_bits(),
            ]);
        }
        self.rigid_cell_state_upload
            .apply(self.accelerator.as_ref(), &uploads);
        let mut inserted_rigid_cellular_body: crate::simulation::RigidCellularBody =
            self.physics_world.insert_rigid_cellular_body(
                position,
                0.0,
                self.data.materials(),
                rigid_cellular_body_cells,
                friction,
                restitution,
                [0.0; 2],
                0.0,
            );
        inserted_rigid_cellular_body.identifier = self.next_rigid_cellular_body_identifier();
        self.rigid_cellular_bodies
            .push(inserted_rigid_cellular_body);
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

    pub(super) fn next_rigid_cellular_body_identifier(&mut self) -> u64 {
        let identifier: u64 = self.rigid_cellular_body_identifier_next;
        self.rigid_cellular_body_identifier_next = self
            .rigid_cellular_body_identifier_next
            .checked_add(1)
            .expect("rigid body identity exhausted");
        identifier
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
            let cellular_edit_byte_offset: u64 = edits[start].0 as u64 * 4;
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_material_identifiers.wgpu_buffer(),
                cellular_edit_byte_offset,
                &material_identifiers,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_appearances.wgpu_buffer(),
                cellular_edit_byte_offset,
                &appearances,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_integrities.wgpu_buffer(),
                cellular_edit_byte_offset,
                &integrities,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_amounts.wgpu_buffer(),
                cellular_edit_byte_offset,
                &amounts,
            );
            self.accelerator.wgpu_queue().write_buffer(
                self.cellular_temperatures.wgpu_buffer(),
                cellular_edit_byte_offset,
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
