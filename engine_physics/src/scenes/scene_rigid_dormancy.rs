// Copyright Rob Gage 2026

use super::*;
use crate::scenes::DormantRigidBody;

impl Scene {
    /// Freezes outgoing bodies while current support is still valid, then
    /// gathers every selected cell through one compact Accelerator batch.
    pub(super) fn rigid_dormancy_begin(
        &mut self,
        future_buffered: TileArea,
    ) -> Result<bool, io::Error> {
        let current_buffered: TileArea = self.area_buffered();
        let mut selected: Vec<(usize, PendingRigidDormancy)> = Vec::new();
        let mut state_count: usize = 0;
        for (index, body) in self.rigid_cellular_bodies.iter().enumerate() {
            let Some(state) = self.physics_world.rigid_cellular_body_state(body) else {
                continue;
            };
            let Some(bounds) = crate::scenes::world_aabb(
                state.translation,
                state.angle,
                body.cells.iter().map(|cell| cell.local),
            ) else {
                continue;
            };
            if !body.cells.is_empty()
                && crate::scenes::intersects_area(bounds, current_buffered)
                && !crate::scenes::intersects_area(bounds, future_buffered)
            {
                state_count += body.cells.len();
                selected.push((
                    index,
                    PendingRigidDormancy {
                        identifier: body.identifier,
                        position: state.translation,
                        rotation: state.angle,
                        linear_velocity: state.linear_velocity,
                        angular_velocity: state.angular_velocity,
                        sleeping: state.sleeping,
                        cells: body.cells.clone(),
                    },
                ));
            }
        }
        if selected.is_empty() {
            return Ok(true);
        }
        if self.rigid_persistence_queue.len() + self.rigid_io_in_flight + selected.len()
            > RIGID_IO_QUEUE_CAPACITY
        {
            return Ok(false);
        }
        let Some(readback_slot): Option<usize> = self.rigid_dormancy_readback_free.pop() else {
            return Ok(false);
        };
        let slots: Vec<u32> = selected
            .iter()
            .flat_map(|(_, body)| body.cells.iter().map(|cell| cell.state_slot))
            .collect();
        self.rigid_cell_state_gather.submit(
            self.accelerator.as_ref(),
            &slots,
            &self.rigid_dormancy_readbacks[readback_slot],
        );
        let (sender, result) = sync_channel::<Result<(), wgpu::BufferAsyncError>>(1);
        self.rigid_dormancy_readbacks[readback_slot]
            .slice(0..state_count as u64 * 16)
            .map_async(wgpu::MapMode::Read, move |outcome| {
                let _ = sender.send(outcome);
            });
        let mut bodies: Vec<PendingRigidDormancy> = selected
            .into_iter()
            .rev()
            .map(|(index, pending)| {
                let body = self.rigid_cellular_bodies.swap_remove(index);
                self.rigid_activation_pending.remove(&body.identifier);
                self.rigid_sleeping_pending.remove(&body.identifier);
                self.physics_world.remove_rigid_cellular_body(&body);
                pending
            })
            .collect();
        // Slots were packed in ascending resident order; restore that output
        // order after the descending swap-removes kept indices stable.
        bodies.reverse();
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_dormancy_batches.push(RigidDormancyBatch {
            bodies,
            readback_slot,
            state_count,
            result,
        });
        self.debug_assert_rigid_resident_invariants();
        Ok(true)
    }

