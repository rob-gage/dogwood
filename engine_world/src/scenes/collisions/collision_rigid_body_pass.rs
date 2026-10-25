// Copyright Rob Gage 2026

use super::{
    COLLISION_RIGID_BODY_CELLULAR_TILE_COUNT_MAXIMUM,
    COLLISION_RIGID_BODY_COUNT_MAXIMUM,
};
use crate::{
    materials::MaterialIdentifier,
    scenes::{
        cellular_tiles::{
            CellularTileBuffers,
            CellularTileRegion,
        },
        SceneError,
    },
};
use engine_host::acceleration::{Accelerator, AcceleratorReadback, AcceleratorReadbackError};
use std::{
    collections::HashMap,
    sync::mpsc::{
        Receiver,
        Sender,
        channel,
    },
};

/// Number of invocations in one compute workgroup
const WORKGROUP_SIZE: u32 = 64;

/// Bytes between the uniforms of consecutive dispatches, the minimum uniform offset alignment
const UNIFORM_STRIDE: u64 = 256;

/// Bytes of one rigid body tile record
const CELLULAR_TILE_BYTE_COUNT: u64 = 32;

/// Bytes of one rigid body record
const RIGID_BODY_BYTE_COUNT: u64 = 64;

/// Largest number of cells written or read by one dispatch
const CELL_COUNT_MAXIMUM: usize = 65536;

/// Largest number of changed rigid body cells reported by one read
const CHANGED_CELL_COUNT_MAXIMUM: usize = 65536;

/// Uniform slot of the cell write dispatch
const SLOT_WRITE: u32 = 0;

/// Uniform slot of the dispatch returning rigid bodies to the grid
const SLOT_RETURN: u32 = 1;

/// Uniform slot of the dispatch clearing cells rigid bodies leave
const SLOT_CLEAR: u32 = 2;

/// Uniform slot of the rasterization dispatch
const SLOT_RASTERIZE: u32 = 3;

/// Uniform slot of the cell read dispatch
const SLOT_READ: u32 = 4;

/// Uniform slot of the dispatch reporting changed rigid body cells
const SLOT_CHANGE: u32 = 5;

/// GPU rasterization of rigid body cells into the cellular tile buffers
pub struct CollisionRigidBodyPass {
    /// Pipelines of every step, named by their entry points
    pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    /// Bind group of the uniforms, cells, rigid body cells, records, and cell lists
    bind_group: wgpu::BindGroup,
    /// Uniforms of every dispatch, each at its own dynamic offset
    uniform_buffer: wgpu::Buffer,
    /// Material identifier and packed color of every rigid body tile cell
    cellular_tile_cell_buffer: wgpu::Buffer,
    /// Stored pressure and integrity of every rigid body cell
    cellular_tile_state_buffer: wgpu::Buffer,
    /// Rigid body, tile coordinates, and occupied cells of every rigid body tile
    cellular_tile_buffer: wgpu::Buffer,
    /// Positions, velocities, and masses of every rigid body
    rigid_body_buffer: wgpu::Buffer,
    /// Rigid body tiles processed by each dispatch
    item_buffer: wgpu::Buffer,
    /// Cells written or read, with their new and expected material identifiers
    cell_write_buffer: wgpu::Buffer,
    /// Material identifiers and colors of read cells
    cell_read_buffer: wgpu::Buffer,
    /// Count and rigid body cells of every changed cell found under rigid bodies
    changed_cell_buffer: wgpu::Buffer,
    /// Mappable copy of the changed cells
    changed_cell_staging_buffer: wgpu::Buffer,
    /// Asynchronous cells read for detach groups, preserving their region and ring offset
    cell_readbacks: AcceleratorReadback<(CellularTileRegion, (u32, u32), Vec<Vec<(u32, u32)>>)>,
    /// Sender for staging buffer mapping results
    response_sender: Sender<Result<(), wgpu::BufferAsyncError>>,
    /// Staging buffer mapping results
    response_receiver: Receiver<Result<(), wgpu::BufferAsyncError>>,
}

