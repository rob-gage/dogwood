// Copyright Rob Gage 2026

use crate::tiles::TileCoordinates;
use engine_compute::{
    Accelerator,
    AcceleratorBuffer,
};
use std::num::NonZeroU64;

/// The number of independently epoch-guarded granular sweeps per fixed tick
const MOVEMENT_STEPS_PER_TICK: u32 = 4;
/// The maximum number of cells one grain may travel during one sweep
const MAXIMUM_MOVEMENT_CELLS: u32 = 4;
/// Uniform-buffer bytes reserved for each movement step
const PARAMETER_SIZE: u64 = 64;

/// Simulates dynamic cellular material in the canonical cellular buffers
pub struct CellularDynamic {
    /// Persistent velocity and subcell residual attached to each physical cell slot
    kinematics: AcceleratorBuffer,
    /// Last movement-step epoch processed, attached to dynamic matter
    processed_epochs: AcceleratorBuffer,
    /// Physical tile activity countdowns retained across fixed ticks
    active_tiles: AcceleratorBuffer,
    /// Compacted logical tile indices, one fixed-capacity segment per parity group
    active_tile_indices: AcceleratorBuffer,
    /// Four GPU-written indirect movement dispatch records
    indirect_dispatch: wgpu::Buffer,
    /// Ring mapping, gravity, movement-step duration, epoch, and parity-group input
    parameters: wgpu::Buffer,
    /// Byte stride satisfying the device's dynamic uniform-offset alignment
    parameter_stride: u32,
    /// Canonical cellular state, solver state, and one dynamic parameter binding
    bind_group: wgpu::BindGroup,
    /// Compaction-only binding for the indirect dispatch records
    indirect_bind_group: wgpu::BindGroup,
    /// The four ordered world-tile parity pipelines
    movement_pipelines: [wgpu::ComputePipeline; 4],
    /// Compacts active tiles into the four parity-group segments
    compact_pipeline: wgpu::ComputePipeline,
    /// Number of cells in the full buffered tile ring
    buffered_cell_count: u32,
    /// Physical dimensions of the buffered tile ring
    buffered_tile_size: [u16; 2],
    /// Epoch assigned to the next complete A/B/C/D movement sweep
    epoch: u32,
}

impl CellularDynamic {

    pub const fn kinematics_buffer(&self) -> &AcceleratorBuffer { &self.kinematics }

    pub(crate) const fn active_tiles_buffer(&self) -> &AcceleratorBuffer {
        &self.active_tiles
    }

