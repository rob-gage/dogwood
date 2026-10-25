// Copyright Rob Gage 2026

use super::{
    PRESSURE_ACTOR_DRAG,
    PRESSURE_AMBIENT_DIFFUSIVITY,
    PRESSURE_BROKEN_CELLULAR_TILE_COUNT_MAXIMUM,
    PRESSURE_BROKEN_RIGID_BODY_CELL_COUNT_MAXIMUM,
    PRESSURE_BULK_VISCOSITY,
    PRESSURE_DAMAGE_RATE,
    PRESSURE_FLOW_DAMPING,
    PRESSURE_FLOW_MINIMUM,
    PRESSURE_GAS_SPEED,
    PRESSURE_SOLID_IMPEDANCE,
    PRESSURE_SOLID_RELAXATION,
    PRESSURE_SOLID_SPEED,
    PRESSURE_SUBSTEP_COUNT,
    PRESSURE_TIME_STEP,
    PRESSURE_TRAPPED_EXPULSION,
};
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
const UNIFORM_BYTE_COUNT: u64 = 96;

/// GPU simulation of the pressure in the active tiles, alternating between the cellular tile gas
/// buffer and a scratch buffer every substep
pub struct PressureCellularTilePass {
    /// Pipelines of every simulation step, named by their entry points
    pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    /// Bind groups reading the cellular tile gas and writing the scratch gas, then the reverse
    bind_groups: [wgpu::BindGroup; 2],
    /// Bind group of the indirect dispatch written by the prepare step
    dispatch_bind_group: wgpu::BindGroup,
    /// Uniforms of every step
    uniform_buffer: wgpu::Buffer,
    /// Active tile count and the count of tiles whose open volumes are measured, then the active
    /// tiles and, after room for every tile of the region, the measured tiles
    control_buffer: wgpu::Buffer,
    /// Indirect dispatches of one workgroup per active tile, then per tile whose open volumes
    /// are measured
    dispatch_buffer: wgpu::Buffer,
    /// Region and ring offset whose open volumes were last measured everywhere, after which only
    /// tiles that may have changed are measured
    volume_region: Option<(CellularTileRegion, (u32, u32))>,
    /// Count of the tiles whose cellular tile cells broke this tick, then each tile's buffer index
    /// and remaining static cells as two `u32` masks
    broken_cellular_tile_buffer: wgpu::Buffer,
    /// Count of the rigid body cells that broke this tick, then each one's rigid body tile index
    /// times 64 plus its cell
    broken_rigid_body_cell_buffer: wgpu::Buffer,
}

impl PressureCellularTilePass {

