//! Pipeline creation telemetry for loading screens, diagnostics and warmup regression checks.
use std::{sync::atomic::{AtomicU64, Ordering}, time::Instant};
static CREATED: AtomicU64 = AtomicU64::new(0);

/// Number of engine compute/render pipeline creations begun in this process.
pub fn pipeline_creation_count() -> u64 { CREATED.load(Ordering::Relaxed) }

/// Create a compute pipeline and record its label, entry point and compilation duration.
pub fn create_compute_pipeline(device: &wgpu::Device, descriptor: &wgpu::ComputePipelineDescriptor<'_>) -> wgpu::ComputePipeline {
    let id = CREATED.fetch_add(1, Ordering::Relaxed) + 1;
    let began = Instant::now();
    eprintln!("GPU pipeline {id} begin: {} / {}", descriptor.label.unwrap_or("compute"), descriptor.entry_point.unwrap_or("default"));
    let result = device.create_compute_pipeline(descriptor);
    eprintln!("GPU pipeline {id} ready in {} ms", began.elapsed().as_millis());
    result
}

/// Create a render pipeline and record its label and compilation duration.
pub fn create_render_pipeline(device: &wgpu::Device, descriptor: &wgpu::RenderPipelineDescriptor<'_>) -> wgpu::RenderPipeline {
    let id = CREATED.fetch_add(1, Ordering::Relaxed) + 1;
    let began = Instant::now();
    eprintln!("GPU pipeline {id} begin: {}", descriptor.label.unwrap_or("render"));
    let result = device.create_render_pipeline(descriptor);
    eprintln!("GPU pipeline {id} ready in {} ms", began.elapsed().as_millis());
    result
}
