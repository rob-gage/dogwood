// Copyright Rob Gage 2026

use super::*;

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
        let min = CellCoordinates {
            x: (center.x as f32 - radius).floor() as i32,
            y: (center.y as f32 - radius).floor() as i32,
        };
        let max = CellCoordinates {
            x: (center.x as f32 + radius).ceil() as i32,
            y: (center.y as f32 + radius).ceil() as i32,
        };
        let min = CellCoordinates {
            x: min.x.max(origin.x * 8),
            y: min.y.max(origin.y * 8),
        };
        let max = CellCoordinates {
            x: max.x.min((origin.x + i32::from(width)) * 8 - 1),
            y: max.y.min((origin.y + i32::from(height)) * 8 - 1),
        };
        if max.x < min.x || max.y < min.y {
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
            min,
            [
                u32::try_from(max.x - min.x + 1).unwrap(),
                u32::try_from(max.y - min.y + 1).unwrap(),
            ],
        );
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("queue cellular radial impulse"),
                });
        let mut pass =
            accelerator.begin_compute_pass(&mut encoder, "queue cellular radial impulse");
        pass.set_pipeline(&self.impulse_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(
            (u32::try_from(max.x - min.x + 1).unwrap() * u32::try_from(max.y - min.y + 1).unwrap())
                .div_ceil(64),
            1,
            1,
        );
        drop(pass);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Clears pressure fields that are derived again during the next fixed tick
    pub fn clear_transient_state(
        &self,
        accelerator: &Accelerator,
        cell_start: usize,
        cell_count: usize,
    ) {
        let zeroes: Vec<u8> = vec![0; cell_count * 16];
        let offset: u64 = cell_start as u64 * 16;
        for buffer in [
            &self.pending_impulses,
            &self.pressure_a,
            &self.pressure_b,
            &self.retained_pressure,
        ] {
            accelerator
                .wgpu_queue()
                .write_buffer(buffer.wgpu_buffer(), offset, &zeroes);
        }
    }
}
