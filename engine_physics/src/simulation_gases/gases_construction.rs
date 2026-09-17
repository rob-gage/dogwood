// Copyright Rob Gage 2026

use super::*;

impl Gases {
    /// Creates dense gas fields matching the physical cellular tile ring
    pub fn new(
        accelerator: &Accelerator,
        materials: &MaterialRegistry,
        cellular_material_identifiers: &AcceleratorBuffer,
        external_body_occupancy: &AcceleratorBuffer,
        fluid_coverage: &AcceleratorBuffer,
        gas_properties: &AcceleratorBuffer,
        buffered_cell_count: usize,
        ambient_temperature: f32,
    ) -> Self {
        let device: &wgpu::Device = accelerator.wgpu_device();
        let buffered_cell_count: u32 = buffered_cell_count
            .try_into()
            .expect("Gas buffer exceeds Accelerator indexing range");
        let gas_count: u32 = materials
            .iter()
            .filter(|(_, material)| matches!(material, Material::Gas { .. }))
            .count()
            .try_into()
            .expect("Gas species count exceeds Accelerator indexing range");
        let concentration_count: u32 = buffered_cell_count
            .checked_mul(gas_count)
            .expect("Gas concentration buffer exceeds Accelerator indexing range");
        let streaming_value_count: u32 = buffered_cell_count
            .checked_mul(
                5u32.checked_add(gas_count)
                    .expect("Gas streaming record is too large"),
            )
            .expect("Gas streaming buffer exceeds Accelerator indexing range");
        let velocity: AcceleratorBuffer =
            accelerator.allocate::<[f32; 2]>(buffered_cell_count as usize);
        let velocity_scratch: AcceleratorBuffer =
            accelerator.allocate::<[f32; 2]>(buffered_cell_count as usize);
        let concentrations: AcceleratorBuffer =
            accelerator.allocate::<f32>(concentration_count.max(1) as usize);
        let gas_temperature = accelerator.allocate::<f32>(buffered_cell_count as usize);
        let concentration_scratch: AcceleratorBuffer =
            accelerator.allocate::<f32>(concentration_count.max(1) as usize);
        let divergence: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count as usize);
        let pressure_a: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count as usize);
        let pressure_b: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count as usize);
        let curl: AcceleratorBuffer = accelerator.allocate::<f32>(buffered_cell_count as usize);
        let streaming_data: AcceleratorBuffer =
            accelerator.allocate::<u32>(streaming_value_count as usize);
        let parameters: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gas simulation parameters"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let storage = |binding, read_only| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("gas simulation bind group layout"),
                entries: &[
                    storage(0, false),
                    storage(1, false),
                    storage(2, false),
                    storage(3, false),
                    storage(4, false),
                    storage(5, false),
                    storage(6, false),
                    storage(7, false),
                    storage(8, true),
                    storage(9, true),
                    storage(10, true),
                    storage(11, true),
                    storage(12, false),
                    storage(14, false),
                    wgpu::BindGroupLayoutEntry {
                        binding: 13,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gas simulation bind group"),
            layout: &layout,
            entries: &[
                Self::binding(0, &velocity),
                Self::binding(1, &velocity_scratch),
                Self::binding(2, &concentrations),
                Self::binding(3, &concentration_scratch),
                Self::binding(4, &divergence),
                Self::binding(5, &pressure_a),
                Self::binding(6, &pressure_b),
                Self::binding(7, &curl),
                Self::binding(8, cellular_material_identifiers),
                Self::binding(9, external_body_occupancy),
                Self::binding(10, fluid_coverage),
                Self::binding(11, gas_properties),
                Self::binding(12, &streaming_data),
                Self::binding(14, &gas_temperature),
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: parameters.as_entire_binding(),
                },
            ],
        });
        let shader: wgpu::ShaderModule = crate::simulation::create_simulation_shader_module(
            device,
            "gas simulation shader",
            concat!(
                include_str!("gases_shader_header.wgsl"),
                include_str!("gases_shader_solver.wgsl"),
                include_str!("gases_shader_streaming.wgsl"),
                include_str!("gases_shader_utility.wgsl"),
            ),
            "engine_physics/src/simulation/gases.wgsl",
        );
        let pipeline_layout: wgpu::PipelineLayout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("gas simulation pipeline layout"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let pipeline = |entry_point, label| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            velocity,
            velocity_scratch,
            concentrations,
            gas_temperature,
            concentration_scratch,
            divergence,
            pressure_a,
            pressure_b,
            curl,
            streaming_data,
            parameters,
            bind_group,
            advect_velocity_pipeline: pipeline(
                "advect_gas_velocity",
                "gas velocity advection pipeline",
            ),
            curl_pipeline: pipeline("calculate_gas_curl", "gas curl pipeline"),
            force_pipeline: pipeline("apply_gas_forces", "gas force pipeline"),
            divergence_pipeline: pipeline("calculate_gas_divergence", "gas divergence pipeline"),
            pressure_clear_pipeline: pipeline("clear_gas_pressure", "gas pressure clear pipeline"),
            pressure_a_pipeline: pipeline("solve_gas_pressure_a", "gas pressure A pipeline"),
            pressure_b_pipeline: pipeline("solve_gas_pressure_b", "gas pressure B pipeline"),
            projection_pipeline: pipeline(
                "project_gas_velocity",
                "gas velocity projection pipeline",
            ),
            concentration_pipeline: pipeline(
                "advect_gas_concentrations",
                "gas concentration advection pipeline",
            ),
            clear_area_pipeline: pipeline("clear_gas_area", "gas streamed area clear pipeline"),
            export_pipeline: pipeline("export_gas_area", "gas streamed area export pipeline"),
            buffered_cell_count,
            gas_count,
            ambient_temperature,
        }
    }
}