    /// Creates simulation resources for every tile the cellular tile buffers can hold, with
    /// actors displacing gas read from the cellular tile actor records and rigid body cells told
    /// apart by the rigid body tile records
    pub fn new(
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        material_table: &MaterialTable,
        fluid_raster_buffer: &wgpu::Buffer,
        actor_buffer: &wgpu::Buffer,
        rigid_body_cellular_tile_buffer: &wgpu::Buffer,
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
                label: Some("`PressureCellularTilePass.bind_group_layout`"),
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
                    storage_entry(2, true),
                    storage_entry(3, false),
                    storage_entry(4, false),
                    storage_entry(5, false),
                    storage_entry(6, false),
                    storage_entry(7, true),
                    storage_entry(8, true),
                    storage_entry(9, true),
                    storage_entry(10, true),
                    storage_entry(11, true),
                    storage_entry(12, true),
                    storage_entry(13, false),
                    storage_entry(14, false),
                    storage_entry(15, false),
                    storage_entry(16, false),
                    storage_entry(17, false),
                    storage_entry(18, false),
                    storage_entry(19, false),
                ],
            },
        );
        let dispatch_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`PressureCellularTilePass.dispatch_bind_group_layout`"),
                entries: &[storage_entry(0, false)],
            },
        );
        // only the prepare step writes the indirect dispatch the other steps read
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`PressureCellularTilePass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let prepare_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`PressureCellularTilePass.prepare_pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(&dispatch_bind_group_layout)],
                immediate_size: 0,
            },
        );
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`PressureCellularTilePass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(include_str!("pressure_cellular_tile.wgsl").into()),
            },
        );
        let mut pipelines: HashMap<&'static str, wgpu::ComputePipeline> = HashMap::new();
        for entry_point in [
            "volume_collect",
            "prepare_volume",
            "volume",
            "collect",
            "clear",
            "prepare",
            "step",
            "publish",
            "damage",
        ] {
            let layout: &wgpu::PipelineLayout = if entry_point.starts_with("prepare") {
                &prepare_pipeline_layout
            } else {
                &pipeline_layout
            };
            pipelines.insert(entry_point, device.create_compute_pipeline(
                &wgpu::ComputePipelineDescriptor {
                    label: Some("`PressureCellularTilePass.pipelines`"),
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
            "`PressureCellularTilePass.uniform_buffer`",
            UNIFORM_BYTE_COUNT,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        // the gas of odd substeps, laid out like the cellular tile gas
        let scratch_buffer: wgpu::Buffer = create_buffer(
            "`PressureCellularTilePass.scratch_buffer`",
            cellular_tile_buffers.gas_buffer().size(),
            wgpu::BufferUsages::STORAGE,
        );
        // each cell's open volume and the velocity of the actor dragging its gas, measured once
        // per tick
        let volume_buffer: wgpu::Buffer = create_buffer(
            "`PressureCellularTilePass.volume_buffer`",
            cell_count * 8,
            wgpu::BufferUsages::STORAGE,
        );
        let control_buffer: wgpu::Buffer = create_buffer(
            "`PressureCellularTilePass.control_buffer`",
            8 + tile_count * 8,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        // the count of tiles where cellular tile cells broke this tick, then each tile's buffer
        // index and remaining static cells
        let broken_cellular_tile_buffer: wgpu::Buffer = create_buffer(
            "`PressureCellularTilePass.broken_cellular_tile_buffer`",
            4 + PRESSURE_BROKEN_CELLULAR_TILE_COUNT_MAXIMUM * 12,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        );
        // the count of rigid body cells that broke this tick, then the cells
        let broken_rigid_body_cell_buffer: wgpu::Buffer = create_buffer(
            "`PressureCellularTilePass.broken_rigid_body_cell_buffer`",
            4 + PRESSURE_BROKEN_RIGID_BODY_CELL_COUNT_MAXIMUM * 4,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        );
        let dispatch_buffer: wgpu::Buffer = create_buffer(
            "`PressureCellularTilePass.dispatch_buffer`",
            24,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
        );
        let create_bind_group = |gas_in: &wgpu::Buffer, gas_out: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("`PressureCellularTilePass.bind_groups`"),
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
                    wgpu::BindGroupEntry {
                        binding: 11,
                        resource: material_table.cellular_static_properties_buffer()
                            .as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 12,
                        resource: cellular_tile_buffers.rigid_body_cell_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 13,
                        resource: rigid_body_cellular_tile_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 14,
                        resource: cellular_tile_buffers.pressure_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 15,
                        resource: cellular_tile_buffers.awake_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 16,
                        resource: cellular_tile_buffers.kinematics_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 17,
                        resource: broken_cellular_tile_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 18,
                        resource: cellular_tile_buffers.color_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 19,
                        resource: broken_rigid_body_cell_buffer.as_entire_binding(),
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
                label: Some("`PressureCellularTilePass.dispatch_bind_group`"),
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
            volume_region: None,
            broken_cellular_tile_buffer,
            broken_rigid_body_cell_buffer,
        }
    }

    /// Simulates one tick of the pressure in the active tiles without waiting for the GPU, with
    /// gravity in cells per substep squared and the first actor records displacing gas, then
    /// publishes the pressure of the active tiles
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
        if u64::from(tile_count) * 8 + 8 > self.control_buffer.size() {
            return Err(SceneError::InvalidCellularTileRegion);
        }
        // every tile is measured when the region or ring moves, and afterward only tiles that may
        // have changed
        let is_volume_listed: bool = self.volume_region == Some((active_region, ring_offset));
        self.volume_region = Some((active_region, ring_offset));
        // the shader measures everything in cells, amount units, and substeps
        let substep_count: f32 = PRESSURE_SUBSTEP_COUNT as f32;
        let substep_time: f32 = PRESSURE_TIME_STEP / substep_count;
        let cells_per_substep = |tiles_per_second: f32| -> f32 {
            tiles_per_second * CELLULAR_TILE_SIZE as f32 * substep_time
        };
        let fraction_per_substep = |rate: f32| -> f32 { 1.0 - (-rate * substep_time).exp() };
        let gas_speed: f32 = cells_per_substep(PRESSURE_GAS_SPEED);
        let solid_speed: f32 = cells_per_substep(PRESSURE_SOLID_SPEED);
        // a solid's impedance, its mass times its wave speed over its response, is its mass times
        // the solid impedance times the impedance of gas, the inverse of the gas wave speed
        let solid_response: f32 = gas_speed * solid_speed / PRESSURE_SOLID_IMPEDANCE;
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
            (1.0 / substep_count).to_bits(),
            (gas_speed * gas_speed).to_bits(),
            fraction_per_substep(PRESSURE_FLOW_DAMPING).to_bits(),
            (PRESSURE_BULK_VISCOSITY * substep_time).to_bits(),
            (PRESSURE_FLOW_MINIMUM * substep_time).to_bits(),
            (PRESSURE_AMBIENT_DIFFUSIVITY * substep_time).to_bits(),
            substep_count.to_bits(),
            fraction_per_substep(PRESSURE_ACTOR_DRAG).to_bits(),
            solid_response.to_bits(),
            solid_speed.to_bits(),
            fraction_per_substep(PRESSURE_SOLID_RELAXATION).to_bits(),
            fraction_per_substep(PRESSURE_TRAPPED_EXPULSION).to_bits(),
            (PRESSURE_DAMAGE_RATE * substep_time * 65535.0).to_bits(),
            u32::from(is_volume_listed),
            0,
        ] {
            uniform_bytes.extend_from_slice(&value.to_le_bytes());
        }
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene pressure cellular tiles"),
            });
        command_encoder.clear_buffer(&self.control_buffer, 0, Some(8));
        command_encoder.clear_buffer(&self.broken_cellular_tile_buffer, 0, Some(4));
        command_encoder.clear_buffer(&self.broken_rigid_body_cell_buffer, 0, Some(4));
        {
            let mut compute_pass: wgpu::ComputePass<'_> = command_encoder
                .begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Scene pressure cellular tiles"),
                    timestamp_writes: None,
                });
            let pipelines: &HashMap<&'static str, wgpu::ComputePipeline> = &self.pipelines;
            compute_pass.set_bind_group(0, &self.bind_groups[0], &[]);
            // measure open volumes where they may have changed so changed cells wake their tiles,
            // then list the active tiles and clear their flags for this tick's simulation to set
            if is_volume_listed {
                compute_pass.set_pipeline(&pipelines["volume_collect"]);
                compute_pass.dispatch_workgroups(tile_count.div_ceil(WORKGROUP_SIZE), 1, 1);
                compute_pass.set_pipeline(&pipelines["prepare_volume"]);
                compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
                compute_pass.dispatch_workgroups(1, 1, 1);
                compute_pass.set_pipeline(&pipelines["volume"]);
                compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 12);
            } else {
                compute_pass.set_pipeline(&pipelines["volume"]);
                compute_pass.dispatch_workgroups(tile_count, 1, 1);
            }
            compute_pass.set_pipeline(&pipelines["collect"]);
            compute_pass.dispatch_workgroups(tile_count.div_ceil(WORKGROUP_SIZE), 1, 1);
            compute_pass.set_pipeline(&pipelines["clear"]);
            compute_pass.dispatch_workgroups(tile_count.div_ceil(WORKGROUP_SIZE), 1, 1);
            compute_pass.set_pipeline(&pipelines["prepare"]);
            compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            for substep in 0..PRESSURE_SUBSTEP_COUNT {
                compute_pass.set_bind_group(0, &self.bind_groups[substep % 2], &[]);
                compute_pass.set_pipeline(&pipelines["step"]);
                compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            }
            // the even substep count leaves the newest cells in the cellular tile gas buffer
            compute_pass.set_bind_group(0, &self.bind_groups[0], &[]);
            compute_pass.set_pipeline(&pipelines["publish"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            // broken cells change in the buffer the last substep wrote, read with the published
            // pressure around them
            compute_pass.set_bind_group(0, &self.bind_groups[1], &[]);
            compute_pass.set_pipeline(&pipelines["damage"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
        }
        accelerator.queue().submit(Some(command_encoder.finish()));
        Ok(())
    }

    /// Returns the count of the tiles whose cellular tile cells broke in the last tick, then each
    /// tile's buffer index and remaining static cells as two `u32` masks
    pub fn broken_cellular_tile_buffer(&self) -> &wgpu::Buffer {
        &self.broken_cellular_tile_buffer
    }

    /// Returns the count of the rigid body cells that broke in the last tick, then each one's
    /// rigid body tile index times 64 plus its cell
    pub fn broken_rigid_body_cell_buffer(&self) -> &wgpu::Buffer {
        &self.broken_rigid_body_cell_buffer
    }

}
