// Copyright Rob Gage 2026

use super::EDIT_CELLULAR_TILE_COUNT_MAXIMUM;
use crate::{
    materials::MaterialIdentifier,
    scenes::cellular_tiles::{
        CellularTileBuffers,
        CellularTileIndex,
    },
};
use engine_host::acceleration::Accelerator;

/// Bytes of one edited tile: its index, padding, both cell masks, materials, and colors
const EDIT_CELLULAR_TILE_BYTE_COUNT: u64 = (6 + 64 + 64) * 4;

/// GPU writing of edited cells into the active cellular tiles
pub struct EditCellularTilePass {
    /// Pipeline writing one edited cell per invocation
    pipeline: wgpu::ComputePipeline,
    /// Bind group of the edited tiles, material identifiers, colors, kinematics, and awake tiles
    bind_group: wgpu::BindGroup,
    /// Edited tiles uploaded for one stream
    edit_cellular_tile_buffer: wgpu::Buffer,
}

impl EditCellularTilePass {

    /// Creates edit resources writing into the cellular tile buffers
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
                label: Some("`EditCellularTilePass.bind_group_layout`"),
                entries: &[
                    storage_entry(0, true),
                    storage_entry(1, false),
                    storage_entry(2, false),
                    storage_entry(3, false),
                    storage_entry(4, false),
                ],
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`EditCellularTilePass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`EditCellularTilePass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(include_str!("edit_cellular_tile.wgsl").into()),
            },
        );
        let pipeline: wgpu::ComputePipeline = device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor {
                label: Some("`EditCellularTilePass.pipeline`"),
                layout: Some(&pipeline_layout),
                module: &shader_module,
                entry_point: Some("compute"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            },
        );
        let edit_cellular_tile_buffer: wgpu::Buffer = device.create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("`EditCellularTilePass.edit_cellular_tile_buffer`"),
                size: EDIT_CELLULAR_TILE_COUNT_MAXIMUM as u64 * EDIT_CELLULAR_TILE_BYTE_COUNT,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`EditCellularTilePass.bind_group`"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: edit_cellular_tile_buffer.as_entire_binding(),
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
            ],
        });
        Self { pipeline, bind_group, edit_cellular_tile_buffer }
    }

    /// Writes inserted and replaced cells into their tiles after every queued tile write
    pub fn write_cellular_tiles(
        &self,
        accelerator: &Accelerator,
        edit_cellular_tiles: &[(CellularTileIndex, u64, u64, [MaterialIdentifier; 64], [u32; 64])],
    ) {
        if edit_cellular_tiles.is_empty() { return; }
        // WGSL u32, vec2<u32>, vec2<u32>, and two array<u32, 64> at offsets 0, 8, 16, 24, and 280
        let mut edit_cellular_tile_bytes: Vec<u8> = Vec::with_capacity(
            edit_cellular_tiles.len() * EDIT_CELLULAR_TILE_BYTE_COUNT as usize,
        );
        for (cellular_tile_index, inserted_cells, replaced_cells, material_identifiers, colors)
            in edit_cellular_tiles
        {
            let cellular_tile_index: u32 = (*cellular_tile_index).into();
            edit_cellular_tile_bytes.extend_from_slice(&cellular_tile_index.to_le_bytes());
            edit_cellular_tile_bytes.extend_from_slice(&[0; 4]);
            edit_cellular_tile_bytes.extend_from_slice(&inserted_cells.to_le_bytes());
            edit_cellular_tile_bytes.extend_from_slice(&replaced_cells.to_le_bytes());
            for material_identifier in material_identifiers {
                edit_cellular_tile_bytes
                    .extend_from_slice(&material_identifier.as_u32().to_le_bytes());
            }
            for color in colors {
                edit_cellular_tile_bytes.extend_from_slice(&color.to_le_bytes());
            }
        }
        accelerator.queue().write_buffer(
            &self.edit_cellular_tile_buffer,
            0,
            &edit_cellular_tile_bytes,
        );
        // one workgroup writes the 64 cells of each edited tile
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene edit cellular tiles"),
            });
        {
            let mut compute_pass: wgpu::ComputePass<'_> = command_encoder
                .begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Scene edit cellular tiles"),
                    timestamp_writes: None,
                });
            compute_pass.set_pipeline(&self.pipeline);
            compute_pass.set_bind_group(0, &self.bind_group, &[]);
            compute_pass.dispatch_workgroups(edit_cellular_tiles.len() as u32, 1, 1);
        }
        accelerator.queue().submit(Some(command_encoder.finish()));
    }

}
