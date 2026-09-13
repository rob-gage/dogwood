// Copyright Rob Gage 2026

use crate::tiles::TileCoordinates;
use engine_compute::{Accelerator, AcceleratorBuffer};

const SUPPORT_RADIUS_CELLS: f32 = 2.5;
const PARTICLE_RADIUS_CELLS: f32 = 0.35;
const MAXIMUM_MOVEMENT_CELLS: u32 = 4;
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
    /// Current ring mapping, spatial dimensions, gravity, and fixed-step values
    parameters: wgpu::Buffer,
    /// All concrete particle, edit, collision, bucket, and derived-cell bindings
    bind_group: wgpu::BindGroup,
    /// Removes authoritative particles from edited cells
    edit_remove_pipeline: wgpu::ComputePipeline,
    /// Claims free slots for requested fluid cells
    edit_spawn_pipeline: wgpu::ComputePipeline,
    /// Clears transient edit values after they are consumed
    edit_clear_pipeline: wgpu::ComputePipeline,
    /// Integrates gravity into predicted positions without replacing authoritative positions
    predict_pipeline: wgpu::ComputePipeline,
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
    /// Gathers nearby particles into ring-aligned derived cell fields
    raster_pipeline: wgpu::ComputePipeline,
    /// Fixed number of authoritative particle slots
    particle_capacity: u32,
    /// Number of physical cells in the buffered tile ring
    buffered_cell_count: u32,
    /// Number of fluid spatial buckets
    bucket_count: u32,
    /// Fluid spatial-grid dimensions, independent from tile dimensions
    bucket_dimensions: [u32; 2],
}

impl Fluids {

