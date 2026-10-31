// Copyright Rob Gage 2026

use super::CHEMISTRY_TIME_STEP;
use crate::{
    materials::MaterialTable,
    scenes::cellular_tiles::{
        CellularTileBuffers,
        CellularTileRegion,
        CELLULAR_TILE_SIZE,
    },
    scenes::pressure::GAS_AMOUNT_PER_ATMOSPHERE,
    scenes::thermal::{
        THERMAL_FLUID_CAPACITY_SCALE,
        THERMAL_FLUID_ENERGY_SCALE,
        THERMAL_HEAT_SCALE,
    },
};
use engine_host::acceleration::Accelerator;
use engine_graphics::world::materials::MATERIAL_RADIANCE_WGSL;
use std::collections::HashMap;

/// Bytes of the uniforms
const UNIFORM_BYTE_COUNT: u64 = 80;

/// Bytes of one thermal index's record in the thermal properties
const THERMAL_RECORD_BYTE_COUNT: u64 = 64;

/// GPU reactions between the matter of the cells of the tiles thermal lists: the tiles whose
/// matter can react are selected, then react with one invocation per block of two by two cells
pub struct ChemistryCellularTilePass {
    /// Pipelines of every step, named by their entry points
    pipelines: HashMap<&'static str, wgpu::ComputePipeline>,
    /// Bind group of the uniforms, cells, fluid species, reactions, and reacting tiles
    bind_group: wgpu::BindGroup,
    /// Bind group of what the selection reads of the cells, and thermal's listed tiles
    select_bind_group: wgpu::BindGroup,
    /// Bind group of the reacting tiles the selection lists, and their indirect dispatch
    reacting_bind_group: wgpu::BindGroup,
    /// Uniforms of every step
    uniform_buffer: wgpu::Buffer,
    /// Indirect dispatch of one workgroup per reacting tile
    dispatch_buffer: wgpu::Buffer,
    /// Current chemistry tick, alternating the blocks' offset
    tick: u32,
    /// First thermal index of each material form
    thermal_offsets: [u32; 4],
    /// Number of thermal indices
    thermal_count: u32,
}

impl ChemistryCellularTilePass {

