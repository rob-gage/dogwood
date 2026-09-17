// Copyright Rob Gage 2026

use super::*;

impl CellularPressure {
    /// Advances pressure propagation and rigid-cell coupling for one fixed tick
    pub fn simulate(
        &mut self,
        accelerator: &Accelerator,
        origin: TileCoordinates,
        width: u16,
        height: u16,
        ring_x: u16,
        ring_y: u16,
        delta_time: f32,
        gravity: [f32; 2],
        rigid_body_count: usize,
        rigid_cell_count: usize,
        rigid_topology_revision: u64,
    ) -> Result<(), io::Error> {
        self.ensure_rigid_body_capacity(accelerator, rigid_body_count);
        let rigid_body_count: u32 = rigid_body_count
            .try_into()
            .map_err(|_| io::Error::other("Rigid body count exceeds Accelerator indexing range"))?;
        let rigid_cell_count: u32 = rigid_cell_count
            .try_into()
            .map_err(|_| io::Error::other("Rigid cell count exceeds Accelerator indexing range"))?;
        self.write_parameters(
            accelerator,
            origin,
            width,
            height,
            ring_x,
            ring_y,
            CellCoordinates { x: 0, y: 0 },
            0.0,
            0.0,
            delta_time,
            gravity,
            rigid_body_count,
            rigid_cell_count,
            CellCoordinates { x: 0, y: 0 },
            [0; 2],
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("cellular pressure simulation"),
                });
        if self.rigid_topology_revision != rigid_topology_revision {
            encoder.clear_buffer(self.rigid_reactions.wgpu_buffer(), 0, None);
            self.rigid_reaction_completed
                .retain(|_, batch| batch.topology_revision == rigid_topology_revision);
            self.rigid_reaction_sequence_apply_next = self.rigid_reaction_sequence_next;
            self.rigid_topology_revision = rigid_topology_revision;
        }
        encoder.clear_buffer(self.rigid_fractures.wgpu_buffer(), 0, None);
        encoder.clear_buffer(&self.rigid_fracture_count, 0, None);
        encoder.clear_buffer(&self.rigid_damage_dispatch, 0, None);
        let tile_count: u32 = self.buffered_cell_count / 64;
        for (pipeline, label) in [
            (
                &self.clear_active_tiles_pipeline,
                "clear active cellular pressure tiles",
            ),
            (
                &self.mark_active_tiles_pipeline,
                "mark active cellular pressure tiles",
            ),
        ] {
            let mut pass: wgpu::ComputePass<'_> =
                accelerator.begin_compute_pass(&mut encoder, label);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            let workgroups: u32 = if label == "mark active cellular pressure tiles" {
                tile_count
            } else {
                tile_count.div_ceil(64)
            };
            pass.dispatch_workgroups(workgroups, 1, 1);
        }
        encoder.clear_buffer(&self.indirect_dispatch, 0, None);
        {
            let mut pass: wgpu::ComputePass<'_> = accelerator
                .begin_compute_pass(&mut encoder, "compact active cellular pressure tiles");
            pass.set_pipeline(&self.compact_active_tiles_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_bind_group(1, &self.indirect_bind_group, &[]);
            pass.dispatch_workgroups(tile_count.div_ceil(64), 1, 1);
        }
        {
            let mut pass: wgpu::ComputePass<'_> =
                accelerator.begin_compute_pass(&mut encoder, "initialize rigid contact state");
            pass.set_pipeline(&self.rigid_contact_initialize_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(rigid_body_count.max(1).div_ceil(64), 1, 1);
        }
        for (pipeline, label) in [
            (
                &self.gather_rigid_static_contacts_pipeline,
                "gather rigid static contacts",
            ),
            (
                &self.resolve_rigid_static_contacts_pipeline,
                "resolve rigid static contacts",
            ),
        ] {
            let mut pass = accelerator.begin_compute_pass(&mut encoder, label);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(rigid_cell_count.max(1).div_ceil(64), 1, 1);
        }
        {
            let mut pass: wgpu::ComputePass<'_> =
                accelerator.begin_compute_pass(&mut encoder, "gather rigid grid interfaces");
            pass.set_pipeline(&self.gather_rigid_contacts_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups_indirect(&self.indirect_dispatch, 0);
        }
        for pipeline in &self.resolve_contacts_pipelines {
            let mut pass: wgpu::ComputePass<'_> =
                accelerator.begin_compute_pass(&mut encoder, "resolve colored cellular faces");
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups_indirect(&self.indirect_dispatch, 0);
        }
        // Contact discovery is intentionally broad.  Rebuild the same compact list from
        // actual pressure sources before running the expensive pressure stencil.
        {
            let mut pass = accelerator.begin_compute_pass(
                &mut encoder,
                "clear pressure active cellular pressure tiles",
            );
            pass.set_pipeline(&self.clear_active_tiles_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(tile_count.div_ceil(64), 1, 1);
        }
        {
            let mut pass = accelerator
                .begin_compute_pass(&mut encoder, "mark pressure active cellular pressure tiles");
            pass.set_pipeline(&self.mark_pressure_active_tiles_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(tile_count, 1, 1);
        }
        encoder.clear_buffer(&self.indirect_dispatch, 0, None);
        {
            let mut pass = accelerator.begin_compute_pass(
                &mut encoder,
                "compact pressure active cellular pressure tiles",
            );
            pass.set_pipeline(&self.compact_active_tiles_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_bind_group(1, &self.indirect_bind_group, &[]);
            pass.dispatch_workgroups(tile_count.div_ceil(64), 1, 1);
        }
        for (pipeline, label) in [
            (
                &self.propagate_pending_pipeline,
                "propagate pending cellular pressure",
            ),
            (
                &self.propagate_b_pipeline,
                "propagate cellular pressure B 1",
            ),
            (
                &self.propagate_a_pipeline,
                "propagate cellular pressure A 1",
            ),
            (
                &self.propagate_b_pipeline,
                "propagate cellular pressure B 2",
            ),
            (
                &self.propagate_a_pipeline,
                "propagate cellular pressure A 2",
            ),
            (&self.finalize_pipeline, "finalize cellular pressure"),
        ] {
            let mut pass: wgpu::ComputePass<'_> =
                accelerator.begin_compute_pass(&mut encoder, label);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            if label == "finalize cellular pressure" {
                pass.set_bind_group(1, &self.rigid_damage_bind_group, &[]);
            }
            pass.dispatch_workgroups_indirect(&self.indirect_dispatch, 0);
        }
        {
            let mut pass =
                accelerator.begin_compute_pass(&mut encoder, "apply rigid pressure damage");
            pass.set_pipeline(&self.apply_rigid_damage_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups_indirect(&self.rigid_damage_dispatch, 0);
        }
        let readback_slot_index: usize = if rigid_body_count == 0 {
            0
        } else if let Some(index) = self.rigid_reaction_readback_slots.iter().position(|slot| {
            slot.status
                .lock()
                .is_ok_and(|status| matches!(*status, RigidGranularReadbackStatus::Available))
        }) {
            index
        } else {
            tracing::warn!("rigid reaction readback pool saturated; skipping this batch");
            accelerator.wgpu_queue().submit(Some(encoder.finish()));
            self.tick = self.tick.wrapping_add(1);
            return Ok(());
        };
        let mut mapping = None;
        if rigid_body_count != 0 {
            {
                let slot = &self.rigid_reaction_readback_slots[readback_slot_index];
                let mut status = slot.status.lock().map_err(|_| {
                    io::Error::other("Rigid granular readback state is unavailable")
                })?;
                if matches!(*status, RigidGranularReadbackStatus::Available) {
                    *status = RigidGranularReadbackStatus::Mapping;
                    let sequence: u64 = self.rigid_reaction_sequence_next;
                    self.rigid_reaction_sequence_next =
                        self.rigid_reaction_sequence_next.wrapping_add(1);
                    let reaction_size: u64 = u64::from(rigid_body_count) * 80;
                    let statistics_size: u64 = u64::from(rigid_body_count) * 48;
                    let statistics_offset: u64 = reaction_size;
                    let fracture_count_offset: u64 = statistics_offset + statistics_size;
                    let fractures_offset: u64 = fracture_count_offset + 4;
                    encoder.copy_buffer_to_buffer(
                        self.rigid_reactions.wgpu_buffer(),
                        0,
                        &slot.buffer,
                        0,
                        reaction_size,
                    );
                    encoder.copy_buffer_to_buffer(
                        self.rigid_contact_statistics.wgpu_buffer(),
                        0,
                        &slot.buffer,
                        statistics_offset,
                        statistics_size,
                    );
                    encoder.clear_buffer(
                        self.rigid_reactions.wgpu_buffer(),
                        0,
                        Some(reaction_size),
                    );
                    encoder.copy_buffer_to_buffer(
                        &self.rigid_fracture_count,
                        0,
                        &slot.buffer,
                        fracture_count_offset,
                        4,
                    );
                    encoder.copy_buffer_to_buffer(
                        self.rigid_fractures.wgpu_buffer(),
                        0,
                        &slot.buffer,
                        fractures_offset,
                        self.rigid_fracture_word_count * 4,
                    );
                    mapping = Some((
                        slot,
                        sequence,
                        fracture_count_offset,
                        fractures_offset,
                        fractures_offset + self.rigid_fracture_word_count * 4,
                    ));
                }
            }
        }
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        if let Some((slot, sequence, fracture_count_offset, fractures_offset, mapped_size)) =
            mapping
        {
            let mapped_buffer: wgpu::Buffer = slot.buffer.clone();
            let callback_status = slot.status.clone();
            let body_count: usize = rigid_body_count as usize;
            mapped_buffer.clone().slice(0..mapped_size).map_async(
                wgpu::MapMode::Read,
                move |result| {
                    let result: Result<RigidGranularReactionBatch, String> = match result {
                        Ok(()) => mapped_buffer
                            .slice(0..mapped_size)
                            .get_mapped_range()
                            .map_err(|error| error.to_string())
                            .and_then(|mapped| {
                                let mut reactions = Vec::with_capacity(body_count);
                                let mut constraints = Vec::with_capacity(body_count);
                                let mut supports = Vec::with_capacity(body_count);
                                let mut recovery = Vec::with_capacity(body_count);
                                let mut source_motion = Vec::with_capacity(body_count);
                                for bytes in mapped[..body_count * 80].chunks_exact(80) {
                                    let state = |offset: usize| {
                                        [
                                            i32::from_le_bytes(
                                                bytes[offset..offset + 4].try_into().unwrap(),
                                            ) as f32
                                                / 65536.0,
                                            i32::from_le_bytes(
                                                bytes[offset + 4..offset + 8].try_into().unwrap(),
                                            ) as f32
                                                / 65536.0,
                                            i32::from_le_bytes(
                                                bytes[offset + 8..offset + 12].try_into().unwrap(),
                                            ) as f32
                                                / 65536.0,
                                            u32::from_le_bytes(
                                                bytes[offset + 12..offset + 16].try_into().unwrap(),
                                            ) as f32
                                                / 256.0,
                                        ]
                                    };
                                    constraints.push(state(16));
                                    supports.push(state(32));
                                    recovery.push(state(48));
                                    source_motion.push([
                                        f32::from_le_bytes(bytes[64..68].try_into().unwrap()),
                                        f32::from_le_bytes(bytes[68..72].try_into().unwrap()),
                                        f32::from_le_bytes(bytes[72..76].try_into().unwrap()),
                                        u32::from_le_bytes(bytes[76..80].try_into().unwrap())
                                            as f32,
                                    ]);
                                    let overflow =
                                        i32::from_le_bytes(bytes[12..16].try_into().unwrap());
                                    if overflow != 0 {
                                        drop(mapped);
                                        mapped_buffer.unmap();
                                        return Err(
                                            "Rigid granular reaction accumulator overflowed"
                                                .to_owned(),
                                        );
                                    }
                                    reactions.push([
                                        i32::from_le_bytes(bytes[0..4].try_into().unwrap()) as f32
                                            / 256.0,
                                        i32::from_le_bytes(bytes[4..8].try_into().unwrap()) as f32
                                            / 256.0,
                                        i32::from_le_bytes(bytes[8..12].try_into().unwrap()) as f32
                                            / 64.0,
                                    ]);
                                }
                                let statistics_start: usize = body_count * 80;
                                let contact_counts = mapped
                                    [statistics_start..statistics_start + body_count * 48]
                                    .chunks_exact(48)
                                    .map(|bytes| {
                                        u32::from_le_bytes(bytes[0..4].try_into().unwrap())
                                    })
                                    .collect::<Vec<_>>()
                                    .into_boxed_slice();
                                let static_contact_counts = mapped
                                    [statistics_start..statistics_start + body_count * 48]
                                    .chunks_exact(48)
                                    .map(|bytes| {
                                        u32::from_le_bytes(bytes[4..8].try_into().unwrap())
                                    })
                                    .collect::<Vec<_>>()
                                    .into_boxed_slice();
                                let granular_contact_counts = mapped
                                    [statistics_start..statistics_start + body_count * 48]
                                    .chunks_exact(48)
                                    .map(|bytes| {
                                        u32::from_le_bytes(bytes[12..16].try_into().unwrap())
                                            & 0xffff
                                    })
                                    .collect::<Vec<_>>()
                                    .into_boxed_slice();
                                let moving_contact_counts = mapped
                                    [statistics_start..statistics_start + body_count * 48]
                                    .chunks_exact(48)
                                    .map(|bytes| {
                                        u32::from_le_bytes(bytes[12..16].try_into().unwrap()) >> 16
                                    })
                                    .collect::<Vec<_>>()
                                    .into_boxed_slice();
                                let energy_budgets = mapped
                                    [statistics_start..statistics_start + body_count * 48]
                                    .chunks_exact(48)
                                    .map(|bytes| {
                                        u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as f32
                                            / 256.0
                                    })
                                    .collect::<Vec<_>>()
                                    .into_boxed_slice();
                                let fracture_count = u32::from_le_bytes(
                                    mapped[fracture_count_offset as usize
                                        ..fracture_count_offset as usize + 4]
                                        .try_into()
                                        .unwrap(),
                                );
                                let fractured_slots = if fracture_count == 0 {
                                    Vec::new()
                                } else {
                                    mapped[fractures_offset as usize..mapped_size as usize]
                                        .chunks_exact(4)
                                        .enumerate()
                                        .flat_map(|(word, bytes)| {
                                            let bits =
                                                u32::from_le_bytes(bytes.try_into().unwrap());
                                            (0..32).filter_map(move |bit| {
                                                ((bits & (1 << bit)) != 0)
                                                    .then_some((word as u32) * 32 + bit)
                                            })
                                        })
                                        .collect()
                                }
                                .into_boxed_slice();
                                drop(mapped);
                                mapped_buffer.unmap();
                                Ok(RigidGranularReactionBatch {
                                    sequence,
                                    topology_revision: rigid_topology_revision,
                                    body_count,
                                    reactions: reactions.into_boxed_slice(),
                                    contact_counts,
                                    static_contact_counts,
                                    granular_contact_counts,
                                    moving_contact_counts,
                                    energy_budgets,
                                    constraints: constraints.into_boxed_slice(),
                                    supports: supports.into_boxed_slice(),
                                    recovery: recovery.into_boxed_slice(),
                                    source_motion: source_motion.into_boxed_slice(),
                                    fractured_slots,
                                })
                            }),
                        Err(_) => Err("Rigid granular reaction readback failed".to_owned()),
                    };
                    if let Ok(mut status) = callback_status.lock() {
                        *status = RigidGranularReadbackStatus::Complete(result);
                    }
                },
            );
        }
        self.tick = self.tick.wrapping_add(1);
        Ok(())
    }

    /// Collects completed mappings and returns every now-contiguous ordered batch
    pub(crate) fn collect_rigid_reactions(
        &mut self,
    ) -> Result<Vec<RigidGranularReactionBatch>, io::Error> {
        for slot in &self.rigid_reaction_readback_slots {
            let mut status = slot
                .status
                .lock()
                .map_err(|_| io::Error::other("Rigid granular readback state is unavailable"))?;
            if matches!(*status, RigidGranularReadbackStatus::Complete(_)) {
                let RigidGranularReadbackStatus::Complete(result) =
                    std::mem::replace(&mut *status, RigidGranularReadbackStatus::Available)
                else {
                    unreachable!()
                };
                let batch = result.map_err(io::Error::other)?;
                self.rigid_reaction_completed.insert(batch.sequence, batch);
            }
        }
        self.rigid_reaction_completed
            .retain(|_, batch| batch.topology_revision == self.rigid_topology_revision);
        let mut ordered = Vec::new();
        while let Some(batch) = self
            .rigid_reaction_completed
            .remove(&self.rigid_reaction_sequence_apply_next)
        {
            ordered.push(batch);
            self.rigid_reaction_sequence_apply_next =
                self.rigid_reaction_sequence_apply_next.wrapping_add(1);
        }
        Ok(ordered)
    }
}
