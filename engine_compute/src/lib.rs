// Copyright Rob Gage 2026

mod accelerator;
mod accelerator_buffer;
mod gpu_timing;
#[cfg(debug_assertions)]
mod gpu_timing_readback;
#[cfg(debug_assertions)]
mod gpu_timing_record;
#[cfg(debug_assertions)]
mod gpu_timing_state;
mod shader_composition;

pub use accelerator::Accelerator;
pub use accelerator_buffer::AcceleratorBuffer;
pub use shader_composition::create_composed_shader_module;
