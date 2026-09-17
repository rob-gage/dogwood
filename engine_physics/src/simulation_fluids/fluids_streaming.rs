// Copyright Rob Gage 2026

use super::*;

impl Fluids {
    /// Compacts outgoing authoritative records, releases their slots, and copies them for readback
    pub fn export(
        &self,
        accelerator: &Accelerator,
        download: &FluidDownload,
        active_origin: TileCoordinates,
        active_width: u16,
        active_height: u16,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) {
        accelerator.wgpu_queue().write_buffer(
            self.streaming_count.wgpu_buffer(),
            0,
            &0u32.to_le_bytes(),
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
            Some(download.area),
            [0.0; 2],
            0.0,
            None,
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("fluid export"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.export_pipeline,
            self.particle_capacity,
            "compact outgoing fluid particles",
        );
        encoder.copy_buffer_to_buffer(
            self.streaming_count.wgpu_buffer(),
            0,
            &download.buffer,
            0,
            4,
        );
        encoder.copy_buffer_to_buffer(
            self.streaming_particles.wgpu_buffer(),
            0,
            &download.buffer,
            16,
            u64::from(self.particle_capacity) * ChunkFluidParticle::GPU_SIZE as u64,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Reconstructs dormant records and copies their exact success flags for readback
    pub fn import(
        &self,
        accelerator: &Accelerator,
        upload: &FluidUpload,
        active_origin: TileCoordinates,
        active_width: u16,
        active_height: u16,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) -> Result<(), io::Error> {
        let mut bytes: Vec<u8> =
            Vec::with_capacity(upload.particles.len() * ChunkFluidParticle::GPU_SIZE);
        for particle in &upload.particles {
            particle.serialize_gpu(&mut bytes)?;
        }
        accelerator
            .wgpu_queue()
            .write_buffer(self.streaming_particles.wgpu_buffer(), 0, &bytes);
        accelerator.wgpu_queue().write_buffer(
            self.streaming_count.wgpu_buffer(),
            0,
            &(upload.particles.len() as u32).to_le_bytes(),
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
                    label: Some("fluid import"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.import_pipeline,
            upload.particles.len() as u32,
            "import dormant fluid particles",
        );
        self.encode_rebuild(accelerator, &mut encoder);
        encoder.copy_buffer_to_buffer(
            self.streaming_results.wgpu_buffer(),
            0,
            &upload.buffer,
            0,
            upload.particles.len() as u64 * 4,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        Ok(())
    }

    pub(super) fn encode_rebuild(
        &self,
        accelerator: &Accelerator,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        self.dispatch(
            accelerator,
            encoder,
            &self.clear_buckets_pipeline,
            self.bucket_count,
            "clear fluid buckets",
        );
        self.dispatch(
            accelerator,
            encoder,
            &self.insert_buckets_pipeline,
            self.particle_capacity,
            "insert fluid particles into buckets",
        );
        self.dispatch(
            accelerator,
            encoder,
            &self.raster_pipeline,
            self.buffered_cell_count,
            "rasterize derived fluid cells",
        );
    }

    pub(super) fn encode_rebuild_indirect(
        &self,
        accelerator: &Accelerator,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        self.dispatch_indirect(
            accelerator,
            encoder,
            &self.clear_buckets_pipeline,
            36,
            "clear Accelerator edited fluid buckets",
        );
        self.dispatch_indirect(
            accelerator,
            encoder,
            &self.insert_buckets_pipeline,
            48,
            "insert Accelerator edited fluid particles",
        );
        self.dispatch_indirect(
            accelerator,
            encoder,
            &self.raster_pipeline,
            60,
            "rasterize Accelerator edited fluid cells",
        );
    }

    pub(super) fn dispatch(
        &self,
        accelerator: &Accelerator,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::ComputePipeline,
        count: u32,
        label: &str,
    ) {
        let mut pass: wgpu::ComputePass<'_> = accelerator.begin_compute_pass(encoder, label);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
    }

    pub(super) fn dispatch_indirect(
        &self,
        accelerator: &Accelerator,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::ComputePipeline,
        offset: u64,
        label: &str,
    ) {
        let mut pass: wgpu::ComputePass<'_> = accelerator.begin_compute_pass(encoder, label);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups_indirect(&self.accelerator_edit_dispatch, offset);
    }

    pub(super) fn write_parameters(
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
        streaming_area: Option<TileArea>,
        gravity: [f32; 2],
        delta_time: f32,
        sample: Option<([f32; 2], ActorCollisionShape)>,
    ) {
        let streaming_origin: TileCoordinates =
            streaming_area.map_or(TileCoordinates { x: 0, y: 0 }, TileArea::origin);
        let streaming_dimensions: [u16; 2] = streaming_area.map_or([0, 0], TileArea::dimensions);
        let (sample_center, sample_kind, sample_parameters) =
            sample.map_or(([0.0; 2], 0, [0.0; 2]), |(center, shape)| {
                let (kind, parameters) = shape.accelerator_parameters();
                (center, kind, parameters)
            });
        let values: [u32; 32] = [
            buffered_origin.x as u32,
            buffered_origin.y as u32,
            u32::from(buffered_width),
            u32::from(buffered_height),
            active_origin.x as u32,
            active_origin.y as u32,
            u32::from(active_width),
            u32::from(active_height),
            u32::from(ring_offset_x),
            u32::from(ring_offset_y),
            self.bucket_dimensions[0],
            self.bucket_dimensions[1],
            streaming_origin.x as u32,
            streaming_origin.y as u32,
            u32::from(streaming_dimensions[0]),
            u32::from(streaming_dimensions[1]),
            gravity[0].to_bits(),
            gravity[1].to_bits(),
            delta_time.to_bits(),
            self.particle_capacity,
            self.buffered_cell_count,
            self.bucket_count,
            SUPPORT_RADIUS_CELLS.to_bits(),
            PARTICLE_RADIUS_CELLS.to_bits(),
            MAXIMUM_MOVEMENT_CELLS,
            0,
            sample_center[0].to_bits(),
            sample_center[1].to_bits(),
            sample_parameters[0].to_bits(),
            sample_parameters[1].to_bits(),
            sample_kind,
            0,
        ];
        let bytes: Vec<u8> = values.into_iter().flat_map(u32::to_le_bytes).collect();
        accelerator
            .wgpu_queue()
            .write_buffer(&self.parameters, 0, &bytes);
    }

    pub(super) fn binding<'a>(
        binding: u32,
        buffer: &'a AcceleratorBuffer,
    ) -> wgpu::BindGroupEntry<'a> {
        wgpu::BindGroupEntry {
            binding,
            resource: buffer.wgpu_buffer().as_entire_binding(),
        }
    }

    /// Returns the transient edit value used to remove fluid without spawning it
    pub const fn erase_edit() -> u32 {
        FLUID_EDIT_ERASE
    }
}
