// Copyright Rob Gage 2026

//! Shared Accelerator resources and composable shader construction.

mod accelerator;
mod accelerator_buffer;
mod accelerator_timing;
#[cfg(debug_assertions)]
mod accelerator_timing_readback;
#[cfg(debug_assertions)]
mod accelerator_timing_record;
#[cfg(debug_assertions)]
mod accelerator_timing_state;
mod shader_composition;

pub use accelerator::Accelerator;
pub use accelerator_buffer::AcceleratorBuffer;
pub use shader_composition::{
    ComposableShaderUtility, create_composed_shader_module,
    create_composed_shader_module_with_utilities,
};
