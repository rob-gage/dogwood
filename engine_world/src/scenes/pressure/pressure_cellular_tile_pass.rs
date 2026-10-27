// Copyright Rob Gage 2026

use super::{
    GAS_AMOUNT_PER_ATMOSPHERE,
    PRESSURE_ACTOR_DRAG,
    PRESSURE_AMBIENT_DIFFUSIVITY,
    PRESSURE_BULK_VISCOSITY,
    PRESSURE_DAMAGE_RATE,
    PRESSURE_FLOW_DAMPING,
    PRESSURE_FLOW_MINIMUM,
    PRESSURE_GAS_SPEED,
    PRESSURE_OPEN_RELAXATION,
    PRESSURE_OPEN_RELAXATION_MINIMUM,
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
const UNIFORM_BYTE_COUNT: u64 = 128;

/// GPU simulation of the pressure in the active tiles on pressure cells of 2x2 cells, rebuilt from
/// the cellular tile gas every tick, simulated alternating between two pressure cell buffers, and
/// spread back over the cells
pub struct PressureCellularTilePass {
    /// Pipelines of every simulation step, named by their entry points
    pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    /// Bind groups of the cell passes reading the cellular tile gas, then writing it
    bind_groups: [wgpu::BindGroup; 2],
    /// Bind groups rebuilding the pressure cells from the cellular tile gas, stepping from the
    /// first pressure cell buffer to the second and back, then spreading either over the cells
    coarse_bind_groups: [wgpu::BindGroup; 5],
    /// Uniforms of the pressure cell steps, measured in pressure cells
    coarse_uniform_buffer: wgpu::Buffer,
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
    /// First thermal index of the gas materials
    gas_thermal_offset: u32,
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
                    storage_entry(20, false),
                    storage_entry(21, true),
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
            "collect_blocks",
            "coarsen",
            "refine",
            "prepare",
            "step",
            "prepare_gas",
            "step_gas",
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
                    // the step writes all its workgroup memory before reading it, so it skips
                    // clearing it for every workgroup of every substep
                    compilation_options: wgpu::PipelineCompilationOptions {
                        zero_initialize_workgroup_memory: !entry_point.starts_with("step"),
                        ..Default::default()
                    },
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
        let coarse_uniform_buffer: wgpu::Buffer = create_buffer(
            "`PressureCellularTilePass.coarse_uniform_buffer`",
            UNIFORM_BYTE_COUNT,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        // the pressure cells, laid out like the cellular tile gas with one entry per 2x2 cells
        let coarse_buffers: [wgpu::Buffer; 2] = [
            create_buffer(
                "`PressureCellularTilePass.coarse_buffers`",
                cellular_tile_buffers.gas_buffer().size() / 4,
                wgpu::BufferUsages::STORAGE,
            ),
            create_buffer(
                "`PressureCellularTilePass.coarse_buffers`",
                cellular_tile_buffers.gas_buffer().size() / 4,
                wgpu::BufferUsages::STORAGE,
            ),
        ];
        // each cell's open volume and the velocity of the actor dragging its gas, measured once
        // per tick
        let volume_buffer: wgpu::Buffer = create_buffer(
            "`PressureCellularTilePass.volume_buffer`",
            cell_count * 8,
            wgpu::BufferUsages::STORAGE,
        );
        let control_buffer: wgpu::Buffer = create_buffer(
            "`PressureCellularTilePass.control_buffer`",
            // four counts, three lists of tiles, and four words per pressure cell
            16 + tile_count * 268,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let dispatch_buffer: wgpu::Buffer = create_buffer(
            "`PressureCellularTilePass.dispatch_buffer`",
            48,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
        );
        let create_bind_group = |uniform_buffer: &wgpu::Buffer, gas_in: &wgpu::Buffer, gas_out: &wgpu::Buffer| {
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
                        resource: changed_cellular_tile_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 18,
                        resource: cellular_tile_buffers.color_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 19,
                        resource: changed_rigid_body_cell_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 20,
                        resource: cellular_tile_buffers.matter_buffer().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 21,
                        resource: material_table.thermal_properties_buffer().as_entire_binding(),
                    },

                ],
            })
        };
        let gas_buffer: &wgpu::Buffer = cellular_tile_buffers.gas_buffer();
        let bind_groups: [wgpu::BindGroup; 2] = [
            create_bind_group(&uniform_buffer, gas_buffer, &coarse_buffers[0]),
            create_bind_group(&uniform_buffer, &coarse_buffers[1], gas_buffer),
        ];
        let coarse_bind_groups: [wgpu::BindGroup; 5] = [
            create_bind_group(&coarse_uniform_buffer, gas_buffer, &coarse_buffers[0]),
            create_bind_group(&coarse_uniform_buffer, &coarse_buffers[0], &coarse_buffers[1]),
            create_bind_group(&coarse_uniform_buffer, &coarse_buffers[1], &coarse_buffers[0]),
            create_bind_group(&coarse_uniform_buffer, &coarse_buffers[0], gas_buffer),
            create_bind_group(&coarse_uniform_buffer, &coarse_buffers[1], gas_buffer),
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
            coarse_bind_groups,
            coarse_uniform_buffer,
            dispatch_bind_group,
            uniform_buffer,
            control_buffer,
            dispatch_buffer,
            volume_region: None,
            gas_thermal_offset: material_table.thermal_offsets()[3],
        }
    }

    /// Simulates one tick of the pressure in the active tiles without waiting for the GPU, with
    /// gravity in cells per substep squared, the first actor records displacing gas, and phantom
    /// air of an ambient temperature and heat capacity per amount unit, then publishes the
    /// pressure of the active tiles
    pub fn simulate_cellular_tiles(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        gravity: f32,
        ambient_amount: u32,
        ambient_mass: f32,
        ambient_temperature: f32,
        ambient_heat_capacity: f32,
        actor_count: u32,
    ) -> Result<(), SceneError> {
        let width: u32 = active_region.width() as u32;
        let height: u32 = active_region.height() as u32;
        let tile_count: u32 = width * height;
        if u64::from(tile_count) * 268 + 16 > self.control_buffer.size() {
            return Err(SceneError::InvalidCellularTileRegion);
        }
        // every tile is measured when the region or ring moves, and afterward only tiles that may
        // have changed
        let is_volume_listed: bool = self.volume_region == Some((active_region, ring_offset));
        self.volume_region = Some((active_region, ring_offset));
        // the shaders measure everything in cells, or pressure cells of 2x2 cells, amount units of
        // a cell or of a pressure cell, and substeps
        let substep_count: f32 = PRESSURE_SUBSTEP_COUNT as f32;
        let substep_time: f32 = PRESSURE_TIME_STEP / substep_count;
        let uniforms = |cell_size: f32| -> Vec<u8> {
            let cells_per_tile: f32 = CELLULAR_TILE_SIZE as f32 / cell_size;
            let cells_per_substep = |tiles_per_second: f32| -> f32 {
                tiles_per_second * cells_per_tile * substep_time
            };
            let fraction_per_substep = |rate: f32| -> f32 { 1.0 - (-rate * substep_time).exp() };
            let gas_speed: f32 = cells_per_substep(PRESSURE_GAS_SPEED);
            let solid_speed: f32 = cells_per_substep(PRESSURE_SOLID_SPEED);
            // a solid's impedance, its mass times its wave speed over its response, is its mass
            // times the solid impedance times the impedance of gas, the inverse of the gas wave
            // speed
            let solid_response: f32 = gas_speed * solid_speed / PRESSURE_SOLID_IMPEDANCE;
            // a pressure cell holds the amount of its cells, and diffusion across wider cells
            // exchanges a smaller fraction
            let area: f32 = cell_size * cell_size;
            let mut uniform_bytes: Vec<u8> = Vec::with_capacity(UNIFORM_BYTE_COUNT as usize);
            for value in [
                width,
                height,
                ring_offset.0,
                ring_offset.1,
                (ambient_amount as f32 * area) as u32,
                ambient_mass.to_bits(),
                (gravity / cell_size).to_bits(),
                FLUID_RASTER_SCALE.to_bits(),
                actor_count,
                (1.0 / substep_count / cell_size).to_bits(),
                (gas_speed * gas_speed).to_bits(),
                fraction_per_substep(PRESSURE_FLOW_DAMPING).to_bits(),
                (PRESSURE_BULK_VISCOSITY * substep_time / area).to_bits(),
                (PRESSURE_FLOW_MINIMUM * substep_time * area).to_bits(),
                (PRESSURE_AMBIENT_DIFFUSIVITY * substep_time / area).to_bits(),
                substep_count.to_bits(),
                fraction_per_substep(PRESSURE_ACTOR_DRAG).to_bits(),
                solid_response.to_bits(),
                solid_speed.to_bits(),
                fraction_per_substep(PRESSURE_SOLID_RELAXATION).to_bits(),
                fraction_per_substep(PRESSURE_TRAPPED_EXPULSION).to_bits(),
                (PRESSURE_DAMAGE_RATE * substep_time * 65535.0).to_bits(),
                u32::from(is_volume_listed),
                (1.0 / area).to_bits(),
                fraction_per_substep(PRESSURE_OPEN_RELAXATION).to_bits(),
                (PRESSURE_OPEN_RELAXATION_MINIMUM * GAS_AMOUNT_PER_ATMOSPHERE * area * substep_time)
                    .to_bits(),
                ambient_temperature.to_bits(),
                ambient_heat_capacity.to_bits(),
                self.gas_thermal_offset,
                0,
                0,
                0,
            ] {
                uniform_bytes.extend_from_slice(&value.to_le_bytes());
            }
            uniform_bytes
        };
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniforms(1.0));
        accelerator.queue().write_buffer(&self.coarse_uniform_buffer, 0, &uniforms(2.0));
        let mut command_encoder: wgpu::CommandEncoder = accelerator.device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scene pressure cellular tiles"),
            });
        command_encoder.clear_buffer(&self.control_buffer, 0, Some(16));
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
            // the step simulates blocks of 2x2 tiles holding an active tile
            let block_count: u32 = width.div_ceil(2) * height.div_ceil(2);
            compute_pass.set_pipeline(&pipelines["collect_blocks"]);
            compute_pass.dispatch_workgroups(block_count.div_ceil(WORKGROUP_SIZE), 1, 1);
            compute_pass.set_pipeline(&pipelines["prepare"]);
            compute_pass.set_bind_group(1, &self.dispatch_bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
            // rebuild the pressure cells the listed blocks' halos reach
            compute_pass.set_bind_group(0, &self.coarse_bind_groups[0], &[]);
            compute_pass.set_pipeline(&pipelines["coarsen"]);
            compute_pass.dispatch_workgroups(tile_count, 1, 1);
            for substep in 0..PRESSURE_SUBSTEP_COUNT {
                compute_pass.set_bind_group(0, &self.coarse_bind_groups[1 + substep % 2], &[]);
                compute_pass.set_pipeline(&pipelines["step"]);
                compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 36);
                // blocks whose halo holds gas run the full step after the others
                compute_pass.set_pipeline(&pipelines["prepare_gas"]);
                compute_pass.dispatch_workgroups(1, 1, 1);
                compute_pass.set_pipeline(&pipelines["step_gas"]);
                compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 24);
            }
            // spread the newest pressure cells over the cells of the active tiles
            compute_pass.set_bind_group(
                0,
                &self.coarse_bind_groups[3 + PRESSURE_SUBSTEP_COUNT % 2],
                &[],
            );
            compute_pass.set_pipeline(&pipelines["refine"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 36);
            // the published pressure is interpolated from the newest pressure cells
            compute_pass.set_bind_group(
                0,
                &self.coarse_bind_groups[3 + PRESSURE_SUBSTEP_COUNT % 2],
                &[],
            );
            compute_pass.set_pipeline(&pipelines["publish"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
            // broken cells change in the cellular tile gas, read with the published pressure
            // around them
            compute_pass.set_bind_group(0, &self.bind_groups[1], &[]);
            compute_pass.set_pipeline(&pipelines["damage"]);
            compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
        }
        accelerator.queue().submit(Some(command_encoder.finish()));
        Ok(())
    }

}
