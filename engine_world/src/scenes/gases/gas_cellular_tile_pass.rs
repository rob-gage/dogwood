// Copyright Rob Gage 2026

use super::GAS_SUBSTEP_COUNT;
use crate::{
    materials::MaterialTable,
    scenes::{
        cellular_tiles::{
            CellularTileBuffers,
            CellularTileRegion,
            CELLULAR_TILE_SIZE,
        },
        fluids::FLUID_RASTER_SCALE,
        SceneError,
    },
};
use engine_host::acceleration::Accelerator;
use std::collections::HashMap;

/// Number of invocations in one compute workgroup, one per cell of a tile
const WORKGROUP_SIZE: u32 = 64;

/// Bytes of the uniforms
const UNIFORM_BYTE_COUNT: u64 = 48;

/// GPU simulation of the gas in the active tiles, alternating between the cellular tile gas buffer
/// and a scratch buffer every substep
pub struct GasCellularTilePass {
    /// Pipelines of every simulation step, named by their entry points
    pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    /// Bind groups reading the cellular tile gas and writing the scratch gas, then the reverse
    bind_groups: [wgpu::BindGroup; 2],
    /// Bind group of the indirect dispatch written by the prepare step
    dispatch_bind_group: wgpu::BindGroup,
    /// Uniforms of every step
    uniform_buffer: wgpu::Buffer,
    /// Active tile count and the active tiles
    control_buffer: wgpu::Buffer,
    /// Indirect dispatch of one workgroup per active tile
    dispatch_buffer: wgpu::Buffer,
}

impl GasCellularTilePass {

