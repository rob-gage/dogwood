// Copyright Rob Gage 2026

use super::collision_cellular_tile::CollisionCellularTile;
use crate::{
    scenes::{
        cellular_tiles::{
            CellularTileBuffers,
            CellularTileRegion,
            CELLULAR_TILE_SIZE,
        },
        SceneError,
    },
};
use engine_host::acceleration::Accelerator;
use std::sync::mpsc::{
    Receiver,
    Sender,
    channel,
};

/// Bytes occupied by one cellular tile's cells in the material identifier buffer
const CELLULAR_TILE_BYTE_COUNT: u64 = (CELLULAR_TILE_SIZE * CELLULAR_TILE_SIZE) as u64 * 4;

/// Bytes occupied by one cellular tile's occupancy
const CELLULAR_TILE_OCCUPANCY_BYTE_COUNT: u64 = 8;

/// Number of invocations in one compute workgroup
const WORKGROUP_SIZE: u32 = 64;

/// GPU extraction of the static occupancy of every active cellular tile
pub struct CollisionCellularTilePass {
    /// Pipeline packing one active tile's occupancy per invocation
    pipeline: wgpu::ComputePipeline,
    /// Bind group of the uniforms, material identifiers, and occupancies
    bind_group: wgpu::BindGroup,
    /// Active region dimensions and ring offset
    uniform_buffer: wgpu::Buffer,
    /// Occupancy of every active tile in active region order
    cellular_tile_buffer: wgpu::Buffer,
    /// Mappable copy of the occupancy buffer
    staging_buffer: wgpu::Buffer,
    /// Sender for staging buffer mapping results
    response_sender: Sender<Result<(), wgpu::BufferAsyncError>>,
    /// Staging buffer mapping results
    response_receiver: Receiver<Result<(), wgpu::BufferAsyncError>>,
}

impl CollisionCellularTilePass {

    /// Creates occupancy extraction resources for every tile the cellular tile buffers can hold
    pub fn new(accelerator: &Accelerator, cellular_tile_buffers: &CellularTileBuffers) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let cellular_tile_count: u64 =
            cellular_tile_buffers.material_identifier_buffer().size() / CELLULAR_TILE_BYTE_COUNT;
        let bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`CollisionCellularTilePass.bind_group_layout`"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`CollisionCellularTilePass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`CollisionCellularTilePass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("collision_cellular_tile.wgsl").into(),
                ),
            },
        );
        let pipeline: wgpu::ComputePipeline = device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor {
                label: Some("`CollisionCellularTilePass.pipeline`"),
                layout: Some(&pipeline_layout),
                module: &shader_module,
                entry_point: Some("compute"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            },
        );
        let uniform_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`CollisionCellularTilePass.uniform_buffer`"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cellular_tile_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`CollisionCellularTilePass.cellular_tile_buffer`"),
            size: cellular_tile_count.max(1) * CELLULAR_TILE_OCCUPANCY_BYTE_COUNT,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging_buffer: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("`CollisionCellularTilePass.staging_buffer`"),
            size: cellular_tile_buffer.size(),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`CollisionCellularTilePass.bind_group`"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: cellular_tile_buffers.material_identifier_buffer()
                        .as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: cellular_tile_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: cellular_tile_buffers.rigid_body_cell_buffer().as_entire_binding(),
                },
            ],
        });
        let (response_sender, response_receiver): (
            Sender<Result<(), wgpu::BufferAsyncError>>,
            Receiver<Result<(), wgpu::BufferAsyncError>>,
        ) = channel();
        Self {
            pipeline,
            bind_group,
            uniform_buffer,
            cellular_tile_buffer,
            staging_buffer,
            response_sender,
            response_receiver,
        }
    }

    /// Returns the occupancy of every active tile in row-major order, waiting for the GPU
    pub fn read_cellular_tiles(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
    ) -> Result<Vec<CollisionCellularTile>, SceneError> {
        let width: u32 = active_region.width() as u32;
        let height: u32 = active_region.height() as u32;
        let cellular_tile_count: u32 = width * height;
        let byte_count: u64 = u64::from(cellular_tile_count) * CELLULAR_TILE_OCCUPANCY_BYTE_COUNT;
        if byte_count > self.cellular_tile_buffer.size() {
            return Err(SceneError::InvalidCellularTileRegion);
        }
        let mut uniform_bytes: [u8; 16] = [0; 16];
        uniform_bytes[0..4].copy_from_slice(&width.to_le_bytes());
        uniform_bytes[4..8].copy_from_slice(&height.to_le_bytes());
        uniform_bytes[8..12].copy_from_slice(&ring_offset.0.to_le_bytes());
        uniform_bytes[12..16].copy_from_slice(&ring_offset.1.to_le_bytes());
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        // pack after every queued tile write, then copy the packed occupancies for the CPU
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene collision cellular tiles"),
            });
        {
            let mut compute_pass: wgpu::ComputePass<'_> = command_encoder
                .begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Scene collision cellular tiles"),
                    timestamp_writes: None,
                });
            compute_pass.set_pipeline(&self.pipeline);
            compute_pass.set_bind_group(0, &self.bind_group, &[]);
            compute_pass.dispatch_workgroups(cellular_tile_count.div_ceil(WORKGROUP_SIZE), 1, 1);
        }
        command_encoder.copy_buffer_to_buffer(
            &self.cellular_tile_buffer,
            0,
            &self.staging_buffer,
            0,
            byte_count,
        );
        accelerator.queue().submit(Some(command_encoder.finish()));
        // wait for this tick's occupancy so collision never lags the GPU cells
        let response_sender: Sender<Result<(), wgpu::BufferAsyncError>> =
            self.response_sender.clone();
        self.staging_buffer.slice(..byte_count).map_async(
            wgpu::MapMode::Read,
            move |result: Result<(), wgpu::BufferAsyncError>| {
                let _ = response_sender.send(result);
            },
        );
        accelerator.device().poll(wgpu::PollType::wait_indefinitely())
            .map_err(|_| SceneError::CollisionCellularTilePoll)?;
        let response: Result<(), wgpu::BufferAsyncError> = self.response_receiver.try_recv()
            .map_err(|_| SceneError::CollisionCellularTilePoll)?;
        response.map_err(|error| SceneError::CollisionCellularTileResponse { error })?;
        let cellular_tiles: Vec<CollisionCellularTile> = {
            let mapped_bytes: wgpu::BufferView = self.staging_buffer
                .get_mapped_range(..byte_count)
                .expect("mapped collision cellular tile buffer should be readable");
            mapped_bytes.chunks_exact(CELLULAR_TILE_OCCUPANCY_BYTE_COUNT as usize)
                .map(|tile_bytes: &[u8]| {
                    CollisionCellularTile::from_u64(u64::from_le_bytes([
                        tile_bytes[0],
                        tile_bytes[1],
                        tile_bytes[2],
                        tile_bytes[3],
                        tile_bytes[4],
                        tile_bytes[5],
                        tile_bytes[6],
                        tile_bytes[7],
                    ]))
                })
                .collect()
        };
        self.staging_buffer.unmap();
        Ok(cellular_tiles)
    }

}