    /// Creates the fixed fluid pool and its concrete GPU simulation resources
    pub fn new(
        accelerator: &Accelerator,
        cellular_material_identifiers: &AcceleratorBuffer,
        external_body_occupancy: &AcceleratorBuffer,
        external_body_velocity: &AcceleratorBuffer,
        buffered_width: u16,
        buffered_height: u16,
    ) -> Self {
        let device: &wgpu::Device = accelerator.wgpu_device();
        let buffered_cell_count: u32 =
            u32::from(buffered_width) * u32::from(buffered_height) * 64;
        let particle_capacity: u32 = buffered_cell_count;
        let bucket_dimensions: [u32; 2] = [
            (f32::from(buffered_width) * 8.0 / SUPPORT_RADIUS_CELLS).ceil() as u32,
            (f32::from(buffered_height) * 8.0 / SUPPORT_RADIUS_CELLS).ceil() as u32,
        ];
        let bucket_count: u32 = bucket_dimensions[0] * bucket_dimensions[1];
        let particles: AcceleratorBuffer =
            accelerator.allocate::<[u32; 8]>(particle_capacity as usize);
        let free_indices: AcceleratorBuffer =
            accelerator.allocate::<u32>(particle_capacity as usize);
        let free_count: AcceleratorBuffer = accelerator.allocate::<u32>(1);
        let edit_cells: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_cell_count as usize);
        let bucket_heads: AcceleratorBuffer =
            accelerator.allocate::<u32>(bucket_count as usize);
        let next_particle: AcceleratorBuffer =
            accelerator.allocate::<u32>(particle_capacity as usize);
        let predicted_positions: AcceleratorBuffer =
            accelerator.allocate::<[f32; 2]>(particle_capacity as usize);
        let lambdas: AcceleratorBuffer =
            accelerator.allocate::<f32>(particle_capacity as usize);
        let position_corrections: AcceleratorBuffer =
            accelerator.allocate::<[f32; 2]>(particle_capacity as usize);
        let derived_material_identifiers: AcceleratorBuffer =
            accelerator.allocate::<u32>(buffered_cell_count as usize);
        let derived_coverage: AcceleratorBuffer =
            accelerator.allocate::<f32>(buffered_cell_count as usize);
        let derived_velocity: AcceleratorBuffer =
            accelerator.allocate::<[f32; 4]>(buffered_cell_count as usize);
        let free_indices_data: Vec<u8> = (0..particle_capacity)
            .flat_map(u32::to_le_bytes).collect();
        accelerator.wgpu_queue().write_buffer(
            free_indices.wgpu_buffer(), 0, &free_indices_data,
        );
        accelerator.wgpu_queue().write_buffer(
            free_count.wgpu_buffer(), 0, &particle_capacity.to_le_bytes(),
        );
        let parameters: wgpu::Buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fluid simulation parameters"),
            size: 80,
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
        let layout: wgpu::BindGroupLayout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("fluid simulation bind group layout"),
                entries: &[
                    storage(0, false), storage(1, false), storage(2, false),
                    storage(3, false), storage(4, false), storage(5, false),
                    storage(6, false), storage(7, false), storage(8, false),
                    storage(9, true), storage(10, true), storage(11, true),
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
                    storage(13, false), storage(14, false), storage(15, false),
                ],
            },
        );
        let bind_group: wgpu::BindGroup = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
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
                ],
            },
        );
        let shader: wgpu::ShaderModule = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: Some("fluid simulation shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("fluids.wgsl").into()),
            },
        );
        let pipeline_layout: wgpu::PipelineLayout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: Some("fluid simulation pipeline layout"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            },
        );
        let pipeline = |entry_point, label| device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            },
        );
        Self {
            particles,
            free_indices,
            free_count,
            edit_cells,
            bucket_heads,
            next_particle,
            predicted_positions,
            lambdas,
            position_corrections,
            derived_material_identifiers,
            derived_coverage,
            derived_velocity,
            parameters,
            bind_group,
            edit_remove_pipeline: pipeline("remove_edited_fluid_particles", "fluid edit removal pipeline"),
            edit_spawn_pipeline: pipeline("spawn_edited_fluid_particles", "fluid edit spawn pipeline"),
            edit_clear_pipeline: pipeline("clear_fluid_edits", "fluid edit clear pipeline"),
            predict_pipeline: pipeline("predict_fluid_particles", "fluid prediction pipeline"),
            clear_buckets_pipeline: pipeline("clear_fluid_buckets", "fluid bucket clear pipeline"),
            insert_buckets_pipeline: pipeline("insert_fluid_particles", "fluid bucket insertion pipeline"),
            insert_predicted_buckets_pipeline: pipeline("insert_predicted_fluid_particles",
                "predicted fluid bucket insertion pipeline"),
            lambda_pipeline: pipeline("calculate_fluid_lambdas", "fluid lambda pipeline"),
            position_correction_pipeline: pipeline("calculate_fluid_position_corrections",
                "fluid position correction pipeline"),
            apply_position_correction_pipeline: pipeline("apply_fluid_position_corrections",
                "fluid position correction application pipeline"),
            commit_pipeline: pipeline("commit_fluid_particles", "fluid commit pipeline"),
            velocity_smoothing_pipeline: pipeline("calculate_fluid_velocity_smoothing",
                "fluid velocity smoothing pipeline"),
            apply_velocity_smoothing_pipeline: pipeline("apply_fluid_velocity_smoothing",
                "fluid velocity smoothing application pipeline"),
            raster_pipeline: pipeline("rasterize_fluid_cells", "fluid cellular raster pipeline"),
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

    /// Consumes ring-aligned spawn/erase edits and immediately refreshes derived cells
    pub fn apply_edits(
        &self,
        accelerator: &Accelerator,
        edits: &[(usize, u32)],
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) {
        for (index, material_identifier) in edits {
            accelerator.wgpu_queue().write_buffer(
                self.edit_cells.wgpu_buffer(), *index as u64 * 4,
                &material_identifier.to_le_bytes(),
            );
        }
        self.write_parameters(
            accelerator, buffered_origin, buffered_width, buffered_height,
            ring_offset_x, ring_offset_y, [0.0; 2], 0.0,
        );
        let mut encoder: wgpu::CommandEncoder = accelerator.wgpu_device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("fluid edits") },
        );
        self.dispatch(&mut encoder, &self.edit_remove_pipeline, self.particle_capacity,
            "remove edited fluid particles");
        self.dispatch(&mut encoder, &self.edit_spawn_pipeline, self.buffered_cell_count,
            "spawn edited fluid particles");
        self.dispatch(&mut encoder, &self.edit_clear_pipeline, self.buffered_cell_count,
            "clear fluid edits");
        self.encode_rebuild(&mut encoder);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Moves authoritative particles and regenerates all transient derived state
    pub fn simulate(
        &self,
        accelerator: &Accelerator,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        gravity: [f32; 2],
        delta_time: f32,
    ) {
        self.write_parameters(
            accelerator, buffered_origin, buffered_width, buffered_height,
            ring_offset_x, ring_offset_y, gravity, delta_time,
        );
        let mut encoder: wgpu::CommandEncoder = accelerator.wgpu_device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("fluid simulation") },
        );
        for _ in 0..PBF_SUBSTEP_COUNT {
            self.dispatch(&mut encoder, &self.predict_pipeline, self.particle_capacity,
                "predict fluid particles");
            for _ in 0..PBF_CONSTRAINT_ITERATION_COUNT {
                self.dispatch(&mut encoder, &self.clear_buckets_pipeline, self.bucket_count,
                    "clear predicted fluid buckets");
                self.dispatch(&mut encoder, &self.insert_predicted_buckets_pipeline,
                    self.particle_capacity, "insert predicted fluid particles into buckets");
                self.dispatch(&mut encoder, &self.lambda_pipeline, self.particle_capacity,
                    "calculate fluid lambdas");
                self.dispatch(&mut encoder, &self.position_correction_pipeline,
                    self.particle_capacity, "calculate fluid position corrections");
                self.dispatch(&mut encoder, &self.apply_position_correction_pipeline,
                    self.particle_capacity, "apply fluid position corrections");
            }
            self.dispatch(&mut encoder, &self.commit_pipeline, self.particle_capacity,
                "commit fluid particles");
        }
        self.dispatch(&mut encoder, &self.clear_buckets_pipeline, self.bucket_count,
            "clear final fluid buckets");
        self.dispatch(&mut encoder, &self.insert_buckets_pipeline, self.particle_capacity,
            "insert final fluid particles into buckets");
        self.dispatch(&mut encoder, &self.velocity_smoothing_pipeline, self.particle_capacity,
            "calculate fluid velocity smoothing");
        self.dispatch(&mut encoder, &self.apply_velocity_smoothing_pipeline,
            self.particle_capacity, "apply fluid velocity smoothing");
        self.dispatch(&mut encoder, &self.raster_pipeline, self.buffered_cell_count,
            "rasterize derived fluid cells");
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    /// Rebuilds buckets and derived cells after a ring remap without moving particles
    pub fn refresh(
        &self,
        accelerator: &Accelerator,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
    ) {
        self.write_parameters(
            accelerator, buffered_origin, buffered_width, buffered_height,
            ring_offset_x, ring_offset_y, [0.0; 2], 0.0,
        );
        let mut encoder: wgpu::CommandEncoder = accelerator.wgpu_device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("fluid cellular refresh") },
        );
        self.encode_rebuild(&mut encoder);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
    }

    fn encode_rebuild(&self, encoder: &mut wgpu::CommandEncoder) {
        self.dispatch(encoder, &self.clear_buckets_pipeline, self.bucket_count,
            "clear fluid buckets");
        self.dispatch(encoder, &self.insert_buckets_pipeline, self.particle_capacity,
            "insert fluid particles into buckets");
        self.dispatch(encoder, &self.raster_pipeline, self.buffered_cell_count,
            "rasterize derived fluid cells");
    }

    fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::ComputePipeline,
        count: u32,
        label: &str,
    ) {
        let mut pass: wgpu::ComputePass<'_> = encoder.begin_compute_pass(
            &wgpu::ComputePassDescriptor { label: Some(label), timestamp_writes: None },
        );
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
    }

    fn write_parameters(
        &self,
        accelerator: &Accelerator,
        buffered_origin: TileCoordinates,
        buffered_width: u16,
        buffered_height: u16,
        ring_offset_x: u16,
        ring_offset_y: u16,
        gravity: [f32; 2],
        delta_time: f32,
    ) {
        let values: [u32; 20] = [
            buffered_origin.x as u32, buffered_origin.y as u32,
            u32::from(buffered_width), u32::from(buffered_height),
            u32::from(ring_offset_x), u32::from(ring_offset_y),
            self.bucket_dimensions[0], self.bucket_dimensions[1],
            gravity[0].to_bits(), gravity[1].to_bits(), delta_time.to_bits(),
            self.particle_capacity, self.buffered_cell_count, self.bucket_count,
            SUPPORT_RADIUS_CELLS.to_bits(), PARTICLE_RADIUS_CELLS.to_bits(),
            MAXIMUM_MOVEMENT_CELLS, 0, 0, 0,
        ];
        let bytes: Vec<u8> = values.into_iter().flat_map(u32::to_le_bytes).collect();
        accelerator.wgpu_queue().write_buffer(&self.parameters, 0, &bytes);
    }

    fn binding<'a>(binding: u32, buffer: &'a AcceleratorBuffer) -> wgpu::BindGroupEntry<'a> {
        wgpu::BindGroupEntry {
            binding,
            resource: buffer.wgpu_buffer().as_entire_binding(),
        }
    }

    /// Returns the transient edit value used to remove fluid without spawning it
    pub const fn erase_edit() -> u32 { FLUID_EDIT_ERASE }

}

