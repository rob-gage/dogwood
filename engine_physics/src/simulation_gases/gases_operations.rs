// Copyright Rob Gage 2026

use engine_compute::Accelerator;
use engine_compute::AcceleratorBuffer;

use super::Gases;
use crate::simulation::simulation_constants::AUTHORED_CONCENTRATION;
use crate::simulation::simulation_constants::PRESSURE_ITERATION_COUNT;
use crate::tiles::TileArea;
use crate::tiles::TileCoordinates;

impl Gases {
    /// Applies stable authored concentrations and explicit cell clears
    pub fn apply_edits(
        &self,
        accelerator: &Accelerator,
        concentrations: &[(usize, u32, f32)],
        clear_cells: &[usize],
    ) {
        fn runs(indices: &[usize], mut write: impl FnMut(usize, usize)) {
            let mut start: usize = 0;
            while start < indices.len() {
                let mut end: usize = start + 1;
                while end < indices.len() && indices[end] == indices[end - 1] + 1 {
                    end += 1;
                }
                write(indices[start], end - start);
                start = end;
            }
        }
        runs(clear_cells, |index, count| {
            accelerator.wgpu_queue().write_buffer(
                self.velocity.wgpu_buffer(),
                index as u64 * 8,
                &vec![0; count * 8],
            )
        });
        for species in 0..self.gas_count {
            runs(clear_cells, |index, count| {
                accelerator.wgpu_queue().write_buffer(
                    self.concentrations.wgpu_buffer(),
                    (u64::from(species) * u64::from(self.buffered_cell_count) + index as u64) * 4,
                    &vec![0; count * 4],
                )
            });
            let mut authored: Vec<usize> = concentrations
                .iter()
                .filter_map(|(cell, value, _)| (*value == species).then_some(*cell))
                .collect();
            authored.sort_unstable();
            runs(&authored, |index, count| {
                accelerator.wgpu_queue().write_buffer(
                    self.concentrations.wgpu_buffer(),
                    (u64::from(species) * u64::from(self.buffered_cell_count) + index as u64) * 4,
                    &vec![AUTHORED_CONCENTRATION.to_bits().to_le_bytes(); count]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>(),
                )
            });
        }
        let mut temperatures: Vec<(usize, f32)> = concentrations
            .iter()
            .map(|&(cell, _, temperature)| (cell, temperature))
            .collect();
        temperatures.sort_unstable_by_key(|&(cell, _)| cell);
        let mut start: usize = 0;
        while start < temperatures.len() {
            let mut end: usize = start + 1;
            while end < temperatures.len() && temperatures[end].0 == temperatures[end - 1].0 + 1 {
                end += 1;
            }
            let gas_temperature_bytes: Vec<u8> = temperatures[start..end]
                .iter()
                .flat_map(|&(_, temperature)| temperature.to_bits().to_le_bytes())
                .collect();
            accelerator.wgpu_queue().write_buffer(
                self.gas_temperature.wgpu_buffer(),
                temperatures[start].0 as u64 * 4,
                &gas_temperature_bytes,
            );
            start = end;
        }
    }

    /// Advances the full resident gas field without CPU readback
    pub fn simulate(
        &self,
        accelerator: &Accelerator,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        gravity: [f32; 2],
        delta_time: f32,
    ) {
        self.simulate_pre_coupling(
            accelerator,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            gravity,
            delta_time,
        );
        self.simulate_post_coupling(accelerator);
    }

    pub(crate) const fn velocity_buffer(&self) -> &AcceleratorBuffer {
        &self.velocity
    }

    pub(crate) fn simulate_pre_coupling(
        &self,
        accelerator: &Accelerator,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        gravity: [f32; 2],
        delta_time: f32,
    ) {
        if self.gas_count == 0 {
            return;
        }
        self.write_parameters(
            accelerator,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            gravity,
            delta_time,
            None,
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gas simulation"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.advect_velocity_pipeline,
            self.buffered_cell_count,
            "advect gas velocity",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.curl_pipeline,
            self.buffered_cell_count,
            "calculate gas curl",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.force_pipeline,
            self.buffered_cell_count,
            "apply gas buoyancy and vorticity confinement",
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    pub(crate) fn simulate_post_coupling(&self, accelerator: &Accelerator) {
        if self.gas_count == 0 {
            return;
        }
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gas projection and transport"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.divergence_pipeline,
            self.buffered_cell_count,
            "calculate gas divergence",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.pressure_clear_pipeline,
            self.buffered_cell_count,
            "clear gas pressure",
        );
        for iteration in 0..PRESSURE_ITERATION_COUNT {
            let (pipeline, label): (&wgpu::ComputePipeline, &str) = if iteration % 2 == 0 {
                (&self.pressure_a_pipeline, "solve gas pressure into A")
            } else {
                (&self.pressure_b_pipeline, "solve gas pressure into B")
            };
            self.dispatch(
                accelerator,
                &mut encoder,
                pipeline,
                self.buffered_cell_count,
                label,
            );
        }
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.projection_pipeline,
            self.buffered_cell_count,
            "project gas velocity",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.concentration_pipeline,
            self.buffered_cell_count * self.gas_count,
            "advect gas concentrations",
        );
        encoder.copy_buffer_to_buffer(
            self.concentration_scratch.wgpu_buffer(),
            0,
            self.concentrations.wgpu_buffer(),
            0,
            u64::from(self.buffered_cell_count) * u64::from(self.gas_count) * 4,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Clears physical slots after they acquire a new world interpretation
    pub fn clear_area(
        &self,
        accelerator: &Accelerator,
        area: TileArea,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) {
        if self.gas_count == 0 {
            return;
        }
        self.write_parameters(
            accelerator,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            [0.0; 2],
            0.0,
            Some(area),
        );
        let dimensions: [u16; 2] = area.dimensions();
        let streamed_gas_cell_count: u32 = u32::from(dimensions[0]) * u32::from(dimensions[1]) * 64;
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gas streamed area clear"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.clear_area_pipeline,
            streamed_gas_cell_count,
            "clear incoming gas area",
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }
}
