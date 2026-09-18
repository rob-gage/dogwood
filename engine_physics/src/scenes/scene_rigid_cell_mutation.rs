// Copyright Rob Gage 2026

use super::*;

impl Scene {
    /// Applies Accelerator-detected rigid phase candidates after their bounded async
    /// readback.  Every candidate is rechecked against authoritative body
    /// state before its already-reserved fluid-particle slot is committed.
    pub(super) fn apply_completed_rigid_thermal_transitions(&mut self) {
        let Some(candidates) = self.thermal_phase_transitions.take_rigid_candidates() else {
            return;
        };
        let mut removals: HashMap<usize, HashSet<[i32; 2]>> = HashMap::new();
        let mut rollback = Vec::new();
        for candidate in candidates {
            let slot = candidate[0];
            let generation = candidate[1];
            let expected = MaterialIdentifier::from_u32(candidate[2]);
            let replacement = candidate[3];
            if !matches!(
                self.data
                    .materials()
                    .get(MaterialIdentifier::from_u32(replacement)),
                Some(Material::Fluid { .. })
            ) {
                rollback.push(candidate[9]);
                continue;
            }
            let amount = f32::from_bits(candidate[4]);
            let temperature = f32::from_bits(candidate[5]);
            let reserved = candidate[9];
            let Some((body_index, cell)) =
                self.rigid_cellular_bodies
                    .iter()
                    .enumerate()
                    .find_map(|(index, body)| {
                        body.cells
                            .iter()
                            .find(|cell| {
                                cell.state_slot == slot
                                    && cell.state_generation == generation
                                    && cell.material == expected
                            })
                            .copied()
                            .map(|cell| (index, cell))
                    })
            else {
                rollback.push(reserved);
                continue;
            };
            if !(amount >= 0.999 && amount <= 1.001)
                || self
                    .rigid_cell_state_generations
                    .get(slot as usize)
                    .copied()
                    != Some(generation)
            {
                rollback.push(reserved);
                continue;
            }
            let Some(state) = self
                .physics_world
                .rigid_cellular_body_state(&self.rigid_cellular_bodies[body_index])
            else {
                rollback.push(reserved);
                continue;
            };
            let local = [
                (cell.local[0] as f32 + 0.5) / 8.0,
                (cell.local[1] as f32 + 0.5) / 8.0,
            ];
            let offset = [
                state.angle.cos() * local[0] - state.angle.sin() * local[1],
                state.angle.sin() * local[0] + state.angle.cos() * local[1],
            ];
            let position = [
                state.translation[0] + offset[0],
                state.translation[1] + offset[1],
            ];
            let velocity = [
                state.linear_velocity[0]
                    - state.angular_velocity * (position[1] - state.center_of_mass[1]),
                state.linear_velocity[1]
                    + state.angular_velocity * (position[0] - state.center_of_mass[0]),
            ];
            self.fluids.commit_reserved_particle(
                self.accelerator.as_ref(),
                reserved,
                replacement,
                position,
                velocity,
                amount,
                temperature,
            );
            removals.entry(body_index).or_default().insert(cell.local);
        }
        if !rollback.is_empty() {
            self.thermal_phase_transitions
                .rollback_rigid_reservations(self.accelerator.as_ref(), &rollback);
        }
        let mut removals: Vec<_> = removals.into_iter().collect();
        removals.sort_unstable_by_key(|(body_index, _)| std::cmp::Reverse(*body_index));
        for (body_index, removed) in removals {
            self.remove_rigid_cellular_body_cells(
                body_index,
                &removed,
                RigidCellRemovalCause::PhaseTransition,
            );
        }
    }

    /// Removes body-local cells and replaces the body with its remaining connected pieces
    pub(super) fn remove_rigid_cellular_body_cells(
        &mut self,
        body_index: usize,
        removed: &HashSet<[i32; 2]>,
        cause: RigidCellRemovalCause,
    ) {
        if body_index >= self.rigid_cellular_bodies.len() || removed.is_empty() {
            return;
        }
        let Some(state) = self
            .physics_world
            .rigid_cellular_body_state(&self.rigid_cellular_bodies[body_index])
        else {
            return;
        };
        let body = self.rigid_cellular_bodies.swap_remove(body_index);
        self.rigid_activation_pending.remove(&body.id);
        self.rigid_sleeping_pending.remove(&body.id);
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_support.clear();
        self.rigid_cellular_recovery.clear();
        self.rigid_cellular_contact_active.clear();
        self.rigid_granular_contact_active.clear();
        self.physics_world.remove_rigid_cellular_body(&body);
        let mut debris = Vec::new();
        for cell in body
            .cells
            .iter()
            .filter(|cell| removed.contains(&cell.local))
        {
            if cause == RigidCellRemovalCause::Fracture
                && let Some(placement) = self.rigid_cell_debris_placement(&state, cell)
            {
                debris.push(placement);
            }
            self.release_rigid_cell_state(cell.state_slot);
        }
        let remaining = body
            .cells
            .into_iter()
            .filter(|cell| !removed.contains(&cell.local))
            .collect();
        for cells in RigidCellularBody::connected_components(remaining) {
            if cells.is_empty() {
                continue;
            }
            if cells.len() < self.rigid_component_minimum(&cells) {
                for cell in cells {
                    if let Some(placement) = self.rigid_cell_debris_placement(&state, &cell) {
                        debris.push(placement);
                    }
                    self.release_rigid_cell_state(cell.state_slot);
                }
                continue;
            }
            let local_center =
                RigidCellularBody::mass_properties(&cells, self.data.materials()).local_com;
            let child_center = [
                state.translation[0] + state.angle.cos() * local_center.x
                    - state.angle.sin() * local_center.y,
                state.translation[1]
                    + state.angle.sin() * local_center.x
                    + state.angle.cos() * local_center.y,
            ];
            let offset = [
                child_center[0] - state.center_of_mass[0],
                child_center[1] - state.center_of_mass[1],
            ];
            let child_velocity = [
                state.linear_velocity[0] - state.angular_velocity * offset[1],
                state.linear_velocity[1] + state.angular_velocity * offset[0],
            ];
            let (rebased_position, cells) =
                Self::rebase_rigid_cells(cells, state.translation, state.angle);
            let (friction, restitution) = self.rigid_cellular_material_response(&cells);
            let mut child = self.physics_world.insert_rigid_cellular_body(
                rebased_position,
                state.angle,
                self.data.materials(),
                cells,
                friction,
                restitution,
                child_velocity,
                state.angular_velocity,
            );
            child.id = self.next_rigid_cellular_body_id();
            self.rigid_cellular_bodies.push(child);
        }
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.debug_assert_rigid_resident_invariants();
        if !debris.is_empty() {
            self.pending_runtime_edits.place_cells(debris);
        }
    }

