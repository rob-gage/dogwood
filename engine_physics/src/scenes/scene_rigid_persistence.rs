// Copyright Rob Gage 2026

use super::*;

impl Scene {
    /// Freezes outgoing bodies while current support is still valid, then
    /// gathers every selected cell through one compact Accelerator batch.
    pub(super) fn rigid_dormancy_begin(
        &mut self,
        future_buffered: TileArea,
    ) -> Result<bool, io::Error> {
        let current_buffered = self.area_buffered();
        let mut selected = Vec::new();
        let mut state_count = 0usize;
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
                        id: body.id,
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
        let Some(readback_slot) = self.rigid_dormancy_readback_free.pop() else {
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
        let (sender, result) = sync_channel(1);
        self.rigid_dormancy_readbacks[readback_slot]
            .slice(0..state_count as u64 * 16)
            .map_async(wgpu::MapMode::Read, move |outcome| {
                let _ = sender.send(outcome);
            });
        let mut bodies: Vec<_> = selected
            .into_iter()
            .rev()
            .map(|(index, pending)| {
                let body = self.rigid_cellular_bodies.swap_remove(index);
                self.rigid_activation_pending.remove(&body.id);
                self.rigid_sleeping_pending.remove(&body.id);
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
        let mut index = 0;
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
                .chunks_exact(16)
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
            let mut cursor = 0;
            let mut bodies = batch.bodies.into_iter();
            let mut records = Vec::new();
            while let Some(body) = bodies.next() {
                let end = cursor + body.cells.len();
                let record = crate::scenes::DormantRigidBody {
                    id: body.id,
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
            let mut body = self.physics_world.insert_rigid_cellular_body(
                pending.position,
                pending.rotation,
                self.data.materials(),
                pending.cells,
                friction,
                restitution,
                pending.linear_velocity,
                pending.angular_velocity,
            );
            body.id = pending.id;
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

    pub(super) fn rigid_owner_load(&mut self, owner: TileCoordinates) {
        if self.rigid_owner_loads.contains_key(&owner)
            || self.rigid_owner_load_queue.contains(&owner)
        {
            return;
        }
        self.rigid_owner_load_queue.push_back(owner);
        self.rigid_io_submit();
    }

    /// Starts at most two owner-file operations.  This is deliberately a
    /// bounded worker frontier: disk latency delays an area shift, never a frame.
    pub(super) fn rigid_io_submit(&mut self) {
        while self.rigid_io_in_flight < RIGID_IO_MAX_IN_FLIGHT {
            let job = if let Some(owner) = self.rigid_owner_load_queue.pop_front() {
                self.rigid_owner_loads
                    .insert(owner, RigidOwnerLoad::Loading);
                let generation = *self.rigid_owner_generation.entry(owner).or_default();
                let data = self.data.clone();
                let sender = self.rigid_streaming_response_sender.clone();
                self.rigid_io_in_flight += 1;
                std::thread::spawn(move || {
                    let _ = sender.send(RigidStreamingResponse::Loaded {
                        owner,
                        generation,
                        result: data.read_dormant_rigids(owner),
                    });
                });
                continue;
            } else if let Some(job) = self.rigid_persistence_queue.pop_front() {
                job
            } else {
                break;
            };
            let data = self.data.clone();
            let sender = self.rigid_streaming_response_sender.clone();
            self.rigid_io_in_flight += 1;
            std::thread::spawn(move || match job {
                RigidIoJob::Persist(request) => {
                    let Some(owner) = crate::scenes::owner_chunk(
                        request.record.position,
                        request.record.rotation,
                        request.record.cells.iter().map(|cell| cell.local),
                    ) else {
                        let _ = sender.send(RigidStreamingResponse::Saved {
                            request,
                            result: Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "invalid rigid geometry",
                            )),
                        });
                        return;
                    };
                    let result = data.read_dormant_rigids(owner).and_then(|mut records| {
                        crate::scenes::append_record(&mut records, request.record.clone())?;
                        data.write_dormant_rigids(owner, &records)
                    });
                    let _ = sender.send(RigidStreamingResponse::Saved { request, result });
                }
                RigidIoJob::Claim {
                    owner,
                    original,
                    restored_ids,
                } => {
                    let result = data.read_dormant_rigids(owner).and_then(|mut records| {
                        crate::scenes::remove_ids(&mut records, &restored_ids);
                        data.write_dormant_rigids(owner, &records)?;
                        Ok(records)
                    });
                    let _ = sender.send(RigidStreamingResponse::Claimed {
                        owner,
                        original,
                        restored_ids,
                        result,
                    });
                }
            });
        }
    }

    pub(super) fn rigid_streaming_apply_completed(&mut self) -> Result<(), io::Error> {
        while let Ok(response) = self.rigid_streaming_responses.try_recv() {
            self.rigid_io_in_flight = self.rigid_io_in_flight.saturating_sub(1);
            match response {
                RigidStreamingResponse::Loaded {
                    owner,
                    generation,
                    result,
                } if self
                    .rigid_owner_generation
                    .get(&owner)
                    .copied()
                    .unwrap_or_default()
                    == generation =>
                {
                    match result {
                        Ok(records) => {
                            self.rigid_owner_loads
                                .insert(owner, RigidOwnerLoad::Ready(records));
                        }
                        Err(error) => {
                            self.rigid_owner_loads.remove(&owner);
                            tracing::warn!(
                                owner_x = owner.x,
                                owner_y = owner.y,
                                "dormant rigid load failed: {error}"
                            );
                        }
                    }
                }
                RigidStreamingResponse::Loaded { owner, .. } => {
                    // A newer generation may already be loading. If not,
                    // stale completion must repair the desired-owner state.
                    if self.rigid_desired_owners.contains(&owner)
                        && !matches!(
                            self.rigid_owner_loads.get(&owner),
                            Some(RigidOwnerLoad::Loading)
                                | Some(RigidOwnerLoad::Ready(_))
                                | Some(RigidOwnerLoad::Claiming)
                        )
                    {
                        self.rigid_owner_load(owner);
                    }
                }
                RigidStreamingResponse::Saved { request, result } => match result {
                    Ok(()) => {
                        let owner = crate::scenes::owner_chunk(
                            request.record.position,
                            request.record.rotation,
                            request.record.cells.iter().map(|cell| cell.local),
                        )
                        .ok_or_else(|| {
                            io::Error::new(io::ErrorKind::InvalidData, "invalid rigid geometry")
                        })?;
                        let generation = self.rigid_owner_generation.entry(owner).or_default();
                        *generation = generation.wrapping_add(1);
                        let claiming = matches!(
                            self.rigid_owner_loads.get(&owner),
                            Some(RigidOwnerLoad::Claiming)
                        );
                        if !claiming {
                            self.rigid_owner_loads.remove(&owner);
                        }
                        for slot in request.slots {
                            self.release_rigid_cell_state(slot);
                        }
                        if !claiming && self.rigid_desired_owners.contains(&owner) {
                            self.rigid_owner_load(owner);
                        }
                    }
                    Err(error) => {
                        self.restore_failed_rigid_persistence(request)?;
                        return Err(error);
                    }
                },
                RigidStreamingResponse::Claimed {
                    owner,
                    original,
                    restored_ids,
                    result,
                } => match result {
                    Ok(records) => {
                        self.rigid_owner_loads
                            .insert(owner, RigidOwnerLoad::Ready(records));
                    }
                    Err(error) => {
                        self.rollback_rigid_restore(&restored_ids);
                        self.rigid_owner_loads
                            .insert(owner, RigidOwnerLoad::Ready(original));
                        if self.rigid_desired_owners.contains(&owner) {
                            self.rigid_owner_loads.remove(&owner);
                            self.rigid_owner_generation
                                .entry(owner)
                                .and_modify(|generation| *generation = generation.wrapping_add(1));
                            self.rigid_owner_load(owner);
                        }
                        return Err(error);
                    }
                },
            }
        }
        self.rigid_io_submit();
        Ok(())
    }

    fn restore_failed_rigid_persistence(
        &mut self,
        request: RigidPersistenceRequest,
    ) -> Result<(), io::Error> {
        let mut cells = Vec::with_capacity(request.record.cells.len());
        for (cell, slot) in request.record.cells.iter().zip(request.slots) {
            cells.push(RigidCellularBodyCell {
                local: cell.local,
                material: cell.material,
                appearance: cell.appearance,
                state_slot: slot,
                state_generation: self.rigid_cell_state_generations[slot as usize],
            });
        }
        let (friction, restitution) = self.rigid_cellular_material_response(&cells);
        let mut body = self.physics_world.insert_rigid_cellular_body(
            request.record.position,
            request.record.rotation,
            self.data.materials(),
            cells,
            friction,
            restitution,
            request.record.linear_velocity,
            request.record.angular_velocity,
        );
        body.id = request.record.id;
        if request.record.sleeping {
            self.physics_world.sleep_rigid_cellular_body(&body);
        }
        self.rigid_cellular_bodies.push(body);
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.debug_assert_rigid_resident_invariants();
        Ok(())
    }

    fn rollback_rigid_restore(&mut self, ids: &[u64]) {
        for id in ids {
            if let Some(index) = self
                .rigid_cellular_bodies
                .iter()
                .position(|body| body.id == *id)
            {
                let body = self.rigid_cellular_bodies.swap_remove(index);
                self.rigid_activation_pending.remove(&body.id);
                self.rigid_sleeping_pending.remove(&body.id);
                self.physics_world.remove_rigid_cellular_body(&body);
                for cell in body.cells {
                    self.release_rigid_cell_state(cell.state_slot);
                }
            }
        }
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
    }

    /// Runs after incoming cellular/fluid/gas uploads have been submitted and
    /// before `update` can enter a fixed simulation tick.
    pub(super) fn restore_ready_rigids(&mut self) -> Result<(), io::Error> {
        let owners: Vec<_> = self
            .rigid_owner_loads
            .iter()
            .filter_map(|(owner, state)| {
                matches!(state, RigidOwnerLoad::Ready(_)).then_some(*owner)
            })
            .collect();
        for owner in owners {
            self.restore_dormant_rigids(owner)?;
        }
        Ok(())
    }

    /// Claims loaded records only after terrain uploads were submitted.  The
    /// owner file remains authoritative until the background claim completes.
    fn restore_dormant_rigids(&mut self, owner: TileCoordinates) -> Result<(), io::Error> {
        let Some(RigidOwnerLoad::Ready(original)) = self.rigid_owner_loads.remove(&owner) else {
            return Ok(());
        };
        let buffered = self.area_buffered();
        let (records, _retained): (Vec<_>, Vec<_>) = original.iter().cloned().partition(|record| {
            crate::scenes::world_aabb(
                record.position,
                record.rotation,
                record.cells.iter().map(|cell| cell.local),
            )
            .is_some_and(|bounds| crate::scenes::intersects_area(bounds, buffered))
        });
        if records.is_empty() {
            self.rigid_owner_loads
                .insert(owner, RigidOwnerLoad::Ready(original));
            return Ok(());
        }
        let required: usize = records.iter().map(|record| record.cells.len()).sum();
        if required > self.rigid_cell_state_free.len() {
            self.rigid_owner_loads
                .insert(owner, RigidOwnerLoad::Ready(original));
            return Ok(());
        }
        for record in &records {
            record.validate(self.data.materials())?;
            if self
                .rigid_cellular_bodies
                .iter()
                .any(|body| body.id == record.id)
            {
                self.rigid_owner_loads
                    .insert(owner, RigidOwnerLoad::Ready(original));
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "duplicate resident rigid identity",
                ));
            }
        }
        let mut uploaded = Vec::with_capacity(required);
        let mut ids = Vec::with_capacity(records.len());
        for record in &records {
            let mut cells = Vec::with_capacity(record.cells.len());
            for cell in &record.cells {
                let slot = self.rigid_cell_state_free.pop().expect("capacity checked");
                let generation = self.rigid_cell_state_generations[slot as usize];
                cells.push(RigidCellularBodyCell {
                    local: cell.local,
                    material: cell.material,
                    appearance: cell.appearance,
                    state_slot: slot,
                    state_generation: generation,
                });
                uploaded.push([
                    slot,
                    cell.integrity.to_bits(),
                    cell.amount.to_bits(),
                    cell.temperature.to_bits(),
                ]);
            }
            let (friction, restitution) = self.rigid_cellular_material_response(&cells);
            let mut body = self.physics_world.insert_rigid_cellular_body(
                record.position,
                record.rotation,
                self.data.materials(),
                cells,
                friction,
                restitution,
                record.linear_velocity,
                record.angular_velocity,
            );
            body.id = record.id;
            if record.sleeping {
                self.rigid_sleeping_pending.insert(body.id);
            }
            self.physics_world
                .set_rigid_cellular_body_enabled(&body, false);
            self.rigid_activation_pending.insert(body.id);
            ids.push(body.id);
            self.rigid_cellular_body_id_next =
                self.rigid_cellular_body_id_next
                    .max(record.id.checked_add(1).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "rigid identity overflow")
                    })?);
            self.rigid_cellular_bodies.push(body);
        }
        self.rigid_cell_state_upload
            .apply(self.accelerator.as_ref(), &uploaded);
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_owner_loads
            .insert(owner, RigidOwnerLoad::Claiming);
        self.rigid_persistence_queue.push_back(RigidIoJob::Claim {
            owner,
            original,
            restored_ids: ids,
        });
        self.rigid_io_submit();
        self.debug_assert_rigid_resident_invariants();
        Ok(())
    }
}