    /// Creates simulation resources for every tile the cellular tile buffers can hold, with
    /// actors displacing gas read from the cellular tile actor records
    pub fn new(
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        material_table: &MaterialTable,
        fluid_raster_buffer: &wgpu::Buffer,
        actor_buffer: &wgpu::Buffer,
    ) -> Self {
        let device: &wgpu::Device = accelerator.device();
        let cell_count: u64 = cellular_tile_buffers.material_identifier_buffer().size() / 4;
        let tile_count: u64 = cell_count / (CELLULAR_TILE_SIZE * CELLULAR_TILE_SIZE) as u64;
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
                label: Some("`GasCellularTilePass.bind_group_layout`"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(UNIFORM_BYTE_COUNT),
                        },
                        count: None,
                    },
                    storage_entry(1, true),
                    storage_entry(2, true),
                    storage_entry(3, false),
                    storage_entry(4, false),
                    storage_entry(5, false),
                    storage_entry(6, false),
                    storage_entry(7, true),
                    storage_entry(8, true),
                    storage_entry(9, true),
                    storage_entry(10, true),
                ],
            },
        );
        let dispatch_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`GasCellularTilePass.dispatch_bind_group_layout`"),
                entries: &[storage_entry(0, false)],
            },
        );
        // only the prepare step writes the indirect dispatch the other steps read
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`GasCellularTilePass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let prepare_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`GasCellularTilePass.prepare_pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(&dispatch_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`GasCellularTilePass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(include_str!("gas_cellular_tile.wgsl").into()),
            },
        );
        let mut pipelines: HashMap<&'static str, wgpu::ComputePipeline> = HashMap::new();
        for entry_point in ["volume", "collect", "clear", "prepare", "step"] {
            let layout: &wgpu::PipelineLayout = if entry_point == "prepare" {
                &prepare_pipeline_layout
            } else {
                &pipeline_layout
            };
            pipelines.insert(entry_point, device.create_compute_pipeline(
                &wgpu::ComputePipelineDescriptor {
                    label: Some("`GasCellularTilePass.pipelines`"),
                    layout: Some(layout),
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
        let uniform_buffer: wgpu::Buffer = create_buffer(
            "`GasCellularTilePass.uniform_buffer`",
            UNIFORM_BYTE_COUNT,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        // the gas of odd substeps, laid out like the cellular tile gas
        let scratch_buffer: wgpu::Buffer = create_buffer(
            "`GasCellularTilePass.scratch_buffer`",
            cellular_tile_buffers.gas_buffer().size(),
            wgpu::BufferUsages::STORAGE,
        );
        // each cell's open volume and the velocity of the actor dragging its gas, measured once
        // per tick
        let volume_buffer: wgpu::Buffer = create_buffer(
            "`GasCellularTilePass.volume_buffer`",
            cell_count * 8,
            wgpu::BufferUsages::STORAGE,
        );
        let control_buffer: wgpu::Buffer = create_buffer(
            "`GasCellularTilePass.control_buffer`",
            4 + tile_count * 4,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let dispatch_buffer: wgpu::Buffer = create_buffer(
            "`GasCellularTilePass.dispatch_buffer`",
            12,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
        );
        let create_bind_group = |gas_in: &wgpu::Buffer, gas_out: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("`GasCellularTilePass.bind_groups`"),
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
                    wgpu::BindGroupEntry { binding: 2, resource: gas_in.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: gas_out.as_entire_binding() },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: cellular_tile_buffers.gas_awake_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: volume_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: control_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: material_table.gas_properties_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,
                        resource: fluid_raster_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 9,
                        resource: material_table.fluid_properties_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 10,
                        resource: actor_buffer.as_entire_binding(),
                    },
                ],
            })
        };
        let bind_groups: [wgpu::BindGroup; 2] = [
            create_bind_group(cellular_tile_buffers.gas_buffer(), &scratch_buffer),
            create_bind_group(&scratch_buffer, cellular_tile_buffers.gas_buffer()),
        ];
        let dispatch_bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("`GasCellularTilePass.dispatch_bind_group`"),
                layout: &dispatch_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: dispatch_buffer.as_entire_binding(),
                }],
            },
        );
        Self {
            pipelines,
            bind_groups,
            dispatch_bind_group,
            uniform_buffer,
            control_buffer,
            dispatch_buffer,
        }
    }

    /// Simulates one tick of the gas in the active tiles without waiting for the GPU, with gravity
    /// in cells per substep squared and the first actor records displacing gas
    pub fn simulate_cellular_tiles(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        gravity: f32,
        ambient_amount: u32,
        ambient_mass: f32,
        actor_count: u32,
    ) -> Result<(), SceneError> {
        let width: u32 = active_region.width() as u32;
        let height: u32 = active_region.height() as u32;
        let tile_count: u32 = width * height;
        if u64::from(tile_count) * 4 + 4 > self.control_buffer.size() {
            return Err(SceneError::InvalidCellularTileRegion);
        }
        let mut uniform_bytes: Vec<u8> = Vec::with_capacity(UNIFORM_BYTE_COUNT as usize);
        for value in [
            width,
            height,
            ring_offset.0,
            ring_offset.1,
            ambient_amount,
            ambient_mass.to_bits(),
            gravity.to_bits(),
            FLUID_RASTER_SCALE.to_bits(),
            actor_count,
            (1.0 / GAS_SUBSTEP_COUNT as f32).to_bits(),
            0,
            0,
        ] {
            uniform_bytes.extend_from_slice(&value.to_le_bytes());
        }
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene gas cellular tiles"),
            });
        command_encoder.clear_buffer(&self.control_buffer, 0, Some(4));
        {
            let mut compute_pass: wgpu::ComputePass<'_> = command_encoder
                .begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Scene gas cellular tiles"),
                    timestamp_writes: None,
                });
            let pipelines: &HashMap<&'static str, wgpu::ComputePipeline> = &self.pipelines;
            compute_pass.set_bind_group(0, &self.bind_groups[0], &[]);
            // measure open volumes everywhere so changed cells wake their tiles, then list the
            // active tiles and clear their flags for this tick's simulation to set
            compute_pass.set_pipeline(&pipelines["volume"]);
            compute_pass.dispatch_workgroups(tile_count, 1, 1);
            compute_pass.set_pipeline(&pipelines["collect"]);
            compute_pass.dispatch_workgroups(tile_count.div_ceil(WORKGROUP_SIZE), 1, 1);
            compute_pass.set_pipeline(&pipelines["clear"]);
            compute_pass.dispatch_workgroups(tile_count.div_ceil(WORKGROUP_SIZE), 1, 1);
            compute_pass.set_pipeline(&pipelines["prepare"]);
            compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            for substep in 0..GAS_SUBSTEP_COUNT {
                compute_pass.set_bind_group(0, &self.bind_groups[substep % 2], &[]);
                compute_pass.set_pipeline(&pipelines["step"]);
                compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            }
        }
        accelerator.queue().submit(Some(command_encoder.finish()));
        Ok(())
    }

}
