// Copyright Rob Gage 2026

use super::FluidAuthorityView;
use crate::scenes::{FluidDownload, FluidUpload};
use crate::simulation::simulation_constants::*;
use crate::{
    actors::ActorCollisionShape,
    chunks::ChunkFluidParticle,
    tiles::{TileArea, TileCoordinates},
};
use engine_compute::{Accelerator, AcceleratorBuffer};
use std::io;

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
    /// Set by Accelerator producers when `edit_cells` contains one or more edits.
    accelerator_edits_pending: AcceleratorBuffer,
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
    accelerator_edit_prepare_bind_group: wgpu::BindGroup,
    /// Removes authoritative particles from edited cells
    edit_remove_pipeline: wgpu::ComputePipeline,
    /// Claims free slots for requested fluid cells
    edit_spawn_pipeline: wgpu::ComputePipeline,
    /// Clears transient edit values after they are consumed
    edit_clear_pipeline: wgpu::ComputePipeline,
    prepare_accelerator_edits_pipeline: wgpu::ComputePipeline,
    accelerator_edit_dispatch: wgpu::Buffer,
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

#[path = "fluids_construction.rs"]
mod fluids_construction;
#[path = "fluids_operations.rs"]
mod fluids_operations;
#[path = "fluids_streaming.rs"]
mod fluids_streaming;

impl Fluids {
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
            particle_capacity: self.particle_capacity,
            free_indices: &self.free_indices,
            free_count: &self.free_count,
            bucket_heads: &self.bucket_heads,
            next_particle: &self.next_particle,
            parameters: &self.parameters,
        }
    }

    /// Returns the maximum number of authoritative resident particles
    pub const fn particle_capacity(&self) -> u32 {
        self.particle_capacity
    }

    pub(crate) const fn derived_thermal_buffer(&self) -> &AcceleratorBuffer {
        &self.derived_thermal
    }

    /// Returns the tile buffer required by active movement and particle-fluid support
    pub fn minimum_buffer_tiles() -> u8 {
        let predicted_movement: f32 = MAXIMUM_MOVEMENT_CELLS as f32 / PBF_SUBSTEP_COUNT as f32
            + MAXIMUM_CORRECTION_CELLS * PBF_CONSTRAINT_ITERATION_COUNT as f32;
        ((SUPPORT_RADIUS_CELLS * 2.0 + predicted_movement) / 8.0).ceil() as u8
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
        self.accelerator_edits_pending.free();
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
