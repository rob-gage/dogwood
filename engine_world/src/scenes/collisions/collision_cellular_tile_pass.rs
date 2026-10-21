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

/// Settled dynamic occupancy snapshots that may be copied for the CPU at once
const DYNAMIC_READBACK_COUNT: usize = 3;

/// GPU extraction of static or settled dynamic occupancy of every active cellular tile
pub struct CollisionCellularTilePass {
    /// Pipeline packing one active tile's occupancy per invocation
    pipeline: wgpu::ComputePipeline,
    /// Bind group of the uniforms, materials, occupancies, rigid body cells, and kinematics
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
    /// Staging buffers of settled dynamic snapshots, each with the sequence number, active
    /// region, and ring offset of its pending or completed copy
    dynamic_readbacks: Vec<(wgpu::Buffer, Option<(u64, CellularTileRegion, (u32, u32), bool)>)>,
    /// Sequence number given to the next settled dynamic snapshot
    next_dynamic_sequence: u64,
    /// Sends the slot and result of each settled dynamic snapshot mapping
    dynamic_response_sender: Sender<(usize, Result<(), wgpu::BufferAsyncError>)>,
    /// Receives the slot and result of each settled dynamic snapshot mapping
    dynamic_response_receiver: Receiver<(usize, Result<(), wgpu::BufferAsyncError>)>,
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
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
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
            size: 32,
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
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: cellular_tile_buffers.kinematics_buffer().as_entire_binding(),
                },
            ],
        });
        let (response_sender, response_receiver): (
            Sender<Result<(), wgpu::BufferAsyncError>>,
            Receiver<Result<(), wgpu::BufferAsyncError>>,
        ) = channel();
        let dynamic_readbacks: Vec<(
            wgpu::Buffer,
            Option<(u64, CellularTileRegion, (u32, u32), bool)>,
        )> = (0..DYNAMIC_READBACK_COUNT)
            .map(|_| (
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("`CollisionCellularTilePass.dynamic_readbacks`"),
                    size: cellular_tile_buffer.size(),
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                None,
            ))
            .collect();
        let (dynamic_response_sender, dynamic_response_receiver): (
            Sender<(usize, Result<(), wgpu::BufferAsyncError>)>,
            Receiver<(usize, Result<(), wgpu::BufferAsyncError>)>,
        ) = channel();
        Self {
            pipeline,
            bind_group,
            uniform_buffer,
            cellular_tile_buffer,
            staging_buffer,
            response_sender,
            response_receiver,
            dynamic_readbacks,
            next_dynamic_sequence: 0,
            dynamic_response_sender,
            dynamic_response_receiver,
        }
    }

    /// Returns static or settled dynamic occupancy in row-major order, waiting for the GPU
    pub fn read_cellular_tiles(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        dynamic: bool,
    ) -> Result<Vec<CollisionCellularTile>, SceneError> {
        let byte_count: u64 =
            self.submit_cellular_tiles(accelerator, active_region, ring_offset, dynamic, None)?;
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
        let cellular_tiles: Vec<CollisionCellularTile> =
            Self::decode_cellular_tiles(&self.staging_buffer, byte_count);
        self.staging_buffer.unmap();
        Ok(cellular_tiles)
    }

    /// Copies the settled dynamic occupancy after the queued GPU work into a free snapshot slot
    /// without waiting, skipping the copy while every slot is in flight
    pub fn request_dynamic_cellular_tiles(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
    ) -> Result<(), SceneError> {
        let Some(slot): Option<usize> = self.dynamic_readbacks.iter()
            .position(|(_, submission)| submission.is_none())
        else { return Ok(()); };
        let byte_count: u64 = self.submit_cellular_tiles(
            accelerator,
            active_region,
            ring_offset,
            true,
            Some(slot),
        )?;
        let (buffer, submission): &mut (
            wgpu::Buffer,
            Option<(u64, CellularTileRegion, (u32, u32), bool)>,
        ) = &mut self.dynamic_readbacks[slot];
        *submission = Some((self.next_dynamic_sequence, active_region, ring_offset, false));
        self.next_dynamic_sequence += 1;
        let response_sender: Sender<(usize, Result<(), wgpu::BufferAsyncError>)> =
            self.dynamic_response_sender.clone();
        buffer.slice(..byte_count).map_async(
            wgpu::MapMode::Read,
            move |result: Result<(), wgpu::BufferAsyncError>| {
                let _ = response_sender.send((slot, result));
            },
        );
        Ok(())
    }

    /// Returns the newest completed settled dynamic snapshot of this active region and ring
    /// offset, if any, and releases every completed slot
    pub fn read_dynamic_cellular_tiles(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
    ) -> Result<Option<Vec<CollisionCellularTile>>, SceneError> {
        accelerator.device().poll(wgpu::PollType::Poll)
            .map_err(|_| SceneError::CollisionCellularTilePoll)?;
        while let Ok((slot, result)) = self.dynamic_response_receiver.try_recv() {
            result.map_err(|error| SceneError::CollisionCellularTileResponse { error })?;
            if let Some(submission) = self.dynamic_readbacks[slot].1.as_mut() {
                submission.3 = true;
            }
        }
        let newest: Option<usize> = self.dynamic_readbacks.iter()
            .enumerate()
            .filter_map(|(slot, (_, submission))| submission
                .filter(|(_, region, ring, is_complete)| *is_complete
                    && *region == active_region
                    && *ring == ring_offset)
                .map(|(sequence, _, _, _)| (sequence, slot)))
            .max()
            .map(|(_, slot)| slot);
        let byte_count: u64 = active_region.width() as u64 * active_region.height() as u64
            * CELLULAR_TILE_OCCUPANCY_BYTE_COUNT;
        let cellular_tiles: Option<Vec<CollisionCellularTile>> = newest.map(|slot|
            Self::decode_cellular_tiles(&self.dynamic_readbacks[slot].0, byte_count));
        for (buffer, submission) in &mut self.dynamic_readbacks {
            if submission.is_some_and(|(_, _, _, is_complete)| is_complete) {
                buffer.unmap();
                *submission = None;
            }
        }
        Ok(cellular_tiles)
    }

    /// Decodes packed occupancies from the front of a mapped staging buffer
    fn decode_cellular_tiles(buffer: &wgpu::Buffer, byte_count: u64) -> Vec<CollisionCellularTile> {
        let mapped_bytes: wgpu::BufferView = buffer
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
    }

    /// Packs static or settled dynamic occupancy after every queued tile write and copies it to
    /// the blocking staging buffer or a dynamic snapshot slot, returning the copied byte count
    fn submit_cellular_tiles(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        dynamic: bool,
        dynamic_slot: Option<usize>,
    ) -> Result<u64, SceneError> {
        let width: u32 = active_region.width() as u32;
        let height: u32 = active_region.height() as u32;
        let cellular_tile_count: u32 = width * height;
        let byte_count: u64 = u64::from(cellular_tile_count) * CELLULAR_TILE_OCCUPANCY_BYTE_COUNT;
        if byte_count > self.cellular_tile_buffer.size() {
            return Err(SceneError::InvalidCellularTileRegion);
        }
        let mut uniform_bytes: [u8; 32] = [0; 32];
        uniform_bytes[0..4].copy_from_slice(&width.to_le_bytes());
        uniform_bytes[4..8].copy_from_slice(&height.to_le_bytes());
        uniform_bytes[8..12].copy_from_slice(&ring_offset.0.to_le_bytes());
        uniform_bytes[12..16].copy_from_slice(&ring_offset.1.to_le_bytes());
        uniform_bytes[16..20].copy_from_slice(&u32::from(dynamic).to_le_bytes());
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
        let staging_buffer: &wgpu::Buffer = match dynamic_slot {
            Some(slot) => &self.dynamic_readbacks[slot].0,
            None => &self.staging_buffer,
        };
        command_encoder.copy_buffer_to_buffer(
            &self.cellular_tile_buffer,
            0,
            staging_buffer,
            0,
            byte_count,
        );
        accelerator.queue().submit(Some(command_encoder.finish()));
        Ok(byte_count)
    }

}
