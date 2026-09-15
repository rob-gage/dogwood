// Copyright Rob Gage 2026

use super::gpu_timing_record::GpuTimingRecord;
use std::sync::{Arc, atomic::AtomicU8};

/// One asynchronously mapped GPU timestamp readback slot.
pub(crate) struct GpuTimingReadback {
    pub(crate) buffer: wgpu::Buffer,
    pub(crate) status: Arc<AtomicU8>,
    pub(crate) sample: u64,
    pub(crate) query_count: u32,
    pub(crate) records: Vec<GpuTimingRecord>,
}

impl GpuTimingReadback {
    pub(crate) const IDLE: u8 = 0;
    pub(crate) const RECORDING: u8 = 1;
    pub(crate) const READY_TO_MAP: u8 = 2;
    pub(crate) const MAPPING: u8 = 3;
    pub(crate) const READY: u8 = 4;
    pub(crate) const FAILED: u8 = 5;

    pub(crate) fn new(device: &wgpu::Device, byte_capacity: u64, index: usize) -> Self {
        Self {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("GPU timing readback {index}")),
                size: byte_capacity,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            status: Arc::new(AtomicU8::new(Self::IDLE)),
            sample: 0,
            query_count: 0,
            records: Vec::new(),
        }
    }
}
