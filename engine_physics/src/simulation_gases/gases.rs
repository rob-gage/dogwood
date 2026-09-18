// Copyright Rob Gage 2026

use engine_compute::AcceleratorBuffer;

/// Owns the shared Eulerian gas velocity and per-species concentration fields
pub struct Gases {
    /// Authoritative shared gas-mixture velocity in each physical cell
    velocity: AcceleratorBuffer,
    /// Velocity advection scratch field
    velocity_scratch: AcceleratorBuffer,
    /// Authoritative species-major concentrations
    concentrations: AcceleratorBuffer,
    /// Persistent shared gas-mixture temperature per physical cell.
    gas_temperature: AcceleratorBuffer,
    /// Species advection and diffusion scratch field
    concentration_scratch: AcceleratorBuffer,
    /// Projection divergence scratch field
    divergence: AcceleratorBuffer,
    /// First pressure Jacobi buffer
    pressure_a: AcceleratorBuffer,
    /// Second and final pressure Jacobi buffer
    pressure_b: AcceleratorBuffer,
    /// Scalar two-dimensional vorticity field
    curl: AcceleratorBuffer,
    /// Dense fixed-stride records used only during residency export
    streaming_data: AcceleratorBuffer,
    /// Buffered ring mapping, gravity, and solver constants
    gas_simulation_parameters: wgpu::Buffer,
    /// All concrete gas solver bindings
    bind_group: wgpu::BindGroup,
    advect_velocity_pipeline: wgpu::ComputePipeline,
    curl_pipeline: wgpu::ComputePipeline,
    force_pipeline: wgpu::ComputePipeline,
    divergence_pipeline: wgpu::ComputePipeline,
    pressure_clear_pipeline: wgpu::ComputePipeline,
    pressure_a_pipeline: wgpu::ComputePipeline,
    pressure_b_pipeline: wgpu::ComputePipeline,
    projection_pipeline: wgpu::ComputePipeline,
    concentration_pipeline: wgpu::ComputePipeline,
    clear_area_pipeline: wgpu::ComputePipeline,
    export_pipeline: wgpu::ComputePipeline,
    buffered_cell_count: u32,
    gas_count: u32,
    ambient_temperature: f32,
}

#[path = "gases_construction.rs"]
mod gases_construction;
#[path = "gases_operations.rs"]
mod gases_operations;
#[path = "gases_streaming.rs"]
mod gases_streaming;

impl Gases {
    /// Returns the authoritative species-major concentration allocation
    pub const fn concentrations_buffer(&self) -> &AcceleratorBuffer {
        &self.concentrations
    }

    #[cfg(test)]
    pub(crate) const fn test_buffered_cell_count(&self) -> u32 {
        self.buffered_cell_count
    }

    pub(crate) const fn temperature_buffer(&self) -> &AcceleratorBuffer {
        &self.gas_temperature
    }

    /// Returns the number of independently registered gas species
    pub const fn gas_count(&self) -> u32 {
        self.gas_count
    }
}

impl Drop for Gases {
    fn drop(&mut self) {
        self.velocity.free();
        self.velocity_scratch.free();
        self.concentrations.free();
        self.gas_temperature.free();
        self.concentration_scratch.free();
        self.divergence.free();
        self.pressure_a.free();
        self.pressure_b.free();
        self.curl.free();
        self.streaming_data.free();
        self.gas_simulation_parameters.destroy();
    }
}
