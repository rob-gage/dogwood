// Copyright Rob Gage 2026

use super::*;

impl Gases {
    /// Restores validated dormant gas directly into its dense authoritative slots
    pub fn import(
        &self,
        accelerator: &Accelerator,
        upload: &GasUpload,
        physical_indices: &[usize],
    ) {
        assert!(upload.cells.len() == physical_indices.len());
        for (physical_index, cell) in physical_indices.iter().zip(&upload.cells) {
            let mut velocity: Vec<u8> = Vec::with_capacity(8);
            for value in cell.velocity {
                velocity.extend_from_slice(&value.to_bits().to_le_bytes());
            }
            accelerator.wgpu_queue().write_buffer(
                self.velocity.wgpu_buffer(),
                *physical_index as u64 * 8,
                &velocity,
            );
            accelerator.wgpu_queue().write_buffer(
                self.gas_temperature.wgpu_buffer(),
                *physical_index as u64 * 4,
                &cell.temperature.to_bits().to_le_bytes(),
            );
            for (identifier, concentration) in &cell.species {
                let index: u64 = u64::from(identifier.index())
                    * u64::from(self.buffered_cell_count)
                    + *physical_index as u64;
                accelerator.wgpu_queue().write_buffer(
                    self.concentrations.wgpu_buffer(),
                    index * 4,
                    &concentration.to_bits().to_le_bytes(),
                );
            }
        }
    }

    /// Captures and clears an outgoing strip under the current ring interpretation
    pub fn export(
        &self,
        accelerator: &Accelerator,
        download: &GasDownload,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) {
        self.write_parameters(
            accelerator,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            [0.0; 2],
            0.0,
            Some(download.area),
        );
        let dimensions: [u16; 2] = download.area.dimensions();
        let count: u32 = u32::from(dimensions[0]) * u32::from(dimensions[1]) * 64;
        let byte_count: u64 = u64::from(count) * u64::from(5 + self.gas_count) * 4;
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gas export"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.export_pipeline,
            count,
            "export outgoing gas area",
        );
        encoder.copy_buffer_to_buffer(
            self.streaming_data.wgpu_buffer(),
            0,
            &download.buffer,
            0,
            byte_count,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
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

    pub(super) fn write_parameters(
        &self,
        accelerator: &Accelerator,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        gravity: [f32; 2],
        delta_time: f32,
        streaming_area: Option<TileArea>,
    ) {
        let streaming_origin: TileCoordinates =
            streaming_area.map_or(TileCoordinates { x: 0, y: 0 }, TileArea::origin);
        let streaming_dimensions: [u16; 2] = streaming_area.map_or([0, 0], TileArea::dimensions);
        let streaming_cell_count: u32 =
            u32::from(streaming_dimensions[0]) * u32::from(streaming_dimensions[1]) * 64;
        let values: [u32; 24] = [
            buffered_origin.x as u32,
            buffered_origin.y as u32,
            u32::from(buffered_width),
            u32::from(buffered_height),
            u32::from(ring_offset_x),
            u32::from(ring_offset_y),
            gravity[0].to_bits(),
            gravity[1].to_bits(),
            delta_time.to_bits(),
            self.buffered_cell_count,
            self.gas_count,
            streaming_cell_count,
            streaming_origin.x as u32,
            streaming_origin.y as u32,
            u32::from(streaming_dimensions[0]),
            u32::from(streaming_dimensions[1]),
            VORTICITY_CONFINEMENT.to_bits(),
            BUOYANCY_COEFFICIENT.to_bits(),
            MAXIMUM_SPEED_CELLS_PER_SECOND.to_bits(),
            FLUID_OBSTACLE_COVERAGE.to_bits(),
            AMBIENT_DENSITY.to_bits(),
            self.ambient_temperature.to_bits(),
            0,
            0,
        ];
        let bytes: Vec<u8> = values.into_iter().flat_map(u32::to_le_bytes).collect();
        accelerator
            .wgpu_queue()
            .write_buffer(&self.parameters, 0, &bytes);
    }

    pub(super) fn binding(binding: u32, buffer: &AcceleratorBuffer) -> wgpu::BindGroupEntry<'_> {
        wgpu::BindGroupEntry {
            binding,
            resource: buffer.wgpu_buffer().as_entire_binding(),
        }
    }
}
