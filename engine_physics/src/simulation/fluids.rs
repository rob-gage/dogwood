// Copyright Rob Gage 2026

use crate::scenes::{FluidDownload, FluidUpload};
use crate::{
    actors::ActorCollisionShape,
    chunks::ChunkFluidParticle,
    tiles::{TileArea, TileCoordinates},
};
use engine_compute::{Accelerator, AcceleratorBuffer};
use std::io;

const SUPPORT_RADIUS_CELLS: f32 = 2.5;
const PARTICLE_RADIUS_CELLS: f32 = 0.45;
const MAXIMUM_MOVEMENT_CELLS: u32 = 4;
const MAXIMUM_CORRECTION_CELLS: f32 = 0.25;
const FLUID_EDIT_ERASE: u32 = 1;
const PBF_SUBSTEP_COUNT: u32 = 2;
const PBF_CONSTRAINT_ITERATION_COUNT: u32 = 4;

/// Owns authoritative fluid particles and their transient spatial/cellular representations.
pub struct Fluids {
    /// Fixed-capacity authoritative material, position, and velocity records
    particles: AcceleratorBuffer,
    /// Stack storage containing currently reusable particle indices
    free_indices: AcceleratorBuffer,
    /// Atomic number of indices currently available on the free stack
    free_count: AcceleratorBuffer,
    /// Ring-aligned transient fluid spawn or erase value for each cell
    edit_cells: AcceleratorBuffer,
    edit_amounts: AcceleratorBuffer,
    edit_temperatures: AcceleratorBuffer,
    /// Set by GPU producers when `edit_cells` contains one or more edits.
    gpu_edits_pending: AcceleratorBuffer,
    /// Atomic head of each support-radius-sized spatial bucket
    bucket_heads: AcceleratorBuffer,
    /// Linked-list successor for each particle in the current spatial buckets
    next_particle: AcceleratorBuffer,
    /// Predicted position read by every constraint pass in the current substep
    predicted_positions: AcceleratorBuffer,
    /// Density-constraint multiplier calculated for each active particle
    lambdas: AcceleratorBuffer,
    /// Position correction calculated without mutating predicted positions
    position_corrections: AcceleratorBuffer,
    /// Ring-aligned fluid material derived from nearby particles
    derived_material_identifiers: AcceleratorBuffer,
    /// Ring-aligned coverage derived from nearby particles
    derived_coverage: AcceleratorBuffer,
    /// Ring-aligned weighted average velocity derived from nearby particles
    derived_velocity: AcceleratorBuffer,
    derived_thermal: AcceleratorBuffer,
    mechanical_cells: AcceleratorBuffer,
    mechanical_original_velocity: AcceleratorBuffer,
    /// Fixed-capacity records used only during residency ownership transfers
    streaming_particles: AcceleratorBuffer,
    /// Atomic export count or immutable import count for the current transfer
    streaming_count: AcceleratorBuffer,
    /// Per-record success flags for a fluid import
    streaming_results: AcceleratorBuffer,
    /// One compact reduction of the possessed pawn capsule over derived fluid cells
    sample_output: AcceleratorBuffer,
    /// Current ring mapping, spatial dimensions, gravity, and fixed-step values
    parameters: wgpu::Buffer,
    /// All concrete particle, edit, collision, bucket, and derived-cell bindings
    bind_group: wgpu::BindGroup,
    gpu_edit_prepare_bind_group: wgpu::BindGroup,
    /// Removes authoritative particles from edited cells
    edit_remove_pipeline: wgpu::ComputePipeline,
    /// Claims free slots for requested fluid cells
    edit_spawn_pipeline: wgpu::ComputePipeline,
    /// Clears transient edit values after they are consumed
    edit_clear_pipeline: wgpu::ComputePipeline,
    prepare_gpu_edits_pipeline: wgpu::ComputePipeline,
    gpu_edit_dispatch: wgpu::Buffer,
    /// Integrates gravity into predicted positions without replacing authoritative positions
    predict_pipeline: wgpu::ComputePipeline,
    /// Snapshots active-area membership once for the entire fixed tick
    classify_active_pipeline: wgpu::ComputePipeline,
    /// Clears linked-list bucket heads before rebuilding the spatial grid
    clear_buckets_pipeline: wgpu::ComputePipeline,
    /// Inserts active resident particles into support-radius-sized buckets
    insert_buckets_pipeline: wgpu::ComputePipeline,
    /// Inserts active particles by predicted position during constraint iterations
    insert_predicted_buckets_pipeline: wgpu::ComputePipeline,
    /// Calculates density constraints and multipliers from predicted neighbors
    lambda_pipeline: wgpu::ComputePipeline,
    /// Calculates race-free predicted-position corrections
    position_correction_pipeline: wgpu::ComputePipeline,
    /// Applies corrections and projects predicted positions out of solid boundaries
    apply_position_correction_pipeline: wgpu::ComputePipeline,
    /// Commits corrected positions and reconstructs authoritative velocity
    commit_pipeline: wgpu::ComputePipeline,
    /// Calculates a small neighbor-weighted velocity smoothing correction
    velocity_smoothing_pipeline: wgpu::ComputePipeline,
    /// Applies the completed velocity smoothing correction without neighbor races
    apply_velocity_smoothing_pipeline: wgpu::ComputePipeline,
    /// Resolves hard contact and swimmer entrainment in the derived cellular representation
    cell_contact_pipeline: wgpu::ComputePipeline,
    /// Applies one derived-cell contact correction to each authoritative particle
    apply_cell_contact_pipeline: wgpu::ComputePipeline,
    /// Gathers nearby particles into ring-aligned derived cell fields
    raster_pipeline: wgpu::ComputePipeline,
    mechanical_scatter_pipeline: wgpu::ComputePipeline,
    /// Reduces final derived fluid state across the possessed pawn capsule
    sample_pipeline: wgpu::ComputePipeline,
    /// Compacts and removes particles belonging to an outgoing tile strip
    export_pipeline: wgpu::ComputePipeline,
    /// Reconstructs imported records through the existing free-particle stack
    import_pipeline: wgpu::ComputePipeline,
    /// Fixed number of authoritative particle slots
    particle_capacity: u32,
    /// Number of physical cells in the buffered tile ring
    buffered_cell_count: u32,
    /// Number of fluid spatial buckets
    bucket_count: u32,
    /// Fluid spatial-grid dimensions, independent from tile dimensions
    bucket_dimensions: [u32; 2],
}

