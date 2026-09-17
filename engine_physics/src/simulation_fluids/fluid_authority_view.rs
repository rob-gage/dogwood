// Copyright Rob Gage 2026

use engine_compute::AcceleratorBuffer;

/// Internal read view of the authoritative fluid spatial index
pub(crate) struct FluidAuthorityView<'a> {
    pub(crate) particles: &'a AcceleratorBuffer,
    pub(crate) particle_capacity: u32,
    pub(crate) free_indices: &'a AcceleratorBuffer,
    pub(crate) free_count: &'a AcceleratorBuffer,
    pub(crate) bucket_heads: &'a AcceleratorBuffer,
    pub(crate) next_particle: &'a AcceleratorBuffer,
    pub(crate) parameters: &'a wgpu::Buffer,
}