    /// Creates reaction resources for every tile the cellular tile buffers can hold, with fluid
    /// particles seen through the fluid species and heat they deposit, over the tiles thermal
    /// lists
    pub fn new(
        accelerator: &Accelerator,
        cellular_tile_buffers: &CellularTileBuffers,
        material_table: &MaterialTable,
        fluid_species_buffer: wgpu::BufferBinding<'_>,
        fluid_heat_buffer: &wgpu::Buffer,
        fluid_spawn_buffer: &wgpu::Buffer,
        rigid_body_tile_buffer: &wgpu::Buffer,
        changed_rigid_body_cell_buffer: &wgpu::Buffer,
        thermal_control_buffer: &wgpu::Buffer,
    ) -> Self {
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
        let uniform_entry: wgpu::BindGroupLayoutEntry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(UNIFORM_BYTE_COUNT),
            },
            count: None,
        };
        // the tiles, materials, reactions, fluid heat, and pressures are only read
        let mut entries: Vec<wgpu::BindGroupLayoutEntry> = vec![uniform_entry];
        for binding in 1..=24 {
            entries.push(storage_entry(binding, matches!(binding, 13..=18 | 20 | 22..=24)));
        }
        let bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`ChemistryCellularTilePass.bind_group_layout`"),
                entries: &entries,
            },
        );
        // the selection reads only what cells hold, beside the reacting tile list it fills
        let select_bindings: [u32; 7] = [0, 1, 4, 12, 13, 18, 19];
        let select_entries: Vec<wgpu::BindGroupLayoutEntry> = select_bindings.iter()
            .map(|binding| entries[*binding as usize])
            .collect();
        let select_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`ChemistryCellularTilePass.select_bind_group_layout`"),
                entries: &select_entries,
            },
        );
        let reacting_bind_group_layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("`ChemistryCellularTilePass.reacting_bind_group_layout`"),
                entries: &[storage_entry(0, false), storage_entry(1, false)],
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`ChemistryCellularTilePass.pipeline_layout`"),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            },
        );
        let select_pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("`ChemistryCellularTilePass.select_pipeline_layout`"),
                bind_group_layouts: &[
                    Some(&select_bind_group_layout),
                    Some(&reacting_bind_group_layout),
                ],
                immediate_size: 0,
            },
        );
        let shader_source: String = [
            MATERIAL_RADIANCE_WGSL,
            include_str!("chemistry_cellular_tile.wgsl"),
            include_str!("../thermal/thermal_transition.wgsl"),
            include_str!("../thermal/thermal_conversion.wgsl"),
        ].join("\n");
        let shader_module: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("`ChemistryCellularTilePass.shader_module`"),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            },
        );
        let mut pipelines: HashMap<&'static str, wgpu::ComputePipeline> = HashMap::new();
        for entry_point in ["select_tiles", "prepare_reactions", "react_tiles"] {
            pipelines.insert(entry_point, device.create_compute_pipeline(
                &wgpu::ComputePipelineDescriptor {
                    label: Some("`ChemistryCellularTilePass.pipelines`"),
                    layout: Some(if entry_point == "react_tiles" {
                        &pipeline_layout
                    } else {
                        &select_pipeline_layout
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
            "`ChemistryCellularTilePass.uniform_buffer`",
            UNIFORM_BYTE_COUNT,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let tile_count: u64 = cellular_tile_buffers.material_identifier_buffer().size() / 4
            / (CELLULAR_TILE_SIZE * CELLULAR_TILE_SIZE) as u64;
        // the reacting tile count, then the buffer index and reactive block mask of every
        // reacting tile, with room to pad them to whole workgroups
        let reacting_tile_buffer: wgpu::Buffer = create_buffer(
            "`ChemistryCellularTilePass.reacting_tile_buffer`",
            4 + (tile_count + 3) * 8,
            wgpu::BufferUsages::STORAGE,
        );
        let dispatch_buffer: wgpu::Buffer = create_buffer(
            "`ChemistryCellularTilePass.dispatch_buffer`",
            12,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
        );
        let resources: [&wgpu::Buffer; 25] = [
            &uniform_buffer,
            cellular_tile_buffers.material_identifier_buffer(),
            cellular_tile_buffers.matter_buffer(),
            cellular_tile_buffers.temperature_change_buffer(),
            cellular_tile_buffers.gas_buffer(),
            cellular_tile_buffers.color_buffer(),
            cellular_tile_buffers.kinematics_buffer(),
            cellular_tile_buffers.rigid_body_cell_buffer(),
            rigid_body_tile_buffer,
            changed_rigid_body_cell_buffer,
            cellular_tile_buffers.awake_buffer(),
            cellular_tile_buffers.gas_awake_buffer(),
            cellular_tile_buffers.thermal_awake_buffer(),
            &reacting_tile_buffer,
            material_table.thermal_properties_buffer(),
            material_table.cellular_static_properties_buffer(),
            material_table.reaction_buffer(),
            material_table.reaction_color_buffer(),
            material_table.reaction_lookup_buffer(),
            fluid_heat_buffer,
            fluid_heat_buffer,
            fluid_spawn_buffer,
            cellular_tile_buffers.pressure_buffer(),
            material_table.fluid_properties_buffer(),
            material_table.gas_properties_buffer(),
        ];
        // the fluid species are a section of the fluid raster, and the selection reads thermal's
        // listed tiles where the reaction step reads the reacting ones
        fn entry<'a>(
            binding: u32,
            buffer: &'a wgpu::Buffer,
            fluid_species_buffer: &wgpu::BufferBinding<'a>,
        ) -> wgpu::BindGroupEntry<'a> {
            wgpu::BindGroupEntry {
                binding,
                resource: if binding == 19 {
                    wgpu::BindingResource::Buffer(fluid_species_buffer.clone())
                } else {
                    buffer.as_entire_binding()
                },
            }
        }
        let bind_group_entries: Vec<wgpu::BindGroupEntry<'_>> = resources.iter()
            .enumerate()
            .map(|(binding, buffer)| entry(binding as u32, buffer, &fluid_species_buffer))
            .collect();
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("`ChemistryCellularTilePass.bind_group`"),
            layout: &bind_group_layout,
            entries: &bind_group_entries,
        });
        let select_bind_group_entries: Vec<wgpu::BindGroupEntry<'_>> = select_bindings.iter()
            .map(|binding| entry(
                *binding,
                if *binding == 13 { thermal_control_buffer } else { resources[*binding as usize] },
                &fluid_species_buffer,
            ))
            .collect();
        let select_bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("`ChemistryCellularTilePass.select_bind_group`"),
                layout: &select_bind_group_layout,
                entries: &select_bind_group_entries,
            },
        );
        let reacting_bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: Some("`ChemistryCellularTilePass.reacting_bind_group`"),
                layout: &reacting_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: reacting_tile_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: dispatch_buffer.as_entire_binding(),
                    },
                ],
            },
        );
        Self {
            pipelines,
            bind_group,
            select_bind_group,
            reacting_bind_group,
            uniform_buffer,
            dispatch_buffer,
            tick: 0,
            thermal_offsets: material_table.thermal_offsets(),
            thermal_count: (material_table.thermal_properties_buffer().size()
                / THERMAL_RECORD_BYTE_COUNT) as u32,
        }
    }

    /// Prepares one tick of reactions in the tiles thermal lists, with phantom air at an ambient
    /// amount per cell and heat capacity per atmosphere
    pub fn prepare_cellular_tiles(
        &mut self,
        accelerator: &Accelerator,
        active_region: CellularTileRegion,
        ring_offset: (u32, u32),
        ambient_gas_amount: u32,
        ambient_heat_capacity: f32,
    ) {
        let mut uniform_bytes: Vec<u8> = Vec::with_capacity(UNIFORM_BYTE_COUNT as usize);
        for value in [
            active_region.width() as u32,
            active_region.height() as u32,
            ring_offset.0,
            ring_offset.1,
            self.thermal_offsets[0],
            self.thermal_offsets[1],
            self.thermal_offsets[2],
            self.thermal_offsets[3],
            self.thermal_count,
            self.tick % 2,
            self.tick,
            CHEMISTRY_TIME_STEP.to_bits(),
            (ambient_gas_amount as f32).to_bits(),
            THERMAL_HEAT_SCALE.to_bits(),
            THERMAL_FLUID_CAPACITY_SCALE.to_bits(),
            THERMAL_FLUID_ENERGY_SCALE.to_bits(),
            (ambient_heat_capacity / GAS_AMOUNT_PER_ATMOSPHERE).to_bits(),
            0,
            0,
            0,
        ] {
            uniform_bytes.extend_from_slice(&value.to_le_bytes());
        }
        accelerator.queue().write_buffer(&self.uniform_buffer, 0, &uniform_bytes);
        self.tick = self.tick.wrapping_add(1);
    }

    /// Records the prepared tick's reactions into a compute pass: the tiles of thermal's indirect
    /// dispatch whose matter can react are listed, then each listed tile reacts
    pub fn record_cellular_tiles(
        &self,
        compute_pass: &mut wgpu::ComputePass<'_>,
        thermal_dispatch_buffer: &wgpu::Buffer,
    ) {
        compute_pass.set_pipeline(&self.pipelines["select_tiles"]);
        compute_pass.set_bind_group(0, &self.select_bind_group, &[]);
        compute_pass.set_bind_group(1, &self.reacting_bind_group, &[]);
        compute_pass.dispatch_workgroups_indirect(thermal_dispatch_buffer, 0);
        compute_pass.set_pipeline(&self.pipelines["prepare_reactions"]);
        compute_pass.dispatch_workgroups(1, 1, 1);
        compute_pass.set_pipeline(&self.pipelines["react_tiles"]);
        compute_pass.set_bind_group(0, &self.bind_group, &[]);
        compute_pass.dispatch_workgroups_indirect(&self.dispatch_buffer, 0);
    }

}