/// Internal read view of the authoritative fluid spatial index.  Simulation
/// subsystems use this instead of reimplementing particle-to-cell mapping.
pub(crate) struct FluidAuthorityView<'a> {
    pub(crate) particles: &'a AcceleratorBuffer,
    pub(crate) bucket_heads: &'a AcceleratorBuffer,
    pub(crate) next_particle: &'a AcceleratorBuffer,
    pub(crate) parameters: &'a wgpu::Buffer,
}

impl Fluids {
    /// Creates the fixed fluid pool and its concrete GPU simulation resources
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
        let edit_amounts = accelerator.allocate::<f32>(buffered_cell_count as usize);
        let edit_temperatures = accelerator.allocate::<f32>(buffered_cell_count as usize);
        let gpu_edits_pending: AcceleratorBuffer = accelerator.allocate::<u32>(1);
        let gpu_edit_dispatch = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU fluid edit indirect dispatch"),
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
        let parameters: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fluid simulation parameters"),
            size: 128,
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
                    wgpu::BindGroupLayoutEntry {
                        binding: 12,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
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
                    wgpu::BindGroupLayoutEntry {
                        binding: 27,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
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
                    resource: parameters.as_entire_binding(),
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
                Self::binding(23, &gpu_edits_pending),
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
        let shader: wgpu::ShaderModule = super::create_simulation_shader_module(
            device,
            "fluid simulation shader",
            include_str!("fluids.wgsl"),
            "engine_physics/src/simulation/fluids.wgsl",
        );
        let pipeline_layout: wgpu::PipelineLayout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("fluid simulation pipeline layout"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let gpu_edit_prepare_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("GPU fluid edit preparation layout"),
                entries: &[storage(0, false)],
            });
        let gpu_edit_prepare_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GPU fluid edit preparation"),
            layout: &gpu_edit_prepare_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: gpu_edit_dispatch.as_entire_binding(),
            }],
        });
        let gpu_edit_prepare_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("GPU fluid edit preparation"),
                bind_group_layouts: &[Some(&layout), Some(&gpu_edit_prepare_layout)],
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
        let prepare_gpu_edits_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("GPU fluid edit preparation pipeline"),
                layout: Some(&gpu_edit_prepare_pipeline_layout),
                module: &shader,
                entry_point: Some("prepare_gpu_fluid_edits"),
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
            gpu_edits_pending,
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
            parameters,
            bind_group,
            gpu_edit_prepare_bind_group,
            edit_remove_pipeline: pipeline(
                "remove_edited_fluid_particles",
                "fluid edit removal pipeline",
            ),
            edit_spawn_pipeline: pipeline(
                "spawn_edited_fluid_particles",
                "fluid edit spawn pipeline",
            ),
            edit_clear_pipeline: pipeline("clear_fluid_edits", "fluid edit clear pipeline"),
            prepare_gpu_edits_pipeline,
            gpu_edit_dispatch,
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

    /// Returns the transient ring-aligned derived fluid material buffer
    pub const fn material_identifiers_buffer(&self) -> &AcceleratorBuffer {
        &self.derived_material_identifiers
    }

    /// Returns the transient ring-aligned derived fluid coverage buffer
    pub const fn coverage_buffer(&self) -> &AcceleratorBuffer {
        &self.derived_coverage
    }

    /// Returns the transient ring-aligned derived average velocity buffer
    pub const fn velocity_buffer(&self) -> &AcceleratorBuffer {
        &self.derived_velocity
    }

    pub(crate) const fn mechanical_cells_buffer(&self) -> &AcceleratorBuffer {
        &self.mechanical_cells
    }

    pub(crate) const fn particles_buffer(&self) -> &AcceleratorBuffer {
        &self.particles
    }

    pub(crate) const fn authority_view(&self) -> FluidAuthorityView<'_> {
        FluidAuthorityView {
            particles: &self.particles,
            bucket_heads: &self.bucket_heads,
            next_particle: &self.next_particle,
            parameters: &self.parameters,
        }
    }
    /// Finalizes a slot reserved by an asynchronous producer.  The slot was
    /// removed from the free stack before this call, so this cannot race a
    /// normal spawn allocation.
    pub(crate) fn commit_reserved_particle(
        &self,
        accelerator: &Accelerator,
        slot: u32,
        material: u32,
        position: [f32; 2],
        velocity: [f32; 2],
        amount: f32,
        temperature: f32,
    ) {
        if slot >= self.particle_capacity {
            return;
        }
        let record = [
            material,
            1,
            position[0].to_bits(),
            position[1].to_bits(),
            velocity[0].to_bits(),
            velocity[1].to_bits(),
            0,
            0,
            amount.to_bits(),
            temperature.to_bits(),
        ];
        accelerator.wgpu_queue().write_buffer(
            self.particles.wgpu_buffer(),
            u64::from(slot) * 40,
            &record
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        );
    }
    pub(crate) const fn free_indices_buffer(&self) -> &AcceleratorBuffer {
        &self.free_indices
    }
    pub(crate) const fn free_count_buffer(&self) -> &AcceleratorBuffer {
        &self.free_count
    }

    pub(crate) const fn edit_cells_buffer(&self) -> &AcceleratorBuffer {
        &self.edit_cells
    }
    pub(crate) const fn edit_amounts_buffer(&self) -> &AcceleratorBuffer {
        &self.edit_amounts
    }
    pub(crate) const fn edit_temperatures_buffer(&self) -> &AcceleratorBuffer {
        &self.edit_temperatures
    }

    pub(crate) const fn gpu_edits_pending_buffer(&self) -> &AcceleratorBuffer {
        &self.gpu_edits_pending
    }

    /// Consumes edits written by another GPU subsystem using the same authoritative pool.
    pub(crate) fn consume_gpu_edits(
        &self,
        accelerator: &Accelerator,
        active_origin: TileCoordinates,
        active_width: u16,
        active_height: u16,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) {
        self.write_parameters(
            accelerator,
            active_origin,
            active_width,
            active_height,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            None,
            [0.0; 2],
            0.0,
            None,
        );
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("GPU fluid edits"),
                });
        {
            let mut pass = accelerator.begin_compute_pass(&mut encoder, "prepare GPU fluid edits");
            pass.set_pipeline(&self.prepare_gpu_edits_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_bind_group(1, &self.gpu_edit_prepare_bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        self.dispatch_indirect(
            accelerator,
            &mut encoder,
            &self.edit_remove_pipeline,
            0,
            "remove GPU edited fluid particles",
        );
        self.dispatch_indirect(
            accelerator,
            &mut encoder,
            &self.edit_spawn_pipeline,
            12,
            "spawn GPU edited fluid particles",
        );
        self.dispatch_indirect(
            accelerator,
            &mut encoder,
            &self.edit_clear_pipeline,
            24,
            "clear GPU fluid edits",
        );
        self.encode_rebuild_indirect(accelerator, &mut encoder);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Consumes ring-aligned spawn/erase edits and immediately refreshes derived cells
    pub fn apply_edits(
        &self,
        accelerator: &Accelerator,
        edits: &[(usize, u32, f32, f32)],
        active_origin: TileCoordinates,
        active_width: u16,
        active_height: u16,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) {
        // One bounded upload per aggregate scene-edit flush. The edit shader already
        // scans this dense ring buffer, so a sparse sequence of tiny writes buys nothing.
        let mut cells = vec![
            crate::materials::MaterialIdentifier::NULL.as_u32();
            self.buffered_cell_count as usize
        ];
        for (index, material_identifier, _, _) in edits {
            cells[*index] = *material_identifier;
        }
        let bytes: Vec<u8> = cells.into_iter().flat_map(u32::to_le_bytes).collect();
        accelerator
            .wgpu_queue()
            .write_buffer(self.edit_cells.wgpu_buffer(), 0, &bytes);
        let mut amounts = vec![0.0f32.to_bits(); self.buffered_cell_count as usize];
        let mut temperatures = vec![0.0f32.to_bits(); self.buffered_cell_count as usize];
        for (index, _, amount, temperature) in edits {
            amounts[*index] = amount.to_bits();
            temperatures[*index] = temperature.to_bits();
        }
        accelerator.wgpu_queue().write_buffer(
            &self.edit_amounts.wgpu_buffer(),
            0,
            &amounts
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        accelerator.wgpu_queue().write_buffer(
            &self.edit_temperatures.wgpu_buffer(),
            0,
            &temperatures
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        self.write_parameters(
            accelerator,
            active_origin,
            active_width,
            active_height,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            None,
            [0.0; 2],
            0.0,
            None,
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("fluid edits"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.edit_remove_pipeline,
            self.particle_capacity,
            "remove edited fluid particles",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.edit_spawn_pipeline,
            self.buffered_cell_count,
            "spawn edited fluid particles",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.edit_clear_pipeline,
            self.buffered_cell_count,
            "clear fluid edits",
        );
        self.encode_rebuild(accelerator, &mut encoder);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Moves authoritative particles and regenerates all transient derived state
    pub fn simulate(
        &self,
        accelerator: &Accelerator,
        active_origin: TileCoordinates,
        active_width: u16,
        active_height: u16,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        gravity: [f32; 2],
        delta_time: f32,
    ) {
        self.write_parameters(
            accelerator,
            active_origin,
            active_width,
            active_height,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            None,
            gravity,
            delta_time,
            None,
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("fluid simulation"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.classify_active_pipeline,
            self.particle_capacity,
            "classify active fluid particles",
        );
        for _ in 0..PBF_SUBSTEP_COUNT {
            self.dispatch(
                accelerator,
                &mut encoder,
                &self.predict_pipeline,
                self.particle_capacity,
                "predict fluid particles",
            );
            for _ in 0..PBF_CONSTRAINT_ITERATION_COUNT {
                self.dispatch(
                    accelerator,
                    &mut encoder,
                    &self.clear_buckets_pipeline,
                    self.bucket_count,
                    "clear predicted fluid buckets",
                );
                self.dispatch(
                    accelerator,
                    &mut encoder,
                    &self.insert_predicted_buckets_pipeline,
                    self.particle_capacity,
                    "insert predicted fluid particles into buckets",
                );
                self.dispatch(
                    accelerator,
                    &mut encoder,
                    &self.lambda_pipeline,
                    self.particle_capacity,
                    "calculate fluid lambdas",
                );
                self.dispatch(
                    accelerator,
                    &mut encoder,
                    &self.position_correction_pipeline,
                    self.particle_capacity,
                    "calculate fluid position corrections",
                );
                self.dispatch(
                    accelerator,
                    &mut encoder,
                    &self.apply_position_correction_pipeline,
                    self.particle_capacity,
                    "apply fluid position corrections",
                );
            }
            self.dispatch(
                accelerator,
                &mut encoder,
                &self.commit_pipeline,
                self.particle_capacity,
                "commit fluid particles",
            );
        }
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.clear_buckets_pipeline,
            self.bucket_count,
            "clear final fluid buckets",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.insert_buckets_pipeline,
            self.particle_capacity,
            "insert final fluid particles into buckets",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.velocity_smoothing_pipeline,
            self.particle_capacity,
            "calculate fluid velocity smoothing",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.apply_velocity_smoothing_pipeline,
            self.particle_capacity,
            "apply fluid velocity smoothing",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.raster_pipeline,
            self.buffered_cell_count,
            "rasterize derived fluid cells",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.cell_contact_pipeline,
            self.buffered_cell_count,
            "resolve fluid cell interactions",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.apply_cell_contact_pipeline,
            self.particle_capacity,
            "apply fluid cell contacts",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.raster_pipeline,
            self.buffered_cell_count,
            "refresh contacted fluid cells",
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    pub(crate) fn scatter_mechanical_response(&self, accelerator: &Accelerator) {
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("fluid mechanical response"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.mechanical_scatter_pipeline,
            self.particle_capacity,
            "scatter mechanical fluid velocity",
        );
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.raster_pipeline,
            self.buffered_cell_count,
            "refresh solved fluid coverage",
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Samples final derived fluid state across one gravity-relative pawn shape
    pub fn sample_pawn(
        &self,
        accelerator: &Accelerator,
        output: &wgpu::Buffer,
        center: [f32; 2],
        shape: ActorCollisionShape,
        active_origin: TileCoordinates,
        active_width: u16,
        active_height: u16,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        gravity: [f32; 2],
    ) {
        self.write_parameters(
            accelerator,
            active_origin,
            active_width,
            active_height,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            None,
            gravity,
            0.0,
            Some((center, shape)),
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("pawn fluid sample"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.sample_pipeline,
            1,
            "sample pawn fluid",
        );
        encoder.copy_buffer_to_buffer(self.sample_output.wgpu_buffer(), 0, output, 0, 32);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Rebuilds buckets and derived cells after a ring remap without moving particles
    pub fn refresh(
        &self,
        accelerator: &Accelerator,
        active_origin: TileCoordinates,
        active_width: u16,
        active_height: u16,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) {
        self.write_parameters(
            accelerator,
            active_origin,
            active_width,
            active_height,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            None,
            [0.0; 2],
            0.0,
            None,
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("fluid cellular refresh"),
                });
        self.encode_rebuild(accelerator, &mut encoder);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Returns the maximum number of authoritative resident particles
    pub const fn particle_capacity(&self) -> u32 {
        self.particle_capacity
    }

    pub(crate) const fn derived_thermal_buffer(&self) -> &AcceleratorBuffer {
        &self.derived_thermal
    }

    /// Returns the tile buffer required by active movement and PBF support
    pub fn minimum_buffer_tiles() -> u8 {
        let predicted_movement: f32 = MAXIMUM_MOVEMENT_CELLS as f32 / PBF_SUBSTEP_COUNT as f32
            + MAXIMUM_CORRECTION_CELLS * PBF_CONSTRAINT_ITERATION_COUNT as f32;
        ((SUPPORT_RADIUS_CELLS * 2.0 + predicted_movement) / 8.0).ceil() as u8
    }

    /// Compacts outgoing authoritative records, releases their slots, and copies them for readback
    pub fn export(
        &self,
        accelerator: &Accelerator,
        download: &FluidDownload,
        active_origin: TileCoordinates,
        active_width: u16,
        active_height: u16,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) {
        accelerator.wgpu_queue().write_buffer(
            self.streaming_count.wgpu_buffer(),
            0,
            &0u32.to_le_bytes(),
        );
        self.write_parameters(
            accelerator,
            active_origin,
            active_width,
            active_height,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            Some(download.area),
            [0.0; 2],
            0.0,
            None,
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("fluid export"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.export_pipeline,
            self.particle_capacity,
            "compact outgoing fluid particles",
        );
        encoder.copy_buffer_to_buffer(
            self.streaming_count.wgpu_buffer(),
            0,
            &download.buffer,
            0,
            4,
        );
        encoder.copy_buffer_to_buffer(
            self.streaming_particles.wgpu_buffer(),
            0,
            &download.buffer,
            16,
            u64::from(self.particle_capacity) * ChunkFluidParticle::GPU_SIZE as u64,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Reconstructs dormant records and copies their exact success flags for readback
    pub fn import(
        &self,
        accelerator: &Accelerator,
        upload: &FluidUpload,
        active_origin: TileCoordinates,
        active_width: u16,
        active_height: u16,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) -> Result<(), io::Error> {
        let mut bytes: Vec<u8> =
            Vec::with_capacity(upload.particles.len() * ChunkFluidParticle::GPU_SIZE);
        for particle in &upload.particles {
            particle.serialize_gpu(&mut bytes)?;
        }
        accelerator
            .wgpu_queue()
            .write_buffer(self.streaming_particles.wgpu_buffer(), 0, &bytes);
        accelerator.wgpu_queue().write_buffer(
            self.streaming_count.wgpu_buffer(),
            0,
            &(upload.particles.len() as u32).to_le_bytes(),
        );
        self.write_parameters(
            accelerator,
            active_origin,
            active_width,
            active_height,
            buffered_origin,
            buffered_width,
            buffered_height,
            ring_offset_x,
            ring_offset_y,
            None,
            [0.0; 2],
            0.0,
            None,
        );
        let mut encoder: wgpu::CommandEncoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("fluid import"),
                });
        self.dispatch(
            accelerator,
            &mut encoder,
            &self.import_pipeline,
            upload.particles.len() as u32,
            "import dormant fluid particles",
        );
        self.encode_rebuild(accelerator, &mut encoder);
        encoder.copy_buffer_to_buffer(
            self.streaming_results.wgpu_buffer(),
            0,
            &upload.buffer,
            0,
            upload.particles.len() as u64 * 4,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        Ok(())
    }

    fn encode_rebuild(&self, accelerator: &Accelerator, encoder: &mut wgpu::CommandEncoder) {
        self.dispatch(
            accelerator,
            encoder,
            &self.clear_buckets_pipeline,
            self.bucket_count,
            "clear fluid buckets",
        );
        self.dispatch(
            accelerator,
            encoder,
            &self.insert_buckets_pipeline,
            self.particle_capacity,
            "insert fluid particles into buckets",
        );
        self.dispatch(
            accelerator,
            encoder,
            &self.raster_pipeline,
            self.buffered_cell_count,
            "rasterize derived fluid cells",
        );
    }

    fn encode_rebuild_indirect(
        &self,
        accelerator: &Accelerator,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        self.dispatch_indirect(
            accelerator,
            encoder,
            &self.clear_buckets_pipeline,
            36,
            "clear GPU edited fluid buckets",
        );
        self.dispatch_indirect(
            accelerator,
            encoder,
            &self.insert_buckets_pipeline,
            48,
            "insert GPU edited fluid particles",
        );
        self.dispatch_indirect(
            accelerator,
            encoder,
            &self.raster_pipeline,
            60,
            "rasterize GPU edited fluid cells",
        );
    }

    fn dispatch(
        &self,
        accelerator: &Accelerator,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::ComputePipeline,
        count: u32,
        label: &str,
    ) {
        let mut pass: wgpu::ComputePass<'_> = accelerator.begin_compute_pass(encoder, label);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
    }

    fn dispatch_indirect(
        &self,
        accelerator: &Accelerator,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::ComputePipeline,
        offset: u64,
        label: &str,
    ) {
        let mut pass: wgpu::ComputePass<'_> = accelerator.begin_compute_pass(encoder, label);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups_indirect(&self.gpu_edit_dispatch, offset);
    }

    fn write_parameters(
        &self,
        accelerator: &Accelerator,
        active_origin: TileCoordinates,
        active_width: u16,
        active_height: u16,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        streaming_area: Option<TileArea>,
        gravity: [f32; 2],
        delta_time: f32,
        sample: Option<([f32; 2], ActorCollisionShape)>,
    ) {
        let streaming_origin: TileCoordinates =
            streaming_area.map_or(TileCoordinates { x: 0, y: 0 }, TileArea::origin);
        let streaming_dimensions: [u16; 2] = streaming_area.map_or([0, 0], TileArea::dimensions);
        let (sample_center, sample_kind, sample_parameters) =
            sample.map_or(([0.0; 2], 0, [0.0; 2]), |(center, shape)| {
                let (kind, parameters) = shape.gpu_parameters();
                (center, kind, parameters)
            });
        let values: [u32; 32] = [
            buffered_origin.x as u32,
            buffered_origin.y as u32,
            u32::from(buffered_width),
            u32::from(buffered_height),
            active_origin.x as u32,
            active_origin.y as u32,
            u32::from(active_width),
            u32::from(active_height),
            u32::from(ring_offset_x),
            u32::from(ring_offset_y),
            self.bucket_dimensions[0],
            self.bucket_dimensions[1],
            streaming_origin.x as u32,
            streaming_origin.y as u32,
            u32::from(streaming_dimensions[0]),
            u32::from(streaming_dimensions[1]),
            gravity[0].to_bits(),
            gravity[1].to_bits(),
            delta_time.to_bits(),
            self.particle_capacity,
            self.buffered_cell_count,
            self.bucket_count,
            SUPPORT_RADIUS_CELLS.to_bits(),
            PARTICLE_RADIUS_CELLS.to_bits(),
            MAXIMUM_MOVEMENT_CELLS,
            0,
            sample_center[0].to_bits(),
            sample_center[1].to_bits(),
            sample_parameters[0].to_bits(),
            sample_parameters[1].to_bits(),
            sample_kind,
            0,
        ];
        let bytes: Vec<u8> = values.into_iter().flat_map(u32::to_le_bytes).collect();
        accelerator
            .wgpu_queue()
            .write_buffer(&self.parameters, 0, &bytes);
    }

    fn binding<'a>(binding: u32, buffer: &'a AcceleratorBuffer) -> wgpu::BindGroupEntry<'a> {
        wgpu::BindGroupEntry {
            binding,
            resource: buffer.wgpu_buffer().as_entire_binding(),
        }
    }

    /// Returns the transient edit value used to remove fluid without spawning it
    pub const fn erase_edit() -> u32 {
        FLUID_EDIT_ERASE
    }
}

impl Drop for Fluids {
    fn drop(&mut self) {
        self.particles.free();
        self.free_indices.free();
        self.free_count.free();
        self.edit_cells.free();
        self.edit_amounts.free();
        self.edit_temperatures.free();
        self.gpu_edits_pending.free();
        self.bucket_heads.free();
        self.next_particle.free();
        self.predicted_positions.free();
        self.lambdas.free();
        self.position_corrections.free();
        self.derived_material_identifiers.free();
        self.derived_coverage.free();
        self.derived_velocity.free();
        self.derived_thermal.free();
        self.mechanical_cells.free();
        self.mechanical_original_velocity.free();
        self.streaming_particles.free();
        self.streaming_count.free();
        self.streaming_results.free();
        self.sample_output.free();
        self.parameters.destroy();
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::materials::{MaterialForm, MaterialIdentifier};
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    #[test]
    fn mechanical_raster_and_scatter_pipelines_compile_on_gpu() {
        let _gpu_test = crate::GPU_TEST_LOCK.lock().unwrap();
        let accelerator = Accelerator::new().unwrap();
        let cells = accelerator.allocate::<u32>(64);
        let occupancy = accelerator.allocate::<u32>(64);
        let velocity = accelerator.allocate::<[f32; 4]>(64);
        let properties = accelerator.allocate::<[f32; 4]>(2);
        let thermal_properties = accelerator.allocate::<[u32; 16]>(1);
        let thermal_parameters = accelerator
            .wgpu_device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        let fluids = Fluids::new(
            &accelerator,
            &cells,
            &occupancy,
            &velocity,
            &properties,
            &thermal_properties,
            &thermal_parameters,
            1,
            1,
        );
        fluids.consume_gpu_edits(
            &accelerator,
            TileCoordinates { x: 0, y: 0 },
            1,
            1,
            TileCoordinates { x: 0, y: 0 },
            1,
            1,
            0,
            0,
        );
        accelerator.poll().unwrap();
        drop(fluids);
    }

    #[test]
    fn solved_mechanical_delta_persists_in_authoritative_particle() {
        let _gpu_test = crate::GPU_TEST_LOCK.lock().unwrap();
        let accelerator = Accelerator::new().unwrap();
        let cells = accelerator.allocate::<u32>(64);
        let occupancy = accelerator.allocate::<u32>(64);
        let velocity = accelerator.allocate::<[f32; 4]>(64);
        let properties = accelerator.allocate::<[f32; 4]>(2);
        let thermal_properties = accelerator.allocate::<[u32; 16]>(1);
        let thermal_parameters = accelerator
            .wgpu_device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        accelerator.wgpu_queue().write_buffer(
            properties.wgpu_buffer(),
            0,
            &[1.0f32, 0.0, 0.0, 4.0, 0.0, 0.0, 1.0, 0.0]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let fluids = Fluids::new(
            &accelerator,
            &cells,
            &occupancy,
            &velocity,
            &properties,
            &thermal_properties,
            &thermal_parameters,
            1,
            1,
        );
        let material = MaterialIdentifier::new(MaterialForm::Fluid, 0).as_u32();
        let particle: [u32; 8] = [material, 1, 0.5f32.to_bits(), 0.5f32.to_bits(), 0, 0, 0, 0];
        accelerator.wgpu_queue().write_buffer(
            fluids.particles.wgpu_buffer(),
            0,
            &particle
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        fluids.simulate(
            &accelerator,
            TileCoordinates { x: 0, y: 0 },
            1,
            1,
            TileCoordinates { x: 0, y: 0 },
            1,
            1,
            0,
            0,
            [0.0, 0.0],
            1.0 / 60.0,
        );
        let cell_index = 4 + 4 * 8;
        accelerator.wgpu_queue().write_buffer(
            fluids.mechanical_cells.wgpu_buffer(),
            cell_index * 16 + 8,
            &1.0f32.to_le_bytes(),
        );
        fluids.scatter_mechanical_response(&accelerator);
        let readback = accelerator
            .wgpu_device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("fluid particle response check"),
                size: 32,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("fluid particle response check"),
                });
        encoder.copy_buffer_to_buffer(fluids.particles.wgpu_buffer(), 0, &readback, 0, 32);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        let (sender, receiver) = mpsc::sync_channel(1);
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        let started = Instant::now();
        loop {
            accelerator.poll().unwrap();
            if receiver.try_recv().is_ok() {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        }
        let mapped = readback.slice(..).get_mapped_range().unwrap();
        let particle_velocity = f32::from_le_bytes(mapped[16..20].try_into().unwrap());
        assert!(
            (particle_velocity - 1.0).abs() < 0.01,
            "mechanical response did not persist: {particle_velocity}"
        );
    }
}
