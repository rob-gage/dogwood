// Copyright Rob Gage 2026

use super::*;

impl Fluids {
    /// Finalizes a slot reserved by an asynchronous producer.  The slot was
    /// removed from the free stack before this call, so this cannot race a
    /// normal spawn allocation.
    pub(crate) fn commit_reserved_particle(
        &self,
        accelerator: &Accelerator,
        slot: u32,
        material: u32,
        position: [f32; 2],
        velocity: [f32; 2],
        amount: f32,
        temperature: f32,
    ) {
        if slot >= self.particle_capacity {
            return;
        }
        let record = [
            material,
            1,
            position[0].to_bits(),
            position[1].to_bits(),
            velocity[0].to_bits(),
            velocity[1].to_bits(),
            0,
            0,
            amount.to_bits(),
            temperature.to_bits(),
        ];
        accelerator.wgpu_queue().write_buffer(
            self.particles.wgpu_buffer(),
            u64::from(slot) * 40,
            &record
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        );
    }
    pub(crate) const fn free_indices_buffer(&self) -> &AcceleratorBuffer {
        &self.free_indices
    }
    pub(crate) const fn free_count_buffer(&self) -> &AcceleratorBuffer {
        &self.free_count
    }

    pub(crate) const fn edit_cells_buffer(&self) -> &AcceleratorBuffer {
        &self.edit_cells
    }
    pub(crate) const fn edit_amounts_buffer(&self) -> &AcceleratorBuffer {
        &self.edit_amounts
    }
    pub(crate) const fn edit_temperatures_buffer(&self) -> &AcceleratorBuffer {
        &self.edit_temperatures
    }

    pub(crate) const fn accelerator_edits_pending_buffer(&self) -> &AcceleratorBuffer {
        &self.accelerator_edits_pending
    }

