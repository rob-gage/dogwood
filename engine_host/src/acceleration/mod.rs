// Copyright Rob Gage 2026

mod accelerator;
mod accelerator_error;
mod accelerator_readback;
mod accelerator_readback_error;

pub use accelerator::Accelerator;
pub use accelerator_error::AcceleratorError;
pub use accelerator_readback::AcceleratorReadback;
pub use accelerator_readback_error::AcceleratorReadbackError;

mod performance;

mod pipelines;
pub use pipelines::{create_compute_pipeline, create_render_pipeline, create_shader_module, create_bind_group_layout, create_pipeline_layout, pipeline_creation_count};
