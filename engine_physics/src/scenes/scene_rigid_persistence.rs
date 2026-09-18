// Copyright Rob Gage 2026

use std::io;
use std::sync::mpsc::SyncSender;

use super::RIGID_IO_MAX_IN_FLIGHT;
use super::Scene;
use super::SceneRigidBodyStreamingResponse;
use super::SceneRigidIoJob;
use super::SceneRigidOwnerLoad;
use super::SceneRigidPersistenceRequest;
use crate::scenes::SceneData;
use crate::scenes::SceneDormantRigidBody;
use crate::simulation::RigidCellularBody;
use crate::simulation::RigidCellularBodyCell;
use crate::tiles::TileArea;
use crate::tiles::TileCoordinates;

impl Scene {
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
            let job: SceneRigidIoJob = if let Some(owner) = self.rigid_owner_load_queue.pop_front()
            {
                self.rigid_owner_loads
                    .insert(owner, SceneRigidOwnerLoad::Loading);
                let generation: u64 = *self.rigid_owner_generation.entry(owner).or_default();
                let scene_data_store: SceneData = self.data.clone();
                let sender: SyncSender<SceneRigidBodyStreamingResponse> =
                    self.rigid_streaming_response_sender.clone();
                self.rigid_io_in_flight += 1;
                std::thread::spawn(move || {
                    sender
                        .send(SceneRigidBodyStreamingResponse::Loaded {
                            owner,
                            generation,
                            result: scene_data_store.read_dormant_rigids(owner),
                        })
                        .ok();
                });
                continue;
            } else if let Some(job) = self.rigid_persistence_queue.pop_front() {
                job
            } else {
                break;
            };
            let scene_data_store: SceneData = self.data.clone();
            let sender: SyncSender<SceneRigidBodyStreamingResponse> =
                self.rigid_streaming_response_sender.clone();
            self.rigid_io_in_flight += 1;
            std::thread::spawn(move || match job {
                SceneRigidIoJob::Persist(request) => {
                    let Some(owner) = crate::scenes::owner_chunk(
                        request.record.position,
                        request.record.rotation,
                        request.record.cells.iter().map(|cell| cell.local),
                    ) else {
                        sender
                            .send(SceneRigidBodyStreamingResponse::Saved {
                                request,
                                result: Err(io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    "invalid rigid geometry",
                                )),
                            })
                            .ok();
                        return;
                    };
                    let rigid_persistence_result: Result<(), io::Error> = scene_data_store
                        .read_dormant_rigids(owner)
                        .and_then(|mut records| {
                            crate::scenes::append_record(&mut records, request.record.clone())?;
                            scene_data_store.write_dormant_rigids(owner, &records)
                        });
                    sender
                        .send(SceneRigidBodyStreamingResponse::Saved {
                            request,
                            result: rigid_persistence_result,
                        })
                        .ok();
                }
                SceneRigidIoJob::Claim {
                    owner,
                    original,
                    restored_ids,
                } => {
                    let rigid_claim_result: Result<Vec<SceneDormantRigidBody>, io::Error> =
                        scene_data_store
                            .read_dormant_rigids(owner)
                            .and_then(|mut records| {
                                crate::scenes::remove_ids(&mut records, &restored_ids);
                                scene_data_store.write_dormant_rigids(owner, &records)?;
                                Ok(records)
                            });
                    sender
                        .send(SceneRigidBodyStreamingResponse::Claimed {
                            owner,
                            original,
                            restored_ids,
                            result: rigid_claim_result,
                        })
                        .ok();
                }
            });
        }
    }

    pub(super) fn rigid_streaming_apply_completed(&mut self) -> Result<(), io::Error> {
        while let Ok(response) = self.rigid_streaming_responses.try_recv() {
            self.rigid_io_in_flight = self.rigid_io_in_flight.saturating_sub(1);
            match response {
                SceneRigidBodyStreamingResponse::Loaded {
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
                                .insert(owner, SceneRigidOwnerLoad::Ready(records));
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
                SceneRigidBodyStreamingResponse::Loaded { owner, .. } => {
                    // a newer generation may already be loading. If not,
                    // stale completion must repair the desired-owner state.
                    if self.rigid_desired_owners.contains(&owner)
                        && !matches!(
                            self.rigid_owner_loads.get(&owner),
                            Some(SceneRigidOwnerLoad::Loading)
                                | Some(SceneRigidOwnerLoad::Ready(_))
                                | Some(SceneRigidOwnerLoad::Claiming)
                        )
                    {
                        self.rigid_owner_load(owner);
                    }
                }
                SceneRigidBodyStreamingResponse::Saved { request, result } => match result {
                    Ok(()) => {
                        let owner: TileCoordinates = crate::scenes::owner_chunk(
                            request.record.position,
                            request.record.rotation,
                            request.record.cells.iter().map(|cell| cell.local),
                        )
                        .ok_or_else(|| {
                            io::Error::new(io::ErrorKind::InvalidData, "invalid rigid geometry")
                        })?;
                        let rigid_owner_generation: &mut u64 =
                            self.rigid_owner_generation.entry(owner).or_default();
                        *rigid_owner_generation = rigid_owner_generation.wrapping_add(1);
                        let claiming: bool = matches!(
                            self.rigid_owner_loads.get(&owner),
                            Some(SceneRigidOwnerLoad::Claiming)
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
                SceneRigidBodyStreamingResponse::Claimed {
                    owner,
                    original,
                    restored_ids,
                    result,
                } => match result {
                    Ok(records) => {
                        self.rigid_owner_loads
                            .insert(owner, SceneRigidOwnerLoad::Ready(records));
                    }
                    Err(error) => {
                        self.rollback_rigid_restore(&restored_ids);
                        self.rigid_owner_loads
                            .insert(owner, SceneRigidOwnerLoad::Ready(original));
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
        request: SceneRigidPersistenceRequest,
    ) -> Result<(), io::Error> {
        let mut restored_rigid_body_cells: Vec<RigidCellularBodyCell> =
            Vec::with_capacity(request.record.cells.len());
        for (cell, rigid_cell_state_slot) in request.record.cells.iter().zip(request.slots) {
            restored_rigid_body_cells.push(RigidCellularBodyCell {
                local: cell.local,
                material: cell.material,
                appearance: cell.appearance,
                state_slot: rigid_cell_state_slot,
                state_generation: self.rigid_cell_state_generations[rigid_cell_state_slot as usize],
            });
        }
        let (friction, restitution) =
            self.rigid_cellular_material_response(&restored_rigid_body_cells);
        let mut restored_rigid_cellular_body: RigidCellularBody =
            self.physics_world.insert_rigid_cellular_body(
                request.record.position,
                request.record.rotation,
                self.data.materials(),
                restored_rigid_body_cells,
                friction,
                restitution,
                request.record.linear_velocity,
                request.record.angular_velocity,
            );
        restored_rigid_cellular_body.identifier = request.record.identifier;
        if request.record.sleeping {
            self.physics_world
                .sleep_rigid_cellular_body(&restored_rigid_cellular_body);
        }
        self.rigid_cellular_bodies
            .push(restored_rigid_cellular_body);
        self.rigid_cellular_topology_revision =
            self.rigid_cellular_topology_revision.wrapping_add(1);
        self.rigid_cellular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.rigid_granular_contact_active
            .resize(self.rigid_cellular_bodies.len(), false);
        self.debug_assert_rigid_resident_invariants();
        Ok(())
    }

    fn rollback_rigid_restore(&mut self, identifiers: &[u64]) {
        for identifier in identifiers {
            if let Some(index) = self
                .rigid_cellular_bodies
                .iter()
                .position(|body| body.identifier == *identifier)
            {
                let removed_rigid_cellular_body: RigidCellularBody =
                    self.rigid_cellular_bodies.swap_remove(index);
                self.rigid_activation_pending
                    .remove(&removed_rigid_cellular_body.identifier);
                self.rigid_sleeping_pending
                    .remove(&removed_rigid_cellular_body.identifier);
                self.physics_world
                    .remove_rigid_cellular_body(&removed_rigid_cellular_body);
                for cell in removed_rigid_cellular_body.cells {
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
        let owners: Vec<TileCoordinates> = self
            .rigid_owner_loads
            .iter()
            .filter_map(|(owner, state)| {
                matches!(state, SceneRigidOwnerLoad::Ready(_)).then_some(*owner)
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
        let Some(SceneRigidOwnerLoad::Ready(original)) = self.rigid_owner_loads.remove(&owner)
        else {
            return Ok(());
        };
        let buffered: TileArea = self.area_buffered();
        let (records, _retained): (Vec<SceneDormantRigidBody>, Vec<SceneDormantRigidBody>) =
            original.iter().cloned().partition(|record| {
                crate::scenes::world_aabb(
                    record.position,
                    record.rotation,
                    record.cells.iter().map(|cell| cell.local),
                )
                .is_some_and(|bounds| crate::scenes::intersects_area(bounds, buffered))
            });
        if records.is_empty() {
            self.rigid_owner_loads
                .insert(owner, SceneRigidOwnerLoad::Ready(original));
            return Ok(());
        }
        let required: usize = records.iter().map(|record| record.cells.len()).sum();
        if required > self.rigid_cell_state_free.len() {
            self.rigid_owner_loads
                .insert(owner, SceneRigidOwnerLoad::Ready(original));
            return Ok(());
        }
        for record in &records {
            record.validate(self.data.materials())?;
            if self
                .rigid_cellular_bodies
                .iter()
                .any(|body| body.identifier == record.identifier)
            {
                self.rigid_owner_loads
                    .insert(owner, SceneRigidOwnerLoad::Ready(original));
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "duplicate resident rigid identity",
                ));
            }
        }
        let mut uploaded: Vec<[u32; 4]> = Vec::with_capacity(required);
        let mut restored_rigid_body_identifiers: Vec<u64> = Vec::with_capacity(records.len());
        for record in &records {
            let mut restored_rigid_body_cells: Vec<RigidCellularBodyCell> =
                Vec::with_capacity(record.cells.len());
            for cell in &record.cells {
                let rigid_cell_state_slot: u32 =
                    self.rigid_cell_state_free.pop().expect("capacity checked");
                let rigid_cell_state_generation: u32 =
                    self.rigid_cell_state_generations[rigid_cell_state_slot as usize];
                restored_rigid_body_cells.push(RigidCellularBodyCell {
                    local: cell.local,
                    material: cell.material,
                    appearance: cell.appearance,
                    state_slot: rigid_cell_state_slot,
                    state_generation: rigid_cell_state_generation,
                });
                uploaded.push([
                    rigid_cell_state_slot,
                    cell.integrity.to_bits(),
                    cell.amount.to_bits(),
                    cell.temperature.to_bits(),
                ]);
            }
            let (friction, restitution) =
                self.rigid_cellular_material_response(&restored_rigid_body_cells);
            let mut restored_rigid_cellular_body: RigidCellularBody =
                self.physics_world.insert_rigid_cellular_body(
                    record.position,
                    record.rotation,
                    self.data.materials(),
                    restored_rigid_body_cells,
                    friction,
                    restitution,
                    record.linear_velocity,
                    record.angular_velocity,
                );
            restored_rigid_cellular_body.identifier = record.identifier;
            if record.sleeping {
                self.rigid_sleeping_pending
                    .insert(restored_rigid_cellular_body.identifier);
            }
            self.physics_world
                .set_rigid_cellular_body_enabled(&restored_rigid_cellular_body, false);
            self.rigid_activation_pending
                .insert(restored_rigid_cellular_body.identifier);
            restored_rigid_body_identifiers.push(restored_rigid_cellular_body.identifier);
            self.rigid_cellular_body_identifier_next = self
                .rigid_cellular_body_identifier_next
                .max(record.identifier.checked_add(1).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "rigid identity overflow")
                })?);
            self.rigid_cellular_bodies
                .push(restored_rigid_cellular_body);
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
            .insert(owner, SceneRigidOwnerLoad::Claiming);
        self.rigid_persistence_queue
            .push_back(SceneRigidIoJob::Claim {
                owner,
                original,
                restored_ids: restored_rigid_body_identifiers,
            });
        self.rigid_io_submit();
        self.debug_assert_rigid_resident_invariants();
        Ok(())
    }
}