    pub(super) fn rigid_dormancy_apply_completed(&mut self) -> Result<(), io::Error> {
        let mut index: usize = 0;
        while index < self.rigid_dormancy_batches.len() {
            match self.rigid_dormancy_batches[index].result.try_recv() {
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    index += 1;
                    continue;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let batch = self.rigid_dormancy_batches.swap_remove(index);
                    self.rigid_dormancy_readbacks[batch.readback_slot].unmap();
                    self.rigid_dormancy_readback_free.push(batch.readback_slot);
                    self.restore_aborted_rigid_dormancy(batch.bodies);
                    return Err(io::Error::other("rigid dormancy readback disconnected"));
                }
                Ok(Err(error)) => {
                    let batch = self.rigid_dormancy_batches.swap_remove(index);
                    self.rigid_dormancy_readbacks[batch.readback_slot].unmap();
                    self.rigid_dormancy_readback_free.push(batch.readback_slot);
                    self.restore_aborted_rigid_dormancy(batch.bodies);
                    return Err(io::Error::other(format!(
                        "rigid dormancy readback failed: {error}"
                    )));
                }
                Ok(Ok(())) => {}
            }
            let batch = self.rigid_dormancy_batches.swap_remove(index);
            let bytes = match self.rigid_dormancy_readbacks[batch.readback_slot]
                .slice(0..batch.state_count as u64 * 16)
                .get_mapped_range()
            {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.rigid_dormancy_readbacks[batch.readback_slot].unmap();
                    self.rigid_dormancy_readback_free.push(batch.readback_slot);
                    self.restore_aborted_rigid_dormancy(batch.bodies);
                    return Err(io::Error::other(format!(
                        "rigid dormancy map failed: {error}"
                    )));
                }
            };
            let states: Vec<[f32; 3]> = bytes
                .as_chunks::<16>()
                .0
                .iter()
                .map(|b| {
                    [
                        f32::from_bits(u32::from_le_bytes(b[0..4].try_into().unwrap())),
                        f32::from_bits(u32::from_le_bytes(b[4..8].try_into().unwrap())),
                        f32::from_bits(u32::from_le_bytes(b[8..12].try_into().unwrap())),
                    ]
                })
                .collect();
            debug_assert_eq!(
                states.len(),
                batch
                    .bodies
                    .iter()
                    .map(|body| body.cells.len())
                    .sum::<usize>()
            );
            drop(bytes);
            self.rigid_dormancy_readbacks[batch.readback_slot].unmap();
            self.rigid_dormancy_readback_free.push(batch.readback_slot);
            let mut cursor: usize = 0;
            let mut bodies = batch.bodies.into_iter();
            let mut records: Vec<(PendingRigidDormancy, DormantRigidBody)> = Vec::new();
            while let Some(body) = bodies.next() {
                let end: usize = cursor + body.cells.len();
                let record: DormantRigidBody = crate::scenes::DormantRigidBody {
                    identifier: body.identifier,
                    position: body.position,
                    rotation: body.rotation,
                    linear_velocity: body.linear_velocity,
                    angular_velocity: body.angular_velocity,
                    sleeping: body.sleeping,
                    cells: body
                        .cells
                        .iter()
                        .zip(&states[cursor..end])
                        .map(|(cell, state)| crate::scenes::DormantRigidCell {
                            local: cell.local,
                            material: cell.material,
                            appearance: cell.appearance,
                            integrity: state[0],
                            amount: state[1],
                            temperature: state[2],
                        })
                        .collect(),
                };
                cursor = end;
                if let Err(error) = record.validate(self.data.materials()) {
                    let mut pending = records
                        .into_iter()
                        .map(|(body, _)| body)
                        .collect::<Vec<_>>();
                    pending.push(body);
                    pending.extend(bodies);
                    self.restore_aborted_rigid_dormancy(pending);
                    return Err(error);
                }
                records.push((body, record));
            }
            for (body, record) in records {
                self.rigid_persistence_queue.push_back(RigidIoJob::Persist(
                    RigidPersistenceRequest {
                        slots: body.cells.iter().map(|cell| cell.state_slot).collect(),
                        record,
                    },
                ));
            }
            debug_assert_eq!(cursor, states.len());
            self.rigid_io_submit();
        }
        Ok(())
    }

    /// A failed map leaves the live Accelerator slots untouched, so reinserting the
    /// frozen CPU snapshot restores the only authoritative resident body.
    fn restore_aborted_rigid_dormancy(&mut self, pending: Vec<PendingRigidDormancy>) {
        for pending in pending {
            let (friction, restitution) = self.rigid_cellular_material_response(&pending.cells);
            let mut body: RigidCellularBody = self.physics_world.insert_rigid_cellular_body(
                pending.position,
                pending.rotation,
                self.data.materials(),
                pending.cells,
                friction,
                restitution,
                pending.linear_velocity,
                pending.angular_velocity,
            );
            body.identifier = pending.identifier;
            if pending.sleeping {
                self.physics_world.sleep_rigid_cellular_body(&body);
            }
            self.rigid_cellular_bodies.push(body);
        }
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.debug_assert_rigid_resident_invariants();
    }
}
