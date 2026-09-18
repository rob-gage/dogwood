// Copyright Rob Gage 2026

use std::sync::mpsc;

use engine_compute::Accelerator;
use engine_compute::AcceleratorBuffer;

use crate::scenes::Scene;

pub(crate) fn read_cell_state(
    accelerator: &Accelerator,
    scene: &Scene,
    index: usize,
) -> (u32, f32) {
    let readback: wgpu::Buffer = accelerator
        .wgpu_device()
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("chemistry cell state readback"),
            size: 8,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
    let mut encoder: wgpu::CommandEncoder = accelerator
        .wgpu_device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_buffer_to_buffer(
        scene
            .test_cellular_material_identifiers_buffer()
            .wgpu_buffer(),
        index as u64 * 4,
        &readback,
        0,
        4,
    );
    encoder.copy_buffer_to_buffer(
        scene.test_cellular_amounts_buffer().wgpu_buffer(),
        index as u64 * 4,
        &readback,
        4,
        4,
    );
    accelerator.wgpu_queue().submit(Some(encoder.finish()));
    let (sender, receiver) = mpsc::sync_channel(1);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    loop {
        accelerator.poll().unwrap();
        if let Ok(result) = receiver.try_recv() {
            result.unwrap();
            break;
        }
        std::thread::yield_now();
    }
    let bytes: wgpu::BufferView = readback.slice(..).get_mapped_range().unwrap();
    (
        u32::from_le_bytes(bytes[..4].try_into().unwrap()),
        f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
    )
}

pub(crate) fn read_amount(accelerator: &Accelerator, buffer: &AcceleratorBuffer, slot: u32) -> f32 {
    let readback: wgpu::Buffer = accelerator
        .wgpu_device()
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("chemistry amount readback"),
            size: 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
    let mut encoder: wgpu::CommandEncoder = accelerator
        .wgpu_device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_buffer_to_buffer(buffer.wgpu_buffer(), u64::from(slot) * 4, &readback, 0, 4);
    accelerator.wgpu_queue().submit(Some(encoder.finish()));
    let (sender, receiver) = mpsc::sync_channel(1);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    loop {
        accelerator.poll().unwrap();
        if let Ok(result) = receiver.try_recv() {
            result.unwrap();
            break;
        }
        std::thread::yield_now();
    }
    let bytes: wgpu::BufferView = readback.slice(..).get_mapped_range().unwrap();
    f32::from_le_bytes(bytes[..4].try_into().unwrap())
}

pub(crate) fn read_fluid_state(
    accelerator: &Accelerator,
    scene: &Scene,
    slot: u32,
) -> (u32, u32, f32) {
    let readback: wgpu::Buffer = accelerator
        .wgpu_device()
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("chemistry fluid state readback"),
            size: 40,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
    let mut encoder: wgpu::CommandEncoder = accelerator
        .wgpu_device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_buffer_to_buffer(
        scene.test_fluid_particles_buffer().wgpu_buffer(),
        u64::from(slot) * 40,
        &readback,
        0,
        40,
    );
    accelerator.wgpu_queue().submit(Some(encoder.finish()));
    let (sender, receiver) = mpsc::sync_channel(1);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    loop {
        accelerator.poll().unwrap();
        if let Ok(result) = receiver.try_recv() {
            result.unwrap();
            break;
        }
        std::thread::yield_now();
    }
    let bytes: wgpu::BufferView = readback.slice(..).get_mapped_range().unwrap();
    (
        u32::from_le_bytes(bytes[..4].try_into().unwrap()),
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        f32::from_le_bytes(bytes[32..36].try_into().unwrap()),
    )
}