impl Drop for Fluids {

    fn drop(&mut self) {
        self.particles.free();
        self.free_indices.free();
        self.free_count.free();
        self.edit_cells.free();
        self.bucket_heads.free();
        self.next_particle.free();
        self.predicted_positions.free();
        self.lambdas.free();
        self.position_corrections.free();
        self.derived_material_identifiers.free();
        self.derived_coverage.free();
        self.derived_velocity.free();
        self.parameters.destroy();
    }

}

#[cfg(test)]
mod tests {

    use super::*;
    use std::sync::mpsc::sync_channel;

    #[test]
    fn fluid_edits_move_across_negative_tile_boundary_and_reuse_capacity() {
        let accelerator: Accelerator = Accelerator::new().unwrap();
        let canonical: AcceleratorBuffer = accelerator.allocate::<u32>(128);
        let body_occupancy: AcceleratorBuffer = accelerator.allocate::<u32>(128);
        let body_velocity: AcceleratorBuffer = accelerator.allocate::<[f32; 4]>(128);
        let fluid: Fluids = Fluids::new(
            &accelerator,
            &canonical,
            &body_occupancy,
            &body_velocity,
            2,
            1,
        );
        let origin: TileCoordinates = TileCoordinates { x: -1, y: 0 };
        let water: u32 = 3u32 << 30;
        let negative_cell_index: usize = 64 + 4 * 8 + 7;
        fluid.apply_edits(
            &accelerator,
            &[(negative_cell_index, water)],
            origin,
            2,
            1,
            1,
            0,
        );
        assert!(read_u32(&accelerator, fluid.material_identifiers_buffer(), 128)
            [negative_cell_index] == water);
        assert!(read_u32(&accelerator, &canonical, 128)
            [negative_cell_index] == 0);

        fluid.simulate(&accelerator, origin, 2, 1, 1, 0, [20.0, 0.0], 0.1);
        let particles: Vec<u32> = read_u32(&accelerator, &fluid.particles, 128 * 8);
        let active: &[u32] = particles.chunks_exact(8)
            .find(|particle| particle[0] == water).unwrap();
        assert!(f32::from_bits(active[2]) > 0.0);

        let crossed_cell_index: usize = 4 * 8;
        fluid.apply_edits(
            &accelerator,
            &[(crossed_cell_index, Fluids::erase_edit())],
            origin,
            2,
            1,
            1,
            0,
        );
        assert!(read_u32(&accelerator, &fluid.free_count, 1)[0] == 128);

        fluid.apply_edits(
            &accelerator,
            &[(negative_cell_index, water)],
            origin,
            2,
            1,
            1,
            0,
        );
        accelerator.wgpu_queue().write_buffer(
            body_occupancy.wgpu_buffer(), (4 * 8 * 4) as u64, &1u32.to_le_bytes(),
        );
        fluid.simulate(&accelerator, origin, 2, 1, 1, 0, [20.0, 0.0], 0.1);
        let particles: Vec<u32> = read_u32(&accelerator, &fluid.particles, 128 * 8);
        let active: &[u32] = particles.chunks_exact(8)
            .find(|particle| particle[0] == water).unwrap();
        assert!(f32::from_bits(active[2]) < 0.0);

        fluid.apply_edits(
            &accelerator,
            &[(negative_cell_index, Fluids::erase_edit())],
            origin,
            2,
            1,
            1,
            0,
        );
        accelerator.wgpu_queue().write_buffer(
            body_occupancy.wgpu_buffer(), (4 * 8 * 4) as u64, &0u32.to_le_bytes(),
        );
        accelerator.wgpu_queue().write_buffer(
            canonical.wgpu_buffer(), (4 * 8 * 4) as u64, &(1u32 << 30).to_le_bytes(),
        );
        fluid.apply_edits(
            &accelerator,
            &[(negative_cell_index, water)],
            origin,
            2,
            1,
            1,
            0,
        );
        fluid.simulate(&accelerator, origin, 2, 1, 1, 0, [20.0, 0.0], 0.1);
        let particles: Vec<u32> = read_u32(&accelerator, &fluid.particles, 128 * 8);
        let active: &[u32] = particles.chunks_exact(8)
            .find(|particle| particle[0] == water).unwrap();
        assert!(f32::from_bits(active[2]) < 0.0);

        fluid.refresh(&accelerator, TileCoordinates { x: 0, y: 0 }, 2, 1, 0, 0);
        assert!(read_u32(&accelerator, &fluid.free_count, 1)[0] == 128);
        assert!(read_u32(&accelerator, fluid.material_identifiers_buffer(), 128)
            .into_iter().all(|material| material == 0));
    }