    /// Creates the private active-tile cellular dynamic solver
    pub fn new(
        accelerator: &Accelerator,
        cellular_material_identifiers: &AcceleratorBuffer,
        cellular_appearances: &AcceleratorBuffer,
        external_body_occupancy: &AcceleratorBuffer,
        buffered_width: u16,
        buffered_height: u16,
    ) -> Self {
        let buffered_tile_count: u32 = u32::from(buffered_width) * u32::from(buffered_height);
        let buffered_cell_count: u32 = buffered_tile_count.checked_mul(64)
            .expect("Cellular dynamic buffer exceeds GPU indexing range");
        let device: &wgpu::Device = accelerator.wgpu_device();
        let kinematics: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let processed_epochs: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_cell_count as usize);
        let active_tiles: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_tile_count as usize);
        let active_tile_indices: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_tile_count as usize * 4);
        let indirect_dispatch: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cellular dynamic indirect dispatch"),
            size: 48,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT |
                wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_alignment: u64 = u64::from(device.limits().min_uniform_buffer_offset_alignment);
        let parameter_stride: u32 = PARAMETER_SIZE.div_ceil(uniform_alignment)
            .checked_mul(uniform_alignment).unwrap().try_into().unwrap();
        let parameters: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cellular dynamic movement-step parameters"),
            size: u64::from(parameter_stride) * u64::from(MOVEMENT_STEPS_PER_TICK),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("cellular dynamic bind group layout"),
                entries: &[
                    Self::storage_layout_entry(0, false),
                    Self::storage_layout_entry(1, false),
                    Self::storage_layout_entry(2, false),
                    Self::storage_layout_entry(3, false),
                    Self::storage_layout_entry(4, false),
                    Self::storage_layout_entry(5, true),
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: NonZeroU64::new(PARAMETER_SIZE),
                        },
                        count: None,
                    },
                    Self::storage_layout_entry(7, false),
                ],
            },
        );
        let indirect_bind_group_layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("cellular dynamic indirect bind group layout"),
                entries: &[Self::storage_layout_entry(0, false)],
            });
        let bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("cellular dynamic bind group"),
                layout: &bind_group_layout,
                entries: &[
                    Self::binding(0, cellular_material_identifiers),
                    Self::binding(1, cellular_appearances),
                    Self::binding(2, &kinematics),
                    Self::binding(3, &processed_epochs),
                    Self::binding(4, &active_tiles),
                    Self::binding(5, external_body_occupancy),
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &parameters,
                            offset: 0,
                            size: NonZeroU64::new(PARAMETER_SIZE),
                        }),
                    },
                    Self::binding(7, &active_tile_indices),
                ],
            },
        );
        let indirect_bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("cellular dynamic indirect bind group"),
                layout: &indirect_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: indirect_dispatch.as_entire_binding(),
                }],
            },
        );
        let shader: wgpu::ShaderModule = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cellular dynamic active-tile shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("cellular_dynamic.wgsl").into()),
        });
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("cellular dynamic pipeline layout"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let compact_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("cellular dynamic compaction pipeline layout"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(&indirect_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let movement_pipelines: [wgpu::ComputePipeline; 4] = [
            Self::create_pipeline(device, &pipeline_layout, &shader,
                "cellular dynamic group A pipeline", "move_cellular_dynamic_group_a"),
            Self::create_pipeline(device, &pipeline_layout, &shader,
                "cellular dynamic group B pipeline", "move_cellular_dynamic_group_b"),
            Self::create_pipeline(device, &pipeline_layout, &shader,
                "cellular dynamic group C pipeline", "move_cellular_dynamic_group_c"),
            Self::create_pipeline(device, &pipeline_layout, &shader,
                "cellular dynamic group D pipeline", "move_cellular_dynamic_group_d"),
        ];
        let compact_pipeline: wgpu::ComputePipeline = Self::create_pipeline(
            device,
            &compact_pipeline_layout,
            &shader,
            "cellular dynamic active tile compaction pipeline",
            "compact_active_cellular_dynamic_tiles",
        );
        Self {
            kinematics,
            processed_epochs,
            active_tiles,
            active_tile_indices,
            indirect_dispatch,
            parameters,
            parameter_stride,
            bind_group,
            indirect_bind_group,
            movement_pipelines,
            compact_pipeline,
            buffered_cell_count,
            buffered_tile_size: [buffered_width, buffered_height],
            epoch: 1,
        }
    }

    /// Runs four A/B/C/D sweeps directly against canonical cellular state
    pub fn simulate_cellular_dynamic_tick(
        &mut self,
        accelerator: &Accelerator,
        buffered_origin: TileCoordinates,
        ring_offset_x: u16,
        ring_offset_y: u16,
        gravity: [f32; 2],
        delta_time: f32,
    ) {
        if self.epoch > u32::MAX - MOVEMENT_STEPS_PER_TICK {
            let mut encoder: wgpu::CommandEncoder = accelerator.wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("cellular dynamic epoch rollover"),
                });
            encoder.clear_buffer(self.processed_epochs.wgpu_buffer(), 0, None);
            accelerator.wgpu_queue().submit(Some(encoder.finish()));
            self.epoch = 1;
        }
        let step_delta_time: f32 = delta_time / MOVEMENT_STEPS_PER_TICK as f32;
        for step in 0..MOVEMENT_STEPS_PER_TICK {
            let values: [u32; 16] = [
                buffered_origin.x as u32,
                buffered_origin.y as u32,
                u32::from(self.buffered_tile_size[0]),
                u32::from(self.buffered_tile_size[1]),
                u32::from(ring_offset_x),
                u32::from(ring_offset_y),
                gravity[0].to_bits(),
                gravity[1].to_bits(),
                step_delta_time.to_bits(),
                self.epoch + step,
                self.buffered_cell_count,
                MAXIMUM_MOVEMENT_CELLS,
                0,
                0,
                0,
                0,
            ];
            let bytes: Vec<u8> = values.into_iter().flat_map(u32::to_le_bytes).collect();
            accelerator.wgpu_queue().write_buffer(
                &self.parameters,
                u64::from(self.parameter_stride) * u64::from(step),
                &bytes,
            );
        }
        let mut encoder: wgpu::CommandEncoder = accelerator.wgpu_device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("cellular dynamic simulation") },
        );
        let tile_count: u32 = self.buffered_cell_count / 64;
        encoder.clear_buffer(&self.indirect_dispatch, 0, None);
        {
            let mut pass: wgpu::ComputePass<'_> =
                encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("compact active cellular dynamic tiles"),
                    timestamp_writes: None,
                });
            pass.set_pipeline(&self.compact_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[0]);
            pass.set_bind_group(1, &self.indirect_bind_group, &[]);
            pass.dispatch_workgroups(tile_count.div_ceil(64), 1, 1);
        }
        for step in 0..MOVEMENT_STEPS_PER_TICK {
            let dynamic_offset: u32 = self.parameter_stride * step;
            for (group, pipeline) in self.movement_pipelines.iter().enumerate() {
                let label: &str = match group {
                    0 => "cellular dynamic group A",
                    1 => "cellular dynamic group B",
                    2 => "cellular dynamic group C",
                    _ => "cellular dynamic group D",
                };
                let mut pass: wgpu::ComputePass<'_> =
                    encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some(label),
                        timestamp_writes: None,
                    });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &self.bind_group, &[dynamic_offset]);
                pass.dispatch_workgroups_indirect(
                    &self.indirect_dispatch,
                    group as u64 * 12,
                );
            }
        }
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        self.epoch += MOVEMENT_STEPS_PER_TICK;
    }

    /// Clears matter-attached transient state and wakes cells replaced by CPU writes
    pub fn clear_cellular_dynamic_kinematics(
        &self,
        accelerator: &Accelerator,
        cell_start: usize,
        cell_count: usize,
    ) {
        accelerator.wgpu_queue().write_buffer(
            self.kinematics.wgpu_buffer(),
            cell_start as u64 * 16,
            &vec![0; cell_count * 16],
        );
        accelerator.wgpu_queue().write_buffer(
            self.processed_epochs.wgpu_buffer(),
            cell_start as u64 * 4,
            &vec![0; cell_count * 4],
        );
        let first_tile: usize = cell_start / 64;
        let last_tile: usize = (cell_start + cell_count - 1) / 64;
        accelerator.wgpu_queue().write_buffer(
            self.active_tiles.wgpu_buffer(),
            first_tile as u64 * 4,
            &(first_tile..=last_tile)
                .flat_map(|_| 0x80000008u32.to_le_bytes()).collect::<Vec<_>>(),
        );
    }

    fn binding(binding: u32, buffer: &AcceleratorBuffer) -> wgpu::BindGroupEntry<'_> {
        wgpu::BindGroupEntry {
            binding,
            resource: buffer.wgpu_buffer().as_entire_binding(),
        }
    }

    fn storage_layout_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }
    }

    fn create_pipeline(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
        label: &str,
        entry_point: &str,
    ) -> wgpu::ComputePipeline {
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            module: shader,
            entry_point: Some(entry_point),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    }

}

impl Drop for CellularDynamic {

    fn drop(&mut self) {
        self.kinematics.free();
        self.processed_epochs.free();
        self.active_tiles.free();
        self.active_tile_indices.free();
        self.indirect_dispatch.destroy();
        self.parameters.destroy();
    }

}
