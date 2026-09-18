// Copyright Rob Gage 2026

use engine_compute::Accelerator;

use super::CellularPressure;
use crate::tiles::CellCoordinates;
use crate::tiles::TileCoordinates;

impl CellularPressure {
    /// Queues a radial pressure impulse for the next simulation dispatch
    pub fn apply_radial_impulse(
        &self,
        accelerator: &Accelerator,
        origin: TileCoordinates,
        width: u16,
        height: u16,
        ring_x: u16,
        ring_y: u16,
        center: CellCoordinates,
        radius: f32,
        strength: f32,
    ) {
        let unclamped_minimum_cell_coordinates: CellCoordinates = CellCoordinates {
            x: (center.x as f32 - radius).floor() as i32,
            y: (center.y as f32 - radius).floor() as i32,
        };
        let unclamped_maximum_cell_coordinates: CellCoordinates = CellCoordinates {
            x: (center.x as f32 + radius).ceil() as i32,
            y: (center.y as f32 + radius).ceil() as i32,
        };
        let minimum_cell_coordinates: CellCoordinates = CellCoordinates {
            x: unclamped_minimum_cell_coordinates.x.max(origin.x * 8),
            y: unclamped_minimum_cell_coordinates.y.max(origin.y * 8),
        };
        let maximum_cell_coordinates: CellCoordinates = CellCoordinates {
            x: unclamped_maximum_cell_coordinates
                .x
                .min((origin.x + i32::from(width)) * 8 - 1),
            y: unclamped_maximum_cell_coordinates
                .y
                .min((origin.y + i32::from(height)) * 8 - 1),
        };
        if maximum_cell_coordinates.x < minimum_cell_coordinates.x
            || maximum_cell_coordinates.y < minimum_cell_coordinates.y
        {
            return;
        }
        self.write_parameters(
            accelerator,
            origin,
            width,
            height,
            ring_x,
            ring_y,
            center,
            radius,
            strength,
            0.0,
            [0.0; 2],
            0,
            0,
            minimum_cell_coordinates,
            [
                u32::try_from(maximum_cell_coordinates.x - minimum_cell_coordinates.x + 1).unwrap(),
                u32::try_from(maximum_cell_coordinates.y - minimum_cell_coordinates.y + 1).unwrap(),
            ],
        );
        let mut command_encoder: wgpu::CommandEncoder = accelerator
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("queue cellular radial impulse"),
            });
        let mut compute_pass: wgpu::ComputePass<'_> =
            accelerator.begin_compute_pass(&mut command_encoder, "queue cellular radial impulse");
        compute_pass.set_pipeline(&self.impulse_pipeline);
        compute_pass.set_bind_group(0, &self.bind_group, &[]);
        compute_pass.dispatch_workgroups(
            (u32::try_from(maximum_cell_coordinates.x - minimum_cell_coordinates.x + 1).unwrap()
                * u32::try_from(maximum_cell_coordinates.y - minimum_cell_coordinates.y + 1)
                    .unwrap())
            .div_ceil(64),
            1,
            1,
        );
        drop(compute_pass);
        accelerator
            .wgpu_queue()
            .submit(Some(command_encoder.finish()));
    }

    /// Clears pressure fields that are derived again during the next fixed tick
    pub fn clear_transient_state(
        &self,
        accelerator: &Accelerator,
        cell_start: usize,
        cell_count: usize,
    ) {
        let zeroes: Vec<u8> = vec![0; cell_count * 16];
        let cellular_pressure_byte_offset: u64 = cell_start as u64 * 16;
        for buffer in [
            &self.pending_impulses,
            &self.pressure_a,
            &self.pressure_b,
            &self.retained_pressure,
        ] {
            accelerator.wgpu_queue().write_buffer(
                buffer.wgpu_buffer(),
                cellular_pressure_byte_offset,
                &zeroes,
            );
        }
    }
}