impl CollisionRigidBodyPass {

    /// Creates rasterization resources for every rigid body tile the scene can hold
    pub fn new(accelerator: &Accelerator, cellular_tile_buffers: &CellularTileBuffers) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let storage_entry = |binding: u32, read_only: bool| -> wgpu::BindGroupLayoutEntry {
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
        };
        let bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`CollisionRigidBodyPass.bind_group_layout`"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: wgpu::BufferSize::new(48),
                        },
                        count: None,
                    },
                    storage_entry(1, false),
                    storage_entry(2, false),
                    storage_entry(3, false),
                    storage_entry(4, false),
                    storage_entry(5, false),
                    storage_entry(6, true),
                    storage_entry(7, false),
                    storage_entry(8, true),
                    storage_entry(9, true),
                    storage_entry(10, true),
                    storage_entry(11, false),
                    storage_entry(12, false),
                    storage_entry(13, false),
                    storage_entry(14, false),
                ],
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`CollisionRigidBodyPass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`CollisionRigidBodyPass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(include_str!("collision_rigid_body.wgsl").into()),
            },
        );
        let mut pipelines: HashMap<&'static str, wgpu::ComputePipeline> = HashMap::new();
        for entry_point in ["release", "rasterize", "write_cells", "read_cells", "copy_states"] {
            pipelines.insert(entry_point, device.create_compute_pipeline(
                &wgpu::ComputePipelineDescriptor {
                    label: Some("`CollisionRigidBodyPass.pipelines`"),
                    layout: Some(&pipeline_layout),
                    module: &shader_module,
                    entry_point: Some(entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    cache: None,
                },
            ));
        }
        let create_buffer = |label: &'static str, size: u64, usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size.max(16),
                usage,
                mapped_at_creation: false,
            })
        };
        let storage: wgpu::BufferUsages = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST;
        let cellular_tile_count: u64 = COLLISION_RIGID_BODY_CELLULAR_TILE_COUNT_MAXIMUM as u64;
        let uniform_buffer: wgpu::Buffer = create_buffer(
            "`CollisionRigidBodyPass.uniform_buffer`",
            UNIFORM_STRIDE * (u64::from(SLOT_CHANGE) + 1),
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let cellular_tile_cell_buffer: wgpu::Buffer = create_buffer(
            "`CollisionRigidBodyPass.cellular_tile_cell_buffer`",
            cellular_tile_count * 64 * 8,
            storage,
        );
        let cellular_tile_state_buffer: wgpu::Buffer = create_buffer(
            "`CollisionRigidBodyPass.cellular_tile_state_buffer`",
            cellular_tile_count * 64 * 8,
            storage,
        );
        let cellular_tile_buffer: wgpu::Buffer = create_buffer(
            "`CollisionRigidBodyPass.cellular_tile_buffer`",
            cellular_tile_count * CELLULAR_TILE_BYTE_COUNT,
            storage,
        );
        let rigid_body_buffer: wgpu::Buffer = create_buffer(
            "`CollisionRigidBodyPass.rigid_body_buffer`",
            COLLISION_RIGID_BODY_COUNT_MAXIMUM as u64 * RIGID_BODY_BYTE_COUNT,
            storage,
        );
        // every tile may be returned, cleared, and rasterized in one submission
        let item_buffer: wgpu::Buffer = create_buffer(
            "`CollisionRigidBodyPass.item_buffer`",
            cellular_tile_count * 3 * 4,
            storage,
        );
        let cell_write_buffer: wgpu::Buffer = create_buffer(
            "`CollisionRigidBodyPass.cell_write_buffer`",
            CELL_COUNT_MAXIMUM as u64 * 16,
            storage,
        );
        let cell_read_buffer: wgpu::Buffer = create_buffer(
            "`CollisionRigidBodyPass.cell_read_buffer`",
            CELL_COUNT_MAXIMUM as u64 * 16,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        );
        let changed_cell_buffer: wgpu::Buffer = create_buffer(
            "`CollisionRigidBodyPass.changed_cell_buffer`",
            (CHANGED_CELL_COUNT_MAXIMUM as u64 + 1) * 4,
            storage | wgpu::BufferUsages::COPY_SRC,
        );
        let changed_cell_staging_buffer: wgpu::Buffer = create_buffer(
            "`CollisionRigidBodyPass.changed_cell_staging_buffer`",
            changed_cell_buffer.size(),
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        );
        let cell_readbacks: AcceleratorReadback<(
            CellularTileRegion,
            (u32, u32),
            Vec<Vec<(u32, u32)>>,
        )> = AcceleratorReadback::new(
            accelerator,
            "`CollisionRigidBodyPass.cell_readbacks`",
            cell_read_buffer.size(),
            3,
        );
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`CollisionRigidBodyPass.bind_group`"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniform_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(48),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: cellular_tile_buffers.material_identifier_buffer()
                        .as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: cellular_tile_buffers.color_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: cellular_tile_buffers.kinematics_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: cellular_tile_buffers.awake_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: cellular_tile_buffers.rigid_body_cell_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: cellular_tile_cell_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: cellular_tile_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: rigid_body_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry { binding: 9, resource: item_buffer.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: cell_write_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: cell_read_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: changed_cell_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: cellular_tile_state_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: cellular_tile_buffers.gas_buffer().as_entire_binding(),
                },
            ],
        });
        let (response_sender, response_receiver): (
            Sender<Result<(), wgpu::BufferAsyncError>>,
            Receiver<Result<(), wgpu::BufferAsyncError>>,
        ) = channel();
        Self {
            pipelines,
            bind_group,
            uniform_buffer,
            cellular_tile_cell_buffer,
            cellular_tile_state_buffer,
            cellular_tile_buffer,
            rigid_body_buffer,
            item_buffer,
            cell_write_buffer,
            cell_read_buffer,
            changed_cell_buffer,
            changed_cell_staging_buffer,
            cell_readbacks,
            response_sender,
            response_receiver,
        }
    }

    /// Returns the material identifier and packed color of every rigid body tile cell
    pub fn cellular_tile_cell_buffer(&self) -> &wgpu::Buffer { &self.cellular_tile_cell_buffer }

    /// Returns the record of every rigid body tile
    pub fn cellular_tile_buffer(&self) -> &wgpu::Buffer { &self.cellular_tile_buffer }

    /// Returns the record of every rigid body
    pub fn rigid_body_buffer(&self) -> &wgpu::Buffer { &self.rigid_body_buffer }

    /// Writes one rigid body tile's rigid body, coordinates in its body, cells, cells last
    /// rasterized, and the material identifiers and colors of its cells
    pub fn write_cellular_tile(
        &self,
        accelerator: &Accelerator,
        cellular_tile: u32,
        rigid_body: u32,
        coordinates: (i32, i32),
        cells: u64,
        previous_cells: u64,
        material_identifiers: &[MaterialIdentifier; 64],
        colors: &[u32; 64],
        states: &[[u32; 2]; 64],
    ) {
        let mut record_bytes: Vec<u8> = Vec::with_capacity(CELLULAR_TILE_BYTE_COUNT as usize);
        record_bytes.extend_from_slice(&rigid_body.to_le_bytes());
        record_bytes.extend_from_slice(&0_u32.to_le_bytes());
        record_bytes.extend_from_slice(&coordinates.0.to_le_bytes());
        record_bytes.extend_from_slice(&coordinates.1.to_le_bytes());
        record_bytes.extend_from_slice(&cells.to_le_bytes());
        record_bytes.extend_from_slice(&previous_cells.to_le_bytes());
        accelerator.queue().write_buffer(
            &self.cellular_tile_buffer,
            u64::from(cellular_tile) * CELLULAR_TILE_BYTE_COUNT,
            &record_bytes,
        );
        let mut cell_bytes: Vec<u8> = Vec::with_capacity(64 * 8);
        for (material_identifier, color) in material_identifiers.iter().zip(colors) {
            cell_bytes.extend_from_slice(&material_identifier.as_u32().to_le_bytes());
            cell_bytes.extend_from_slice(&color.to_le_bytes());
        }
        accelerator.queue().write_buffer(
            &self.cellular_tile_cell_buffer,
            u64::from(cellular_tile) * 64 * 8,
            &cell_bytes,
        );
        let state_bytes: Vec<u8> = states.iter().flatten().copied()
            .flat_map(u32::to_le_bytes).collect();
        accelerator.queue().write_buffer(
            &self.cellular_tile_state_buffer,
            u64::from(cellular_tile) * 64 * 8,
            &state_bytes,
        );
    }

    /// Writes rigid body records: position, rotation cosine and sine, the position and rotation
    /// they were last rasterized at, velocity, and center of mass in cells from the collision
    /// origin, then angular velocity in radians per tick and mass
    pub fn write_rigid_bodies(&self, accelerator: &Accelerator, rigid_bodies: &[(u32, [f32; 14])]) {
        for (rigid_body, values) in rigid_bodies {
            let mut record_bytes: [u8; RIGID_BODY_BYTE_COUNT as usize] =
                [0; RIGID_BODY_BYTE_COUNT as usize];
            for (field, value) in values.iter().enumerate() {
                record_bytes[field * 4..field * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
            accelerator.queue().write_buffer(
                &self.rigid_body_buffer,
                u64::from(*rigid_body) * RIGID_BODY_BYTE_COUNT,
                &record_bytes,
            );
        }
    }

    /// Copies the current state of cells moving into newly allocated rigid body tiles
    pub fn copy_cell_states(
        &self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        cell_copies: &[(u32, u32)],
    ) {
        if cell_copies.is_empty() { return; }
        let copy_count_maximum: usize = self.item_buffer.size() as usize / 8;
        for cell_copies in cell_copies.chunks(copy_count_maximum) {
            let item_bytes: Vec<u8> = cell_copies.iter().flat_map(|(old_cell, new_cell)| {
                [*old_cell, *new_cell]
            }).flat_map(u32::to_le_bytes).collect();
            accelerator.queue().write_buffer(&self.item_buffer, 0, &item_bytes);
            self.write_uniforms(
                accelerator,
                SLOT_READ,
                active_region,
                ring_offset,
                (0, 0),
                0,
                cell_copies.len(),
            );
            let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Scene collision rigid body cell state copies"),
                });
            {
                let mut compute_pass: wgpu::ComputePass<'_> = self.begin_compute_pass(
                    &mut command_encoder,
                );
                self.dispatch(
                    &mut compute_pass,
                    "copy_states",
                    SLOT_READ,
                    cell_copies.len(),
                    false,
                );
            }
            accelerator.queue().submit(Some(command_encoder.finish()));
        }
    }

    /// Dispatches reads of the leading groups of cells that fit in one read and maps them for the
    /// next tick without waiting for the GPU, returning the groups left for a later tick
    pub fn request_cell_reads(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        mut groups: Vec<Vec<(u32, u32)>>,
    ) -> Vec<Vec<(u32, u32)>> {
        let mut cell_count: usize = 0;
        let read_group_count: usize = groups.iter()
            .take_while(|group| {
                cell_count += group.len();
                cell_count <= CELL_COUNT_MAXIMUM
            })
            .count();
        let remaining_groups: Vec<Vec<(u32, u32)>> = groups.split_off(read_group_count);
        let cells: Vec<(u32, u32)> = groups.iter().flatten().copied().collect();
        if cells.is_empty() { return remaining_groups; }
        let cell_write_bytes: Vec<u8> = cells.iter()
            .flat_map(|(x, y)| [*x, *y, 0, 0])
            .flat_map(u32::to_le_bytes)
            .collect();
        accelerator.queue().write_buffer(&self.cell_write_buffer, 0, &cell_write_bytes);
        self.write_uniforms(
            accelerator,
            SLOT_READ,
            active_region,
            ring_offset,
            (0, 0),
            0,
            cells.len(),
        );
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene collision rigid body cell reads"),
            });
        {
            let mut compute_pass: wgpu::ComputePass<'_> = self.begin_compute_pass(
                &mut command_encoder,
            );
            self.dispatch(&mut compute_pass, "read_cells", SLOT_READ, cells.len(), false);
        }
        let byte_count: u64 = cells.len() as u64 * 16;
        self.cell_readbacks.copy(
            &mut command_encoder,
            &self.cell_read_buffer,
            0,
            byte_count,
            (active_region, ring_offset, groups),
        );
        let submission_index: wgpu::SubmissionIndex =
            accelerator.queue().submit(Some(command_encoder.finish()));
        self.cell_readbacks.map(submission_index);
        remaining_groups
    }

    /// Returns every completed group of cell reads, waiting only for its own submission
    pub fn read_cell_reads(
        &mut self,
        accelerator: &Accelerator,
    ) -> Result<Vec<(
        CellularTileRegion,
        (u32, u32),
        Vec<Vec<((u32, u32), (MaterialIdentifier, u32, [u32; 2]))>>,
    )>, SceneError> {
        let mut cell_groups: Vec<(
            CellularTileRegion,
            (u32, u32),
            Vec<Vec<((u32, u32), (MaterialIdentifier, u32, [u32; 2]))>>,
        )> = Vec::new();
        let read_result: Result<(), AcceleratorReadbackError> = self.cell_readbacks.read(
            accelerator,
            true,
            |(active_region, ring_offset, groups), bytes| {
                let mut cell_offset: usize = 0;
                let mut read_groups: Vec<
                    Vec<((u32, u32), (MaterialIdentifier, u32, [u32; 2]))>,
                > = Vec::new();
                for group in groups {
                    let mut read_group: Vec<((u32, u32), (MaterialIdentifier, u32, [u32; 2]))> =
                        Vec::new();
                    for cell in group {
                        let start: usize = cell_offset * 16;
                        let read: &[u8] = &bytes[start..start + 16];
                        read_group.push((cell, (
                            MaterialIdentifier::from_u32(
                                u32::from_le_bytes(read[0..4].try_into().unwrap()),
                            ),
                            u32::from_le_bytes(read[4..8].try_into().unwrap()),
                            [
                                u32::from_le_bytes(read[8..12].try_into().unwrap()),
                                u32::from_le_bytes(read[12..16].try_into().unwrap()),
                            ],
                        )));
                        cell_offset += 1;
                    }
                    read_groups.push(read_group);
                }
                cell_groups.push((active_region, ring_offset, read_groups));
            },
        );
        read_result.map_err(|error| match error {
            AcceleratorReadbackError::Poll(_) => SceneError::CollisionCellularTilePoll,
            AcceleratorReadbackError::BufferAsync(error) =>
                SceneError::CollisionCellularTileResponse { error },
        })?;
        Ok(cell_groups)
    }

    /// Detaches every cell under the listed rigid body tiles that no longer holds its rigid body
    /// cell's material, returning those rigid body cells and whether more were found than
    /// returned, waiting for the GPU
    pub fn read_changed_cells(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        region_origin: (i32, i32),
        cellular_tiles: &[u32],
    ) -> Result<(Vec<u32>, bool), SceneError> {
        let item_bytes: Vec<u8> =
            cellular_tiles.iter().copied().flat_map(u32::to_le_bytes).collect();
        if !item_bytes.is_empty() {
            accelerator.queue().write_buffer(&self.item_buffer, 0, &item_bytes);
        }
        self.write_uniforms(
            accelerator,
            SLOT_CHANGE,
            active_region,
            ring_offset,
            region_origin,
            0,
            cellular_tiles.len(),
        );
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene collision rigid body changed cells"),
            });
        command_encoder.clear_buffer(&self.changed_cell_buffer, 0, Some(4));
        {
            let mut compute_pass: wgpu::ComputePass<'_> =
                self.begin_compute_pass(&mut command_encoder);
            self.dispatch(&mut compute_pass, "release", SLOT_CHANGE, cellular_tiles.len(), true);
        }
        command_encoder.copy_buffer_to_buffer(
            &self.changed_cell_buffer,
            0,
            &self.changed_cell_staging_buffer,
            0,
            self.changed_cell_buffer.size(),
        );
        accelerator.queue().submit(Some(command_encoder.finish()));
        let changed_cell_bytes: Vec<u8> = self.read_staging_buffer(
            accelerator,
            &self.changed_cell_staging_buffer,
            self.changed_cell_buffer.size(),
        )?;
        let changed_cell_count: usize = u32::from_le_bytes([
            changed_cell_bytes[0],
            changed_cell_bytes[1],
            changed_cell_bytes[2],
            changed_cell_bytes[3],
        ]) as usize;
        let changed_cells: Vec<u32> = changed_cell_bytes[4..]
            .chunks_exact(4)
            .take(changed_cell_count.min(CHANGED_CELL_COUNT_MAXIMUM))
            .map(|bytes| u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            .collect();
        Ok((changed_cells, changed_cell_count > CHANGED_CELL_COUNT_MAXIMUM))
    }

    /// Writes listed cells whose material is still the expected one, then returns rigid body
    /// tiles to the grid, clears where moving tiles were, and rasterizes tiles where they are,
    /// without waiting for the GPU
    pub fn rasterize_rigid_bodies(
        &self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        region_origin: (i32, i32),
        cell_writes: &[[u32; 4]],
        returned_cellular_tiles: &[u32],
        cleared_cellular_tiles: &[u32],
        rasterized_cellular_tiles: &[u32],
    ) {
        // queued writes take effect in submission order, so each chunk of cell writes submits
        // before the next overwrites the list
        for cell_writes in cell_writes.chunks(CELL_COUNT_MAXIMUM) {
            let cell_write_bytes: Vec<u8> =
                cell_writes.iter().flatten().copied().flat_map(u32::to_le_bytes).collect();
            accelerator.queue().write_buffer(&self.cell_write_buffer, 0, &cell_write_bytes);
            self.write_uniforms(
                accelerator,
                SLOT_WRITE,
                active_region,
                ring_offset,
                region_origin,
                0,
                cell_writes.len(),
            );
            let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Scene collision rigid body cell writes"),
                });
            {
                let mut compute_pass: wgpu::ComputePass<'_> =
                    self.begin_compute_pass(&mut command_encoder);
                self.dispatch(
                    &mut compute_pass,
                    "write_cells",
                    SLOT_WRITE,
                    cell_writes.len(),
                    false,
                );
            }
            accelerator.queue().submit(Some(command_encoder.finish()));
        }
        let item_count: usize = returned_cellular_tiles.len()
            + cleared_cellular_tiles.len()
            + rasterized_cellular_tiles.len();
        if item_count == 0 { return; }
        let item_bytes: Vec<u8> = returned_cellular_tiles.iter()
            .chain(cleared_cellular_tiles)
            .chain(rasterized_cellular_tiles)
            .copied()
            .flat_map(u32::to_le_bytes)
            .collect();
        accelerator.queue().write_buffer(&self.item_buffer, 0, &item_bytes);
        let mut item_offset: usize = 0;
        let mut dispatches: Vec<(&'static str, u32, usize)> = Vec::new();
        for (entry_point, slot, cellular_tiles) in [
            ("release", SLOT_RETURN, returned_cellular_tiles),
            ("release", SLOT_CLEAR, cleared_cellular_tiles),
            ("rasterize", SLOT_RASTERIZE, rasterized_cellular_tiles),
        ] {
            self.write_uniforms(
                accelerator,
                slot,
                active_region,
                ring_offset,
                region_origin,
                item_offset,
                cellular_tiles.len(),
            );
            dispatches.push((entry_point, slot, cellular_tiles.len()));
            item_offset += cellular_tiles.len();
        }
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene collision rigid bodies"),
            });
        {
            let mut compute_pass: wgpu::ComputePass<'_> =
                self.begin_compute_pass(&mut command_encoder);
            // every tile leaves before any is rasterized, so moving bodies never block themselves
            for (entry_point, slot, count) in dispatches {
                self.dispatch(&mut compute_pass, entry_point, slot, count, true);
            }
        }
        accelerator.queue().submit(Some(command_encoder.finish()));
    }

    /// Writes the uniforms of one dispatch slot
    fn write_uniforms(
        &self,
        accelerator: &Accelerator,
        slot: u32,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        region_origin: (i32, i32),
        item_offset: usize,
        item_count: usize,
    ) {
        let mode: u32 = match slot {
            SLOT_RETURN => 1,
            SLOT_CHANGE => 2,
            _ => 0,
        };
        let mut uniform_bytes: [u8; 48] = [0; 48];
        for (field, value) in [
            active_region.width() as u32,
            active_region.height() as u32,
            ring_offset.0,
            ring_offset.1,
            region_origin.0 as u32,
            region_origin.1 as u32,
            item_offset as u32,
            item_count as u32,
            mode,
        ].into_iter().enumerate() {
            uniform_bytes[field * 4..field * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        accelerator.queue().write_buffer(
            &self.uniform_buffer,
            u64::from(slot) * UNIFORM_STRIDE,
            &uniform_bytes,
        );
    }

    /// Begins a compute pass of rigid body steps
    fn begin_compute_pass<'a>(
        &self,
        command_encoder: &'a mut wgpu::CommandEncoder,
    ) -> wgpu::ComputePass<'a> {
        command_encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Scene collision rigid bodies"),
            timestamp_writes: None,
        })
    }

    /// Dispatches one step over a number of items, one workgroup per rigid body tile or one
    /// invocation per cell
    fn dispatch(
        &self,
        compute_pass: &mut wgpu::ComputePass<'_>,
        entry_point: &str,
        slot: u32,
        item_count: usize,
        is_per_cellular_tile: bool,
    ) {
        if item_count == 0 { return; }
        compute_pass.set_pipeline(&self.pipelines[entry_point]);
        compute_pass.set_bind_group(
            0,
            &self.bind_group,
            &[(u64::from(slot) * UNIFORM_STRIDE) as u32],
        );
        let workgroup_count: u32 = if is_per_cellular_tile {
            item_count as u32
        } else {
            (item_count as u32).div_ceil(WORKGROUP_SIZE)
        };
        compute_pass.dispatch_workgroups(workgroup_count, 1, 1);
    }

    /// Returns the first bytes of a staging buffer once the GPU has written it
    fn read_staging_buffer(
        &self,
        accelerator: &Accelerator,
        staging_buffer: &wgpu::Buffer,
        byte_count: u64,
    ) -> Result<Vec<u8>, SceneError> {
        let response_sender: Sender<Result<(), wgpu::BufferAsyncError>> =
            self.response_sender.clone();
        staging_buffer.slice(..byte_count).map_async(
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
        let bytes: Vec<u8> = staging_buffer.get_mapped_range(..byte_count)
            .expect("mapped collision rigid body staging buffer should be readable")
            .to_vec();
        staging_buffer.unmap();
        Ok(bytes)
    }

}
