// Copyright Rob Gage 2026

use super::{
    THERMAL_CONVERSION_COUNT_MAXIMUM,
    THERMAL_FLUID_CAPACITY_SCALE,
    THERMAL_FLUID_CONDUCTION_SCALE,
    THERMAL_FLUID_ENERGY_SCALE,
    THERMAL_HEAT_SCALE,
    THERMAL_SLEEP_TEMPERATURE,
    THERMAL_TIME_STEP,
};
use crate::{
    materials::MaterialTable,
    scenes::cellular_tiles::{
        CellularTileBuffers,
        CellularTileRegion,
        CELLULAR_TILE_SIZE,
    },
    scenes::SceneError,
};
use engine_host::acceleration::{Accelerator, AcceleratorReadback, AcceleratorReadbackError};
use engine_graphics::world::materials::MATERIAL_RADIANCE_WGSL;
use std::collections::HashMap;

/// Number of invocations in one compute workgroup, one per cell of a tile
const WORKGROUP_SIZE: u32 = 64;

/// Bytes of the uniforms
const UNIFORM_BYTE_COUNT: u64 = 80;

/// Number of asynchronous conversion count readbacks
const CONVERSION_READBACK_COUNT: usize = 3;

/// GPU conduction of heat through the cells of the active tiles, one step per tick over the tiles
/// whose temperatures change and their neighbors
pub struct ThermalCellularTilePass {
    /// Pipelines of every step, named by their entry points
    pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    /// Bind group of the uniforms, cells, fluid heat, and steps
    bind_group: wgpu::BindGroup,
    /// Bind group of the indirect dispatch written by the prepare step
    dispatch_bind_group: wgpu::BindGroup,
    /// Uniforms of every step
    uniform_buffer: wgpu::Buffer,
    /// Active tile count, then the buffer index of every active tile
    control_buffer: wgpu::Buffer,
    /// Indirect dispatch of one workgroup per active tile
    dispatch_buffer: wgpu::Buffer,
    /// Cells and gases queued for fluid conversion
    conversion_buffer: wgpu::Buffer,
    /// Asynchronous conversion counts paired with their tick
    conversion_readbacks: AcceleratorReadback<u64>,
    /// Tick of the latest completed nonzero fluid conversion sample
    last_nonzero_conversion_tick: Option<u32>,
    /// Current thermal tick
    tick: u32,
    /// First thermal index of each material form
    thermal_offsets: [u32; 4],
}

impl ThermalCellularTilePass {