    /// Rebase a split component while preserving every cell's world position.
    fn rebase_rigid_cells(
        mut cells: Vec<RigidCellularBodyCell>,
        position: [f32; 2],
        angle: f32,
    ) -> ([f32; 2], Vec<RigidCellularBodyCell>) {
        let offset = cells
            .iter()
            .map(|cell| cell.local)
            .fold([i32::MAX, i32::MAX], |[min_x, min_y], [x, y]| {
                [min_x.min(x), min_y.min(y)]
            });
        for cell in &mut cells {
            cell.local[0] -= offset[0];
            cell.local[1] -= offset[1];
        }
        let (sin, cos) = angle.sin_cos();
        (
            [
                position[0] + (cos * offset[0] as f32 - sin * offset[1] as f32) / 8.0,
                position[1] + (sin * offset[0] as f32 + cos * offset[1] as f32) / 8.0,
            ],
            cells,
        )
    }

    /// Averages the existing static material response for one concrete body
    pub(super) fn rigid_cellular_material_response(
        &self,
        cells: &[RigidCellularBodyCell],
    ) -> (f32, f32) {
        let (friction, restitution) = cells.iter().fold((0.0, 0.0), |sum, cell| {
            match self.data.materials().get(cell.material) {
                Some(Material::CellularStatic {
                    friction,
                    restitution,
                    ..
                }) => (sum.0 + friction, sum.1 + restitution),
                _ => sum,
            }
        });
        let divisor = cells.len().max(1) as f32;
        (friction / divisor, restitution / divisor)
    }

    /// The strictest material in a mixed component controls its minimum size.
    pub(super) fn rigid_component_minimum(&self, cells: &[RigidCellularBodyCell]) -> usize {
        cells
            .iter()
            .filter_map(|cell| match self.data.materials().get(cell.material) {
                Some(Material::CellularStatic {
                    minimum_rigid_body_cell_count,
                    ..
                }) => Some(*minimum_rigid_body_cell_count as usize),
                _ => None,
            })
            .max()
            .unwrap_or(1)
    }

    pub(super) fn release_rigid_cell_state(&mut self, slot: u32) {
        let generation = &mut self.rigid_cell_state_generations[slot as usize];
        *generation = generation.wrapping_add(1);
        self.rigid_cell_state_free.push(slot);
    }

    #[cfg(debug_assertions)]
    pub(super) fn debug_assert_rigid_resident_invariants(&self) {
        debug_assert_eq!(
            self.rigid_cellular_contact_active.len(),
            self.rigid_cellular_bodies.len(),
            "rigid contact sidecar must stay positional with resident bodies"
        );
        debug_assert_eq!(
            self.rigid_granular_contact_active.len(),
            self.rigid_cellular_bodies.len(),
            "rigid granular sidecar must stay positional with resident bodies"
        );
        let mut ids = HashSet::with_capacity(self.rigid_cellular_bodies.len());
        let mut slots = HashSet::new();
        for body in &self.rigid_cellular_bodies {
            debug_assert!(
                body.id != 0,
                "resident rigid body has no persistent identity"
            );
            debug_assert!(
                ids.insert(body.id),
                "resident rigid identities must be unique"
            );
            for cell in &body.cells {
                debug_assert!((cell.state_slot as usize) < self.rigid_cell_state_generations.len());
                debug_assert_eq!(
                    self.rigid_cell_state_generations[cell.state_slot as usize],
                    cell.state_generation,
                    "resident rigid cell owns a stale state generation"
                );
                debug_assert!(
                    slots.insert(cell.state_slot),
                    "rigid state slot is owned twice"
                );
            }
        }
    }

    #[cfg(not(debug_assertions))]
    #[inline(always)]
    pub(super) fn debug_assert_rigid_resident_invariants(&self) {}
}
