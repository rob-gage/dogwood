// Copyright Rob Gage 2026
use std::collections::HashSet;
use std::io;

use super::RIGID_DETACHMENT_MAXIMUM_CELLS;
use super::Scene;
use super::ScenePendingStaticDetachment;
use crate::materials::Material;
use crate::materials::MaterialIdentifier;
use crate::scene_editing::SceneEditBatch;
use crate::scene_editing::SceneEditCellPlacement;
use crate::simulation::CollisionOccupancySnapshot;
use crate::simulation::RigidCellularBodyCell;
use crate::simulation::RigidCellularBodyState;
use crate::simulation_cellulars::CellularStaticState;
use crate::tiles::CellCoordinates;
use crate::tiles::CellularAppearance;

impl Scene {
    /// Transfers newly disconnected static components into authoritative body-local matter
    pub(super) fn apply_completed_static_detachment(&mut self) -> Result<(), io::Error> {
        let Some(result) = self.cellular_static_state_gather.take_completed() else {
            return Ok(());
        };
        let Some(in_flight_generation) = self.static_detachment_in_flight_generation.take() else {
            return Ok(());
        };
        let Some(pending) = self.pending_static_detachment.take() else {
            return Ok(());
        };
        if pending.generation != in_flight_generation {
            #[cfg(debug_assertions)]
            tracing::trace!(
                target: "engine_physics::static_detachment",
                generation = in_flight_generation,
                "discarded stale static detachment gather"
            );
            self.pending_static_detachment = Some(pending);
            return Ok(());
        }
        let static_detachment_states: Vec<CellularStaticState> = match result {
            Ok(static_detachment_states)
                if static_detachment_states.len() == pending.indices.len() =>
            {
                static_detachment_states
            }
            _ => {
                self.pending_static_detachment = Some(pending);
                return Ok(());
            }
        };
        let current_indices: Option<Vec<u32>> = pending
            .components
            .iter()
            .flatten()
            .map(|coordinates| self.cell_edit_index(*coordinates).map(|index| index as u32))
            .collect();
        if current_indices.as_deref() != Some(&pending.indices) {
            self.static_detachment_generation = self.static_detachment_generation.wrapping_add(1);
            self.pending_static_detachment =
                current_indices.map(|indices| ScenePendingStaticDetachment {
                    components: pending.components,
                    indices,
                    generation: self.static_detachment_generation,
                    ring_offset: (self.tiles_ring_offset_x, self.tiles_ring_offset_y),
                });
            return Ok(());
        }
        if pending.ring_offset != (self.tiles_ring_offset_x, self.tiles_ring_offset_y) {
            self.static_detachment_generation = self.static_detachment_generation.wrapping_add(1);
            self.pending_static_detachment = Some(ScenePendingStaticDetachment {
                components: pending.components,
                indices: current_indices.unwrap_or_default(),
                generation: self.static_detachment_generation,
                ring_offset: (self.tiles_ring_offset_x, self.tiles_ring_offset_y),
            });
            return Ok(());
        }
        let mut rigid_detachment_state_offset: usize = 0;
        let mut edits: SceneEditBatch = SceneEditBatch::new();
        let mut rigid_insertions: Vec<(
            [f32; 2],
            Vec<RigidCellularBodyCell>,
            f32,
            f32,
            Vec<(f32, f32, f32)>,
        )> = Vec::new();
        for component in pending.components {
            let rigid_detachment_state_end: usize = rigid_detachment_state_offset + component.len();
            let component_states: &[CellularStaticState] = &static_detachment_states
                [rigid_detachment_state_offset..rigid_detachment_state_end];
            rigid_detachment_state_offset = rigid_detachment_state_end;
            if component_states.iter().any(|state| {
                state.amount <= 0.000001
                    || !matches!(
                        self.data
                            .materials()
                            .get(MaterialIdentifier::from_u32(state.material)),
                        Some(Material::CellularStatic { .. })
                    )
            }) {
                continue;
            }
            let minimum_x: i32 = component.iter().map(|cell| cell.x).min().unwrap();
            let minimum_y: i32 = component.iter().map(|cell| cell.y).min().unwrap();
            let mut detached_rigid_cells: Vec<RigidCellularBodyCell> =
                Vec::with_capacity(component.len());
            let mut integrities: Vec<f32> = Vec::with_capacity(component.len());
            let mut amounts: Vec<f32> = Vec::with_capacity(component.len());
            let mut temperatures: Vec<f32> = Vec::with_capacity(component.len());
            let mut friction: f32 = 0.0;
            let mut restitution: f32 = 0.0;
            for (coordinates, state) in component.iter().zip(component_states) {
                let material_identifier: MaterialIdentifier =
                    MaterialIdentifier::from_u32(state.material);
                let Some(Material::CellularStatic {
                    friction: cell_friction,
                    restitution: cell_restitution,
                    ..
                }) = self.data.materials().get(material_identifier)
                else {
                    detached_rigid_cells.clear();
                    break;
                };
                friction += *cell_friction;
                restitution += *cell_restitution;
                detached_rigid_cells.push(RigidCellularBodyCell {
                    local: [coordinates.x - minimum_x, coordinates.y - minimum_y],
                    material: material_identifier,
                    appearance: CellularAppearance(state.appearance),
                    state_slot: u32::MAX,
                    state_generation: 0,
                });
                integrities.push(state.integrity);
                amounts.push(state.amount);
                temperatures.push(state.temperature);
            }
            if detached_rigid_cells.len() != component.len() {
                continue;
            }
            if detached_rigid_cells.len() < self.rigid_component_minimum(&detached_rigid_cells) {
                let debris: Vec<SceneEditCellPlacement> = detached_rigid_cells
                    .iter()
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
                                    x: minimum_x + cell.local[0],
                                    y: minimum_y + cell.local[1],
                                },
                                material_identifier: *material_identifier,
                                appearance: cell.appearance,
                            })
                        }
                        _ => None,
                    })
                    .collect();
                edits.erase(component.clone());
                edits.place_cells(debris);
                continue;
            }
            let divisor: f32 = detached_rigid_cells.len() as f32;
            edits.erase(component.clone());
            rigid_insertions.push((
                [minimum_x as f32 / 8.0, minimum_y as f32 / 8.0],
                detached_rigid_cells,
                friction / divisor,
                restitution / divisor,
                integrities
                    .iter()
                    .copied()
                    .zip(amounts.iter().copied().zip(temperatures.iter().copied()))
                    .map(|(integrity, (amount, temperature))| (integrity, amount, temperature))
                    .collect::<Vec<_>>(),
            ));
        }
        if !edits.is_empty() {
            self.apply_edits_immediate(&mut edits)?;
        }
        for (position, detached_rigid_cells, friction, restitution, state) in rigid_insertions {
            self.insert_rigid_cellular_body(
                position,
                detached_rigid_cells,
                friction,
                restitution,
                Some(&state),
            );
        }
        #[cfg(debug_assertions)]
        tracing::trace!(
            target: "engine_physics::static_detachment",
            resolved_cells = rigid_detachment_state_offset,
            edit_applied = !edits.is_empty(),
            "resolved static detachment batch"
        );
        Ok(())
    }

    pub(super) fn submit_pending_static_detachment(&mut self) {
        if self.static_detachment_in_flight_generation.is_some() {
            return;
        }
        let Some(pending) = self.pending_static_detachment.as_ref() else {
            return;
        };
        if self
            .cellular_static_state_gather
            .submit(self.accelerator.as_ref(), &pending.indices)
        {
            self.static_detachment_in_flight_generation = Some(pending.generation);
            #[cfg(debug_assertions)]
            tracing::trace!(
                target: "engine_physics::static_detachment",
                generation = pending.generation,
                gather_cells = pending.indices.len(),
                "submitted static detachment gather"
            );
        }
    }
    pub(super) fn detach_unanchored_static_components(
        &mut self,
        snapshot: &mut CollisionOccupancySnapshot,
    ) -> Result<(), io::Error> {
        let Some(previous) = self.rigid_detachment_snapshot.take() else {
            self.rigid_detachment_snapshot = Some(snapshot.clone());
            return Ok(());
        };
        if previous.origin != snapshot.origin
            || previous.width != snapshot.width
            || previous.height != snapshot.height
        {
            self.rigid_detachment_snapshot = Some(snapshot.clone());
            if let Some(pending) = self.pending_static_detachment.take() {
                let indices: Vec<u32> = pending
                    .components
                    .iter()
                    .flatten()
                    .filter_map(|coordinates| self.cell_edit_index(*coordinates))
                    .map(|index| index as u32)
                    .collect::<Vec<_>>();
                self.static_detachment_generation =
                    self.static_detachment_generation.wrapping_add(1);
                self.pending_static_detachment =
                    (!indices.is_empty()).then_some(ScenePendingStaticDetachment {
                        components: pending.components,
                        indices,
                        generation: self.static_detachment_generation,
                        ring_offset: (self.tiles_ring_offset_x, self.tiles_ring_offset_y),
                    });
            }
            self.submit_pending_static_detachment();
            return Ok(());
        }
        if previous.static_masks == snapshot.static_masks {
            self.submit_pending_static_detachment();
            self.rigid_detachment_snapshot = Some(snapshot.clone());
            return Ok(());
        }
        let origin_x: i32 = snapshot.origin.x * 8;
        let origin_y: i32 = snapshot.origin.y * 8;
        let width: i32 = i32::from(snapshot.width) * 8;
        let height: i32 = i32::from(snapshot.height) * 8;
        let mut seeds: Vec<CellCoordinates> = Vec::new();
        let mut changed_bits: usize = 0;
        for tile_y in 0..snapshot.height {
            for tile_x in 0..snapshot.width {
                let tile: usize =
                    usize::from(tile_y) * usize::from(snapshot.width) + usize::from(tile_x);
                for word in 0..2 {
                    let current: u32 = snapshot.static_masks[tile][word];
                    let previous: u32 = previous.static_masks[tile][word];
                    let added: u32 = current & !previous;
                    let removed: u32 = previous & !current;
                    changed_bits += (added | removed).count_ones() as usize;
                    for mask in [added, removed] {
                        let mut bits: u32 = mask;
                        while bits != 0 {
                            let bit: u32 = bits.trailing_zeros();
                            let local: i32 = word as i32 * 32 + bit as i32;
                            let cell: CellCoordinates = CellCoordinates {
                                x: (snapshot.origin.x + i32::from(tile_x)) * 8 + local % 8,
                                y: (snapshot.origin.y + i32::from(tile_y)) * 8 + local / 8,
                            };
                            if mask == added {
                                seeds.push(cell);
                            } else {
                                for neighbor in [
                                    CellCoordinates {
                                        x: cell.x - 1,
                                        y: cell.y,
                                    },
                                    CellCoordinates {
                                        x: cell.x + 1,
                                        y: cell.y,
                                    },
                                    CellCoordinates {
                                        x: cell.x,
                                        y: cell.y - 1,
                                    },
                                    CellCoordinates {
                                        x: cell.x,
                                        y: cell.y + 1,
                                    },
                                ] {
                                    if snapshot.is_static_cell_occupied(neighbor.x, neighbor.y)
                                        == Some(true)
                                    {
                                        seeds.push(neighbor);
                                    }
                                }
                            }
                            bits &= bits - 1;
                        }
                    }
                }
            }
        }
        seeds.sort_unstable_by_key(|cell| (cell.y, cell.x));
        seeds.dedup();
        let visit_count: usize = (width * height) as usize;
        if self.static_detachment_visit_stamps.len() != visit_count {
            self.static_detachment_visit_stamps = vec![0; visit_count];
            self.static_detachment_visit_generation = 0;
        }
        self.static_detachment_visit_generation = self
            .static_detachment_visit_generation
            .wrapping_add(1)
            .max(1);
        let visit_generation: u32 = self.static_detachment_visit_generation;
        let visit_index: &dyn Fn(CellCoordinates) -> usize =
            &|cell: CellCoordinates| ((cell.y - origin_y) * width + cell.x - origin_x) as usize;
        let mut candidates: Vec<Vec<CellCoordinates>> = Vec::new();
        let mut visited_cells: usize = 0;
        for seed in seeds.iter().copied() {
            if snapshot.is_static_cell_occupied(seed.x, seed.y) != Some(true)
                || self.static_detachment_visit_stamps[visit_index(seed)] == visit_generation
            {
                continue;
            }
            let mut queue: Vec<CellCoordinates> = vec![seed];
            self.static_detachment_visit_stamps[visit_index(seed)] = visit_generation;
            let mut component: Vec<CellCoordinates> = Vec::new();
            let mut cursor: usize = 0;
            let mut anchored: bool = false;
            while cursor < queue.len() {
                let cell: CellCoordinates = queue[cursor];
                cursor += 1;
                component.push(cell);
                visited_cells += 1;
                anchored |= cell.x == origin_x
                    || cell.y == origin_y
                    || cell.x == origin_x + width - 1
                    || cell.y == origin_y + height - 1;
                if anchored || component.len() > RIGID_DETACHMENT_MAXIMUM_CELLS {
                    break;
                }
                for neighbor in [
                    CellCoordinates {
                        x: cell.x - 1,
                        y: cell.y,
                    },
                    CellCoordinates {
                        x: cell.x + 1,
                        y: cell.y,
                    },
                    CellCoordinates {
                        x: cell.x,
                        y: cell.y - 1,
                    },
                    CellCoordinates {
                        x: cell.x,
                        y: cell.y + 1,
                    },
                ] {
                    if neighbor.x < origin_x
                        || neighbor.y < origin_y
                        || neighbor.x >= origin_x + width
                        || neighbor.y >= origin_y + height
                        || snapshot.is_static_cell_occupied(neighbor.x, neighbor.y) != Some(true)
                    {
                        continue;
                    }
                    let static_detachment_visit_index: usize = visit_index(neighbor);
                    if self.static_detachment_visit_stamps[static_detachment_visit_index]
                        != visit_generation
                    {
                        self.static_detachment_visit_stamps[static_detachment_visit_index] =
                            visit_generation;
                        queue.push(neighbor);
                    }
                }
            }
            if !anchored && component.len() <= RIGID_DETACHMENT_MAXIMUM_CELLS {
                candidates.push(component);
            }
        }
        #[cfg(debug_assertions)]
        tracing::trace!(
            target: "engine_physics::static_detachment",
            changed_bits,
            seed_count = seeds.len(),
            visited_cells,
            candidate_components = candidates.len(),
            candidate_cells = candidates.iter().map(Vec::len).sum::<usize>(),
            "delta-seeded static detachment"
        );
        let newly_discovered_cells: HashSet<CellCoordinates> =
            candidates.iter().flatten().copied().collect();
        let mut desired_components: Vec<Vec<CellCoordinates>> = self
            .pending_static_detachment
            .take()
            .map(|pending| {
                pending
                    .components
                    .into_iter()
                    .filter(|component| {
                        component.iter().all(|cell| {
                            snapshot.is_static_cell_occupied(cell.x, cell.y) == Some(true)
                                && !newly_discovered_cells.contains(cell)
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        desired_components.extend(candidates);
        if desired_components.is_empty() {
            self.rigid_detachment_snapshot = Some(snapshot.clone());
            self.submit_pending_static_detachment();
            return Ok(());
        }
        let mut indices: Vec<u32> = Vec::new();
        let mut valid_components: Vec<Vec<CellCoordinates>> = Vec::new();
        for component in desired_components {
            let Some(component_indices) = component
                .iter()
                .map(|coordinates| self.cell_edit_index(*coordinates).map(|index| index as u32))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            indices.extend(component_indices);
            valid_components.push(component);
        }
        self.static_detachment_generation = self.static_detachment_generation.wrapping_add(1);
        self.pending_static_detachment = Some(ScenePendingStaticDetachment {
            components: valid_components,
            indices,
            generation: self.static_detachment_generation,
            ring_offset: (self.tiles_ring_offset_x, self.tiles_ring_offset_y),
        });
        self.submit_pending_static_detachment();
        self.rigid_detachment_snapshot = Some(snapshot.clone());
        Ok(())
    }

    pub(super) fn rigid_cell_debris_placement(
        &self,
        state: &RigidCellularBodyState,
        cell: &RigidCellularBodyCell,
    ) -> Option<SceneEditCellPlacement> {
        let Some(Material::CellularStatic {
            debris_material: Some(material_identifier),
            debris_yield_rate,
            ..
        }) = self.data.materials().get(cell.material)
        else {
            return None;
        };
        let seed: u32 = cell
            .state_slot
            .wrapping_mul(747_796_405)
            .wrapping_add(2_891_336_453);
        if (seed % 10_000) as f32 >= debris_yield_rate * 10_000.0 {
            return None;
        }
        let local: [f32; 2] = [
            (cell.local[0] as f32 + 0.5) / 8.0,
            (cell.local[1] as f32 + 0.5) / 8.0,
        ];
        let world: [f32; 2] = [
            state.translation[0] + state.angle.cos() * local[0] - state.angle.sin() * local[1],
            state.translation[1] + state.angle.sin() * local[0] + state.angle.cos() * local[1],
        ];
        Some(SceneEditCellPlacement {
            coordinates: CellCoordinates {
                x: (world[0] * 8.0).floor() as i32,
                y: (world[1] * 8.0).floor() as i32,
            },
            material_identifier: *material_identifier,
            appearance: cell.appearance,
        })
    }
}