    #[test]
    fn pbf_water_retains_volume_and_stays_finite() {
        let accelerator: Accelerator = Accelerator::new().unwrap();
        let width: u16 = 8;
        let height: u16 = 4;
        let count: usize = usize::from(width) * usize::from(height) * 64;
        let origin: TileCoordinates = TileCoordinates { x: -4, y: 0 };
        let canonical: AcceleratorBuffer = accelerator.allocate::<u32>(count);
        let body_occupancy: AcceleratorBuffer = accelerator.allocate::<u32>(count);
        let body_velocity: AcceleratorBuffer = accelerator.allocate::<[f32; 4]>(count);
        let fluid: Fluids = Fluids::new(
            &accelerator, &canonical, &body_occupancy, &body_velocity, width, height,
        );
        let physical_index = |x: i32, y: i32| -> usize {
            let tile_x: usize = (x.div_euclid(8) - origin.x) as usize;
            let tile_y: usize = (y.div_euclid(8) - origin.y) as usize;
            (tile_y * usize::from(width) + tile_x) * 64 +
                y.rem_euclid(8) as usize * 8 + x.rem_euclid(8) as usize
        };
        let stone: u32 = 1u32 << 30;
        for x in -8..8 {
            accelerator.wgpu_queue().write_buffer(
                canonical.wgpu_buffer(), physical_index(x, 0) as u64 * 4,
                &stone.to_le_bytes(),
            );
        }
        for y in 0..16 {
            for x in [-8, 7] {
                accelerator.wgpu_queue().write_buffer(
                    canonical.wgpu_buffer(), physical_index(x, y) as u64 * 4,
                    &stone.to_le_bytes(),
                );
            }
        }
        let water: u32 = 3u32 << 30;
        let edits: Vec<(usize, u32)> = (12..20).flat_map(|y| {
            (-4..4).map(move |x| (physical_index(x, y), water))
        }).collect();
        fluid.apply_edits(&accelerator, &edits, origin, width, height, 0, 0);
        for _ in 0..240 {
            fluid.simulate(
                &accelerator, origin, width, height, 0, 0, [0.0, -18.0], 1.0 / 60.0,
            );
        }
        let particle_data: Vec<u32> = read_u32(&accelerator, &fluid.particles, count * 8);
        let positions: Vec<[f32; 2]> = particle_data.chunks_exact(8)
            .filter(|particle| particle[0] == water)
            .map(|particle| [f32::from_bits(particle[2]), f32::from_bits(particle[3])])
            .collect();
        let maximum_speed: f32 = particle_data.chunks_exact(8)
            .filter(|particle| particle[0] == water)
            .map(|particle| f32::hypot(
                f32::from_bits(particle[4]), f32::from_bits(particle[5]),
            )).fold(0.0, f32::max);
        println!("settled maximum speed: {maximum_speed}");
        assert!(positions.len() == 64);
        assert!(positions.iter().flatten().all(|value| value.is_finite()));
        let minimum_x: f32 = positions.iter().map(|position| position[0])
            .fold(f32::INFINITY, f32::min);
        let maximum_x: f32 = positions.iter().map(|position| position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let minimum_y: f32 = positions.iter().map(|position| position[1])
            .fold(f32::INFINITY, f32::min);
        let maximum_y: f32 = positions.iter().map(|position| position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(maximum_x - minimum_x > 1.0);
        assert!(maximum_y - minimum_y > 0.25);
        assert!(minimum_y >= 0.12);

        fluid.apply_edits(&accelerator, &edits, origin, width, height, 0, 0);
        for _ in 0..240 {
            fluid.simulate(
                &accelerator, origin, width, height, 0, 0, [0.0, -18.0], 1.0 / 60.0,
            );
        }
        let particle_data: Vec<u32> = read_u32(&accelerator, &fluid.particles, count * 8);
        let positions: Vec<[f32; 2]> = particle_data.chunks_exact(8)
            .filter(|particle| particle[0] == water)
            .map(|particle| [f32::from_bits(particle[2]), f32::from_bits(particle[3])])
            .collect();
        assert!(positions.len() == 128);
        assert!(positions.iter().flatten().all(|value| value.is_finite()));
        assert!(positions.iter().map(|position| position[1])
            .fold(f32::NEG_INFINITY, f32::max) > maximum_y + 0.1);
    }

    #[test]
    fn temporary_pbf_stair_flow_check() {
        let accelerator: Accelerator = Accelerator::new().unwrap();
        let width: u16 = 8;
        let height: u16 = 4;
        let count: usize = usize::from(width) * usize::from(height) * 64;
        let origin: TileCoordinates = TileCoordinates { x: -4, y: 0 };
        let canonical: AcceleratorBuffer = accelerator.allocate::<u32>(count);
        let body_occupancy: AcceleratorBuffer = accelerator.allocate::<u32>(count);
        let body_velocity: AcceleratorBuffer = accelerator.allocate::<[f32; 4]>(count);
        let fluid: Fluids = Fluids::new(
            &accelerator, &canonical, &body_occupancy, &body_velocity, width, height,
        );
        let physical_index = |x: i32, y: i32| -> usize {
            let tile_x: usize = (x.div_euclid(8) - origin.x) as usize;
            let tile_y: usize = (y.div_euclid(8) - origin.y) as usize;
            (tile_y * usize::from(width) + tile_x) * 64 +
                y.rem_euclid(8) as usize * 8 + x.rem_euclid(8) as usize
        };
        for x in -24i32..24 {
            let surface: i32 = 8 - (x + 24).div_euclid(4).clamp(0, 7);
            for y in 0..=surface {
                accelerator.wgpu_queue().write_buffer(
                    canonical.wgpu_buffer(), physical_index(x, y) as u64 * 4,
                    &(1u32 << 30).to_le_bytes(),
                );
            }
        }
        let water: u32 = 3u32 << 30;
        let edits: Vec<(usize, u32)> = (11..17).flat_map(|y| {
            (-20..-14).map(move |x| (physical_index(x, y), water))
        }).collect();
        fluid.apply_edits(&accelerator, &edits, origin, width, height, 0, 0);
        for _ in 0..120 {
            fluid.simulate(
                &accelerator, origin, width, height, 0, 0, [0.0, -18.0], 1.0 / 60.0,
            );
        }
        let particles: Vec<u32> = read_u32(&accelerator, &fluid.particles, count * 8);
        let positions: Vec<f32> = particles.chunks_exact(8)
            .filter(|particle| particle[0] == water)
            .map(|particle| f32::from_bits(particle[2]))
            .collect();
        assert!(!positions.is_empty());
        let average_x: f32 = positions.iter().sum::<f32>() / positions.len() as f32;
        assert!(average_x > -1.5);
    }

    fn read_u32(
        accelerator: &Accelerator,
        source: &AcceleratorBuffer,
        count: usize,
    ) -> Vec<u32> {
        let staging: wgpu::Buffer = accelerator.wgpu_device().create_buffer(
            &wgpu::BufferDescriptor {
                label: Some("fluid test readback"),
                size: (count * 4) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );
        let mut encoder: wgpu::CommandEncoder = accelerator.wgpu_device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("fluid test readback") },
        );
        encoder.copy_buffer_to_buffer(
            source.wgpu_buffer(), 0, &staging, 0, (count * 4) as u64,
        );
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        let (sender, receiver) = sync_channel(1);
        staging.slice(..).map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
        accelerator.wgpu_device().poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receiver.recv().unwrap().unwrap();
        let mapped = staging.slice(..).get_mapped_range().unwrap();
        let values: Vec<u32> = mapped.chunks_exact(4).map(|bytes| {
            u32::from_le_bytes(bytes.try_into().unwrap())
        }).collect();
        drop(mapped);
        staging.unmap();
        values
    }

}