    /// Creates conduction resources for every tile the cellular tile buffers can hold, with fluid
    /// particles' heat read from the fluid heat they deposit
    pub fn new(
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        material_table: &MaterialTable,
        fluid_heat_buffer: &wgpu::Buffer,
        rigid_body_tile_buffer: &wgpu::Buffer,
        changed_cellular_tile_buffer: &wgpu::Buffer,
        changed_rigid_body_cell_buffer: &wgpu::Buffer,
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
                label: Some("`ThermalCellularTilePass.bind_group_layout`"),
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
                    storage_entry(1, false),
                    storage_entry(2, false),
                    storage_entry(3, false),
                    storage_entry(4, false),
                    storage_entry(5, false),
                    storage_entry(6, false),
                    storage_entry(7, true),
                    storage_entry(8, false),
                    storage_entry(9, false),
                    storage_entry(10, false),
                    storage_entry(11, false),
                    storage_entry(12, true),
                    storage_entry(13, false),
                    storage_entry(14, false),
                    storage_entry(15, false),
                    storage_entry(16, false),
                    storage_entry(17, false),
                    storage_entry(18, false),
                    storage_entry(19, false),
                    storage_entry(20, true),
                    storage_entry(21, true),
                ],
            },
        );
        let dispatch_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`ThermalCellularTilePass.dispatch_bind_group_layout`"),
                entries: &[storage_entry(0, false)],
            },
        );
        // only the prepare step writes the indirect dispatch the other steps read
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`ThermalCellularTilePass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let prepare_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`ThermalCellularTilePass.prepare_pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(&dispatch_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_source: String = [
            MATERIAL_RADIANCE_WGSL,
            include_str!("thermal_cellular_tile.wgsl"),
            include_str!("thermal_transition.wgsl"),
        ].join("\n");
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`ThermalCellularTilePass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            },
        );
        let mut pipelines: HashMap<&'static str, wgpu::ComputePipeline> = HashMap::new();
        for entry_point in ["collect", "clear", "prepare", "step", "apply"] {
            pipelines.insert(entry_point, device.create_compute_pipeline(
                &wgpu::ComputePipelineDescriptor {
                    label: Some("`ThermalCellularTilePass.pipelines`"),
                    layout: Some(if entry_point == "prepare" {
                        &prepare_pipeline_layout
                    } else {
                        &pipeline_layout
                    }),
                    module: &shader_module,
                    entry_point: Some(entry_point),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    cache: None,
                },
            ));
        }
        let create_buffer = |label: &str, size: u64, usage: wgpu::BufferUsages| -> wgpu::Buffer {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let uniform_buffer: wgpu::Buffer = create_buffer(
            "`ThermalCellularTilePass.uniform_buffer`",
            UNIFORM_BYTE_COUNT,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let control_buffer: wgpu::Buffer = create_buffer(
            "`ThermalCellularTilePass.control_buffer`",
            4 + tile_count * 4,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let dispatch_buffer: wgpu::Buffer = create_buffer(
            "`ThermalCellularTilePass.dispatch_buffer`",
            12,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
        );
        // each cell's stepped temperature and the change of its fluid particles
        let step_buffer: wgpu::Buffer = create_buffer(
            "`ThermalCellularTilePass.step_buffer`",
            cell_count * 8,
            wgpu::BufferUsages::STORAGE,
        );
        let conversion_buffer: wgpu::Buffer = create_buffer(
            "`ThermalCellularTilePass.conversion_buffer`",
            (2 + THERMAL_CONVERSION_COUNT_MAXIMUM * 2) * 4,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
        );
        let conversion_readbacks: AcceleratorReadback<u64> = AcceleratorReadback::new(
            accelerator,
            "`ThermalCellularTilePass.conversion_readback`",
            4,
            CONVERSION_READBACK_COUNT,
        );
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`ThermalCellularTilePass.bind_group`"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: cellular_tile_buffers.material_identifier_buffer()
                        .as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: cellular_tile_buffers.matter_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: cellular_tile_buffers.temperature_change_buffer()
                        .as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: cellular_tile_buffers.gas_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: cellular_tile_buffers.thermal_awake_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry { binding: 6, resource: control_buffer.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: material_table.thermal_properties_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: fluid_heat_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry { binding: 9, resource: step_buffer.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: cellular_tile_buffers.awake_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: cellular_tile_buffers.gas_awake_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: material_table.transition_color_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: cellular_tile_buffers.color_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: cellular_tile_buffers.kinematics_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 15,
                    resource: cellular_tile_buffers.rigid_body_cell_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 16,
                    resource: rigid_body_tile_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 17,
                    resource: changed_cellular_tile_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 18,
                    resource: changed_rigid_body_cell_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 19,
                    resource: conversion_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 20,
                    resource: material_table.gas_properties_buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 21,
                    resource: material_table.cellular_static_properties_buffer()
                        .as_entire_binding(),
                },
            ],
        });
        let dispatch_bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("`ThermalCellularTilePass.dispatch_bind_group`"),
                layout: &dispatch_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: dispatch_buffer.as_entire_binding(),
                }],
            },
        );
        Self {
            pipelines,
            bind_group,
            dispatch_bind_group,
            uniform_buffer,
            control_buffer,
            dispatch_buffer,
            conversion_buffer,
            conversion_readbacks,
            last_nonzero_conversion_tick: None,
            tick: 0,
            thermal_offsets: material_table.thermal_offsets(),
        }
    }

    /// Conducts one tick of heat through the active tiles without waiting for the GPU, with
    /// phantom air of an ambient temperature, heat capacity per amount unit, and conduction
    pub fn simulate_cellular_tiles(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        ambient_temperature: f32,
        ambient_heat_capacity: f32,
        ambient_conduction: f32,
    ) {
        let width: u32 = active_region.width() as u32;
        let height: u32 = active_region.height() as u32;
        let tile_count: u32 = width * height;
        let mut uniform_bytes: Vec<u8> = Vec::with_capacity(UNIFORM_BYTE_COUNT as usize);
        for value in [
            width,
            height,
            ring_offset.0,
            ring_offset.1,
            self.thermal_offsets[0],
            self.thermal_offsets[1],
            self.thermal_offsets[2],
            self.thermal_offsets[3],
            THERMAL_TIME_STEP.to_bits(),
            ambient_temperature.to_bits(),
            ambient_heat_capacity.to_bits(),
            ambient_conduction.to_bits(),
            THERMAL_SLEEP_TEMPERATURE.to_bits(),
            THERMAL_FLUID_CAPACITY_SCALE.to_bits(),
            THERMAL_FLUID_ENERGY_SCALE.to_bits(),
            THERMAL_FLUID_CONDUCTION_SCALE.to_bits(),
            THERMAL_HEAT_SCALE.to_bits(),
            self.tick,
            0,
            0,
        ] {
            uniform_bytes.extend_from_slice(&value.to_le_bytes());
        }
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene thermal cellular tiles"),
            });
        command_encoder.clear_buffer(&self.conversion_buffer, 0, Some(8));
        command_encoder.clear_buffer(&self.control_buffer, 0, Some(4));
        {
            let mut compute_pass: wgpu::ComputePass<'_> = command_encoder
                .begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Scene thermal cellular tiles"),
                    timestamp_writes: None,
                });
            let pipelines: &HashMap<&'static str, wgpu::ComputePipeline> = &self.pipelines;
            compute_pass.set_bind_group(0, &self.bind_group, &[]);
            // list the tiles awake for any reason and their neighbors, then clear their flags for
            // this tick's step to set
            compute_pass.set_pipeline(&pipelines["collect"]);
            compute_pass.dispatch_workgroups(tile_count.div_ceil(WORKGROUP_SIZE), 1, 1);
            compute_pass.set_pipeline(&pipelines["clear"]);
            compute_pass.dispatch_workgroups(tile_count.div_ceil(WORKGROUP_SIZE), 1, 1);
            compute_pass.set_pipeline(&pipelines["prepare"]);
            compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            // every cell steps from the temperatures before the step, then writes its result
            compute_pass.set_pipeline(&pipelines["step"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            compute_pass.set_pipeline(&pipelines["apply"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
        }
        let readback: bool = self.conversion_readbacks.copy(
            &mut command_encoder,
            &self.conversion_buffer,
            0,
            4,
            u64::from(self.tick),
        );
        let submission_index: wgpu::SubmissionIndex =
            accelerator.queue().submit(Some(command_encoder.finish()));
        if readback { self.conversion_readbacks.map(submission_index); }
        self.tick = self.tick.wrapping_add(1);
    }

    /// Returns the cells and gases queued for conversion into fluid particles
    pub fn conversion_buffer(&self) -> &wgpu::Buffer { &self.conversion_buffer }

    /// Returns whether a recent completed sample contains fluid conversions
    pub fn has_fluid_conversions(&mut self, accelerator: &Accelerator) -> Result<bool, SceneError> {
        let mut newest: Option<(u64, u32)> = None;
        let result: Result<(), AcceleratorReadbackError> = self.conversion_readbacks.read(
            accelerator,
            false,
            |tick, bytes| {
                let count: u32 = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                if newest.is_none_or(|(newest_tick, _)| tick >= newest_tick) {
                    newest = Some((tick, count));
                }
            },
        );
        result.map_err(|error| match error {
            AcceleratorReadbackError::Poll(_) => SceneError::FluidParticlePoll,
            AcceleratorReadbackError::BufferAsync(error) =>
                SceneError::FluidParticleResponse { error },
        })?;
        if let Some((tick, count)) = newest {
            if count > 0 { self.last_nonzero_conversion_tick = Some(tick as u32); }
        }
        let recent_nonzero: bool = self.last_nonzero_conversion_tick
            .is_some_and(|tick| self.tick.wrapping_sub(tick) < 2);
        Ok(newest.is_some_and(|(_, count)| count > 0) || recent_nonzero)
    }

}