    /// Consumes edits written by another Accelerator subsystem using the same authoritative pool.
    pub(crate) fn consume_accelerator_edits(
        &self,
        accelerator: &Accelerator,
        active_origin: TileCoordinates,
        active_width: u16,
        active_height: u16,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) {
        self.write_parameters(
            accelerator,
            active_origin,
            active_width,
            active_height,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            None,
            [0.0; 2],
            0.0,
            None,
        );
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Accelerator fluid edits"),
                });
        {
            let mut pass =
                accelerator.begin_compute_pass(&mut encoder, "prepare Accelerator fluid edits");
            pass.set_pipeline(&self.prepare_accelerator_edits_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_bind_group(1, &self.accelerator_edit_prepare_bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        self.dispatch_indirect(
            accelerator,
            &mut encoder,
            &self.edit_remove_pipeline,
            0,
            "remove Accelerator edited fluid particles",
        );
        self.dispatch_indirect(
            accelerator,
            &mut encoder,
            &self.edit_spawn_pipeline,
            12,
            "spawn Accelerator edited fluid particles",
        );
        self.dispatch_indirect(
            accelerator,
            &mut encoder,
            &self.edit_clear_pipeline,
            24,
            "clear Accelerator fluid edits",
        );
        self.encode_rebuild_indirect(accelerator, &mut encoder);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Consumes ring-aligned spawn/erase edits and immediately refreshes derived cells
    pub fn apply_edits(
        &self,
        accelerator: &Accelerator,
        edits: &[(usize, u32, f32, f32)],
        active_origin: TileCoordinates,
        active_width: u16,
        active_height: u16,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) {
        // one bounded upload per aggregate scene-edit flush. The edit shader already
        // scans this dense ring buffer, so a sparse sequence of tiny writes buys nothing.
        let mut cells = vec![
            crate::materials::MaterialIdentifier::NULL.as_u32();
            self.buffered_cell_count as usize
        ];
        for (index, material_identifier, _, _) in edits {
            cells[*index] = *material_identifier;
        }
        let bytes: Vec<u8> = cells.into_iter().flat_map(u32::to_le_bytes).collect();
        accelerator
            .wgpu_queue()
            .write_buffer(self.edit_cells.wgpu_buffer(), 0, &bytes);
        let mut amounts = vec![0.0f32.to_bits(); self.buffered_cell_count as usize];
        let mut temperatures = vec![0.0f32.to_bits(); self.buffered_cell_count as usize];
        for (index, _, amount, temperature) in edits {
            amounts[*index] = amount.to_bits();
            temperatures[*index] = temperature.to_bits();
        }
        accelerator.wgpu_queue().write_buffer(
            self.edit_amounts.wgpu_buffer(),
            0,
            &amounts
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        accelerator.wgpu_queue().write_buffer(
            self.edit_temperatures.wgpu_buffer(),
            0,
            &temperatures
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        self.write_parameters(
            accelerator,
            active_origin,
            active_width,
            active_height,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            None,
            [0.0; 2],
            0.0,
            None,
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("fluid edits"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.edit_remove_pipeline,
            self.particle_capacity,
            "remove edited fluid particles",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.edit_spawn_pipeline,
            self.buffered_cell_count,
            "spawn edited fluid particles",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.edit_clear_pipeline,
            self.buffered_cell_count,
            "clear fluid edits",
        );
        self.encode_rebuild(accelerator, &mut encoder);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Moves authoritative particles and regenerates all transient derived state
    pub fn simulate(
        &self,
        accelerator: &Accelerator,
        active_origin: TileCoordinates,
        active_width: u16,
        active_height: u16,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        gravity: [f32; 2],
        delta_time: f32,
    ) {
        self.write_parameters(
            accelerator,
            active_origin,
            active_width,
            active_height,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            None,
            gravity,
            delta_time,
            None,
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("fluid simulation"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.classify_active_pipeline,
            self.particle_capacity,
            "classify active fluid particles",
        );
        for _ in 0..PBF_SUBSTEP_COUNT {
            self.dispatch(
                accelerator,
                &mut encoder,
                &self.predict_pipeline,
                self.particle_capacity,
                "predict fluid particles",
            );
            for _ in 0..PBF_CONSTRAINT_ITERATION_COUNT {
                self.dispatch(
                    accelerator,
                    &mut encoder,
                    &self.clear_buckets_pipeline,
                    self.bucket_count,
                    "clear predicted fluid buckets",
                );
                self.dispatch(
                    accelerator,
                    &mut encoder,
                    &self.insert_predicted_buckets_pipeline,
                    self.particle_capacity,
                    "insert predicted fluid particles into buckets",
                );
                self.dispatch(
                    accelerator,
                    &mut encoder,
                    &self.lambda_pipeline,
                    self.particle_capacity,
                    "calculate fluid lambdas",
                );
                self.dispatch(
                    accelerator,
                    &mut encoder,
                    &self.position_correction_pipeline,
                    self.particle_capacity,
                    "calculate fluid position corrections",
                );
                self.dispatch(
                    accelerator,
                    &mut encoder,
                    &self.apply_position_correction_pipeline,
                    self.particle_capacity,
                    "apply fluid position corrections",
                );
            }
            self.dispatch(
                accelerator,
                &mut encoder,
                &self.commit_pipeline,
                self.particle_capacity,
                "commit fluid particles",
            );
        }
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.clear_buckets_pipeline,
            self.bucket_count,
            "clear final fluid buckets",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.insert_buckets_pipeline,
            self.particle_capacity,
            "insert final fluid particles into buckets",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.velocity_smoothing_pipeline,
            self.particle_capacity,
            "calculate fluid velocity smoothing",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.apply_velocity_smoothing_pipeline,
            self.particle_capacity,
            "apply fluid velocity smoothing",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.raster_pipeline,
            self.buffered_cell_count,
            "rasterize derived fluid cells",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.cell_contact_pipeline,
            self.buffered_cell_count,
            "resolve fluid cell interactions",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.apply_cell_contact_pipeline,
            self.particle_capacity,
            "apply fluid cell contacts",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.raster_pipeline,
            self.buffered_cell_count,
            "refresh contacted fluid cells",
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    pub(crate) fn scatter_mechanical_response(&self, accelerator: &Accelerator) {
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("fluid mechanical response"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.mechanical_scatter_pipeline,
            self.particle_capacity,
            "scatter mechanical fluid velocity",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.raster_pipeline,
            self.buffered_cell_count,
            "refresh solved fluid coverage",
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Samples final derived fluid state across one gravity-relative pawn shape
    pub fn sample_pawn(
        &self,
        accelerator: &Accelerator,
        output: &wgpu::Buffer,
        center: [f32; 2],
        shape: ActorCollisionShape,
        active_origin: TileCoordinates,
        active_width: u16,
        active_height: u16,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        gravity: [f32; 2],
    ) {
        self.write_parameters(
            accelerator,
            active_origin,
            active_width,
            active_height,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            None,
            gravity,
            0.0,
            Some((center, shape)),
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("pawn fluid sample"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.sample_pipeline,
            1,
            "sample pawn fluid",
        );
        encoder.copy_buffer_to_buffer(self.sample_output.wgpu_buffer(), 0, output, 0, 32);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Rebuilds buckets and derived cells after a ring remap without moving particles
    pub fn refresh(
        &self,
        accelerator: &Accelerator,
        active_origin: TileCoordinates,
        active_width: u16,
        active_height: u16,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) {
        self.write_parameters(
            accelerator,
            active_origin,
            active_width,
            active_height,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            None,
            [0.0; 2],
            0.0,
            None,
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("fluid cellular refresh"),
                });
        self.encode_rebuild(accelerator, &mut encoder);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
}
