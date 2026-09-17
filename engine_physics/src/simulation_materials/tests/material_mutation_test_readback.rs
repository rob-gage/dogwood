// Copyright Rob Gage 2026

use engine_compute::{Accelerator, AcceleratorBuffer};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

pub(crate) fn read_u32(
    accelerator: &Accelerator,
    source: &AcceleratorBuffer,
    count: u64,
) -> Vec<u32> {
    let buffer = accelerator
        .wgpu_device()
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("material mutation test readback"),
            size: count * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
    let mut encoder = accelerator
        .wgpu_device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_buffer_to_buffer(source.wgpu_buffer(), 0, &buffer, 0, count * 4);
    accelerator.wgpu_queue().submit(Some(encoder.finish()));
    let (sender, receiver) = mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    let start = Instant::now();
    loop {
        accelerator.poll().unwrap();
        if let Ok(result) = receiver.try_recv() {
            result.unwrap();
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::yield_now();
    }
    let result = buffer
        .slice(..)
        .get_mapped_range()
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| u32::from_le_bytes(*bytes))
        .collect();
    buffer.unmap();
    result
}
