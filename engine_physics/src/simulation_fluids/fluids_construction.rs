// Copyright Rob Gage 2026

use engine_compute::Accelerator;
use engine_compute::AcceleratorBuffer;

use super::Fluids;
use crate::simulation::simulation_constants::SUPPORT_RADIUS_CELLS;

impl Fluids {
    /// Creates the authoritative fluid particle store and solver resources.
    pub fn new(
        accelerator: &Accelerator,
        cellular_material_identifiers: &AcceleratorBuffer,
        external_body_occupancy: &AcceleratorBuffer,
        external_body_velocity: &AcceleratorBuffer,
        fluid_properties: &AcceleratorBuffer,
        thermal_properties: &AcceleratorBuffer,
        thermal_parameters: &wgpu::Buffer,
        buffered_width: u16,
        buffered_height: u16,
    ) -> Self {
        let device: &wgpu::Device = accelerator.wgpu_device();
        let buffered_cell_count: u32 = u32::from(buffered_width) * u32::from(buffered_height) * 64;
        let particle_capacity: u32 = buffered_cell_count;
        let bucket_dimensions: [u32; 2] = [
            (f32::from(buffered_width) * 8.0 / SUPPORT_RADIUS_CELLS).ceil() as u32,
            (f32::from(buffered_height) * 8.0 / SUPPORT_RADIUS_CELLS).ceil() as u32,
        ];
        let bucket_count: u32 = bucket_dimensions[0] * bucket_dimensions[1];
        let particles: AcceleratorBuffer =
            accelerator.allocate::<[u32; 10]>(particle_capacity as usize);
        let free_indices: AcceleratorBuffer =
            accelerator.allocate::<u32>(particle_capacity as usize);
        let free_count: AcceleratorBuffer = accelerator.allocate::<u32>(1);
        let edit_cells: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_cell_count as usize);
        let edit_amounts: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count as usize);
        let edit_temperatures: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count as usize);
        let accelerator_edits_pending: AcceleratorBuffer = accelerator.allocate::<u32>(1);
        let accelerator_edit_dispatch: wgpu::Buffer =
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Accelerator fluid edit indirect dispatch"),
                size: 72,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
                mapped_at_creation: false,
            });
        let bucket_heads: AcceleratorBuffer = accelerator.allocate::<u32>(bucket_count as usize);
        let next_particle: AcceleratorBuffer =
            accelerator.allocate::<u32>(particle_capacity as usize);
        let predicted_positions: AcceleratorBuffer =
            accelerator.allocate::<[f32; 2]>(particle_capacity as usize);
        let lambdas: AcceleratorBuffer = accelerator.allocate::<f32>(particle_capacity as usize);
        let position_corrections: AcceleratorBuffer =
            accelerator.allocate::<[f32; 2]>(particle_capacity as usize);
        let derived_material_identifiers: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_cell_count as usize);
        let derived_coverage: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count as usize);
        let derived_velocity: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let derived_thermal: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let mechanical_cells: AcceleratorBuffer =
            accelerator.allocate::<[u32; 4]>(buffered_cell_count as usize);
        let mechanical_original_velocity: AcceleratorBuffer =
            accelerator.allocate::<[f32; 2]>(buffered_cell_count as usize);
        let streaming_particles: AcceleratorBuffer =
            accelerator.allocate::<[u32; 10]>(particle_capacity as usize);
        let streaming_count: AcceleratorBuffer = accelerator.allocate::<u32>(1);
        let streaming_results: AcceleratorBuffer =
            accelerator.allocate::<u32>(particle_capacity as usize);
        let sample_output: AcceleratorBuffer = accelerator.allocate::<[f32; 8]>(1);
        let free_indices_data: Vec<u8> =
            (0..particle_capacity).flat_map(u32::to_le_bytes).collect();
        accelerator
            .wgpu_queue()
            .write_buffer(free_indices.wgpu_buffer(), 0, &free_indices_data);
        accelerator.wgpu_queue().write_buffer(
            free_count.wgpu_buffer(),
            0,
            &particle_capacity.to_le_bytes(),
        );
        let fluid_simulation_parameters: wgpu::Buffer =
            crate::simulation::create_simulation_uniform_buffer(
                device,
                "fluid simulation parameters",
                128,
            );
        let storage: fn(u32, bool) -> wgpu::BindGroupLayoutEntry =
            crate::simulation::storage_bind_group_layout_entry;
        let layout: wgpu::BindGroupLayout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("fluid simulation bind group layout"),
                entries: &[
                    storage(0, false),
                    storage(1, false),
                    storage(2, false),
                    storage(3, false),
                    storage(4, false),
                    storage(5, false),
                    storage(6, false),
                    storage(7, false),
                    storage(8, false),
                    storage(9, true),
                    storage(10, true),
                    storage(11, true),
                    crate::simulation::uniform_bind_group_layout_entry(12),
                    storage(13, false),
                    storage(14, false),
                    storage(15, false),
                    storage(16, true),
                    storage(17, false),
                    storage(18, false),
                    storage(19, false),
                    storage(20, false),
                    storage(21, false),
                    storage(22, false),
                    storage(23, false),
                    storage(24, false),
                    storage(25, false),
                    storage(26, true),
                    crate::simulation::uniform_bind_group_layout_entry(27),
                    storage(28, false),
                ],
            });
        let bind_group: wgpu::BindGroup = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fluid simulation bind group"),
            layout: &layout,
            entries: &[
                Self::binding(0, &particles),
                Self::binding(1, &free_indices),
                Self::binding(2, &free_count),
                Self::binding(3, &edit_cells),
                Self::binding(4, &bucket_heads),
                Self::binding(5, &next_particle),
                Self::binding(6, &derived_material_identifiers),
                Self::binding(7, &derived_coverage),
                Self::binding(8, &derived_velocity),
                Self::binding(9, cellular_material_identifiers),
                Self::binding(10, external_body_occupancy),
                Self::binding(11, external_body_velocity),
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: fluid_simulation_parameters.as_entire_binding(),
                },
                Self::binding(13, &predicted_positions),
                Self::binding(14, &lambdas),
                Self::binding(15, &position_corrections),
                Self::binding(16, fluid_properties),
                Self::binding(17, &streaming_particles),
                Self::binding(18, &streaming_count),
                Self::binding(19, &streaming_results),
                Self::binding(20, &sample_output),
                Self::binding(21, &mechanical_cells),
                Self::binding(22, &mechanical_original_velocity),
                Self::binding(23, &accelerator_edits_pending),
                Self::binding(24, &edit_amounts),
                Self::binding(25, &edit_temperatures),
                Self::binding(26, thermal_properties),
                wgpu::BindGroupEntry {
                    binding: 27,
                    resource: thermal_parameters.as_entire_binding(),
                },
                Self::binding(28, &derived_thermal),
            ],
        });
        let shader: wgpu::ShaderModule = crate::simulation::create_simulation_shader_module(
            device,
            "fluid simulation shader",
            concat!(
                include_str!("fluids_shader_header.wgsl"),
                include_str!("fluids_shader_particle_operations.wgsl"),
                include_str!("fluids_shader_streaming.wgsl"),
                include_str!("fluids_shader_sampling.wgsl"),
                include_str!("fluids_shader_collision.wgsl"),
                include_str!("fluids_shader_residency.wgsl"),
            ),
            "engine_physics/src/simulation_fluids/fluids.wgsl",
        );
        let pipeline_layout: wgpu::PipelineLayout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("fluid simulation pipeline layout"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let accelerator_edit_prepare_layout: wgpu::BindGroupLayout = device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Accelerator fluid edit preparation layout"),
                entries: &[storage(0, false)],
            });
        let accelerator_edit_prepare_bind_group: wgpu::BindGroup =
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Accelerator fluid edit preparation"),
                layout: &accelerator_edit_prepare_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: accelerator_edit_dispatch.as_entire_binding(),
                }],
            });
        let accelerator_edit_prepare_pipeline_layout: wgpu::PipelineLayout = device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Accelerator fluid edit preparation"),
                bind_group_layouts: &[Some(&layout), Some(&accelerator_edit_prepare_layout)],
                immediate_size: 0,
            });
        let pipeline: &dyn Fn(&'static str, &'static str) -> wgpu::ComputePipeline =
            &|entry_point: &'static str, label: &'static str| -> wgpu::ComputePipeline {
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(label),
                    layout: Some(&pipeline_layout),
                    module: &shader,
                    entry_point: Some(entry_point),
                    compilation_options: Default::default(),
                    cache: None,
                })
            };
        let prepare_accelerator_edits_pipeline: wgpu::ComputePipeline = device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Accelerator fluid edit preparation pipeline"),
                layout: Some(&accelerator_edit_prepare_pipeline_layout),
                module: &shader,
                entry_point: Some("prepare_accelerator_fluid_edits"),
                compilation_options: Default::default(),
                cache: None,
            });
        Self {
            particles,
            free_indices,
            free_count,
            edit_cells,
            edit_amounts,
            edit_temperatures,
            accelerator_edits_pending,
            bucket_heads,
            next_particle,
            predicted_positions,
            lambdas,
            position_corrections,
            derived_material_identifiers,
            derived_coverage,
            derived_velocity,
            derived_thermal,
            mechanical_cells,
            mechanical_original_velocity,
            streaming_particles,
            streaming_count,
            streaming_results,
            sample_output,
            fluid_simulation_parameters,
            bind_group,
            accelerator_edit_prepare_bind_group,
            edit_remove_pipeline: pipeline(
                "remove_edited_fluid_particles",
                "fluid edit removal pipeline",
            ),
            edit_spawn_pipeline: pipeline(
                "spawn_edited_fluid_particles",
                "fluid edit spawn pipeline",
            ),
            edit_clear_pipeline: pipeline("clear_fluid_edits", "fluid edit clear pipeline"),
            prepare_accelerator_edits_pipeline,
            accelerator_edit_dispatch,
            predict_pipeline: pipeline("predict_fluid_particles", "fluid prediction pipeline"),
            classify_active_pipeline: pipeline(
                "classify_active_fluid_particles",
                "fluid active classification pipeline",
            ),
            clear_buckets_pipeline: pipeline("clear_fluid_buckets", "fluid bucket clear pipeline"),
            insert_buckets_pipeline: pipeline(
                "insert_committed_fluid_particles_into_spatial_buckets",
                "fluid bucket insertion pipeline",
            ),
            insert_predicted_buckets_pipeline: pipeline(
                "insert_predicted_fluid_particles_into_spatial_buckets",
                "predicted fluid bucket insertion pipeline",
            ),
            lambda_pipeline: pipeline(
                "calculate_fluid_density_constraint_lambdas",
                "fluid lambda pipeline",
            ),
            position_correction_pipeline: pipeline(
                "calculate_fluid_position_corrections",
                "fluid position correction pipeline",
            ),
            apply_position_correction_pipeline: pipeline(
                "apply_fluid_position_corrections",
                "fluid position correction application pipeline",
            ),
            commit_pipeline: pipeline("commit_fluid_particles", "fluid commit pipeline"),
            velocity_smoothing_pipeline: pipeline(
                "calculate_fluid_velocity_smoothing",
                "fluid velocity smoothing pipeline",
            ),
            apply_velocity_smoothing_pipeline: pipeline(
                "apply_fluid_velocity_smoothing",
                "fluid velocity smoothing application pipeline",
            ),
            cell_contact_pipeline: pipeline(
                "resolve_fluid_cellular_contact_velocity",
                "fluid cell contact pipeline",
            ),
            apply_cell_contact_pipeline: pipeline(
                "apply_fluid_cellular_contact_velocity_to_particles",
                "fluid cell contact application pipeline",
            ),
            raster_pipeline: pipeline(
                "rasterize_fluid_particle_coverage_into_cells",
                "fluid cellular raster pipeline",
            ),
            mechanical_scatter_pipeline: pipeline(
                "scatter_fluid_mechanical_response",
                "fluid mechanical response scatter pipeline",
            ),
            sample_pipeline: pipeline(
                "sample_fluid_state_inside_pawn_capsule",
                "pawn fluid sample pipeline",
            ),
            export_pipeline: pipeline("export_fluid_particles", "fluid export pipeline"),
            import_pipeline: pipeline("import_fluid_particles", "fluid import pipeline"),
            particle_capacity,
            buffered_cell_count,
            bucket_count,
            bucket_dimensions,
        }
    }
}
