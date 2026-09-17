// Copyright Rob Gage 2026

use crate::simulation::tests::new_accelerator_test;

use super::*;
use crate::{
    materials::{MaterialForm, MaterialIdentifier},
    tiles::TileCoordinates,
};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

#[test]
fn test_mechanical_raster_and_scatter_pipelines_compile_on_accelerator() {
    let (_accelerator_test_lock, accelerator) = new_accelerator_test();
    let cells = accelerator.allocate::<u32>(64);
    let occupancy = accelerator.allocate::<u32>(64);
    let velocity = accelerator.allocate::<[f32; 4]>(64);
    let properties = accelerator.allocate::<[f32; 4]>(2);
    let thermal_properties = accelerator.allocate::<[u32; 16]>(1);
    let thermal_parameters = accelerator
        .wgpu_device()
        .create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    let fluids = Fluids::new(
        &accelerator,
        &cells,
        &occupancy,
        &velocity,
        &properties,
        &thermal_properties,
        &thermal_parameters,
        1,
        1,
    );
    fluids.consume_accelerator_edits(
        &accelerator,
        TileCoordinates { x: 0, y: 0 },
        1,
        1,
        TileCoordinates { x: 0, y: 0 },
        1,
        1,
        0,
        0,
    );
    accelerator.poll().unwrap();
    drop(fluids);
}

#[test]
fn test_solved_mechanical_delta_persists_in_authoritative_particle() {
    let (_accelerator_test_lock, accelerator) = new_accelerator_test();
    let cells = accelerator.allocate::<u32>(64);
    let occupancy = accelerator.allocate::<u32>(64);
    let velocity = accelerator.allocate::<[f32; 4]>(64);
    let properties = accelerator.allocate::<[f32; 4]>(2);
    let thermal_properties = accelerator.allocate::<[u32; 16]>(1);
    let thermal_parameters = accelerator
        .wgpu_device()
        .create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    accelerator.wgpu_queue().write_buffer(
        properties.wgpu_buffer(),
        0,
        &[1.0f32, 0.0, 0.0, 4.0, 0.0, 0.0, 1.0, 0.0]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    let fluids = Fluids::new(
        &accelerator,
        &cells,
        &occupancy,
        &velocity,
        &properties,
        &thermal_properties,
        &thermal_parameters,
        1,
        1,
    );
    let material = MaterialIdentifier::new(MaterialForm::Fluid, 0).as_u32();
    let particle: [u32; 8] = [material, 1, 0.5f32.to_bits(), 0.5f32.to_bits(), 0, 0, 0, 0];
    accelerator.wgpu_queue().write_buffer(
        fluids.particles_buffer().wgpu_buffer(),
        0,
        &particle
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    fluids.simulate(
        &accelerator,
        TileCoordinates { x: 0, y: 0 },
        1,
        1,
        TileCoordinates { x: 0, y: 0 },
        1,
        1,
        0,
        0,
        [0.0, 0.0],
        1.0 / 60.0,
    );
    let cell_index = 4 + 4 * 8;
    accelerator.wgpu_queue().write_buffer(
        fluids.mechanical_cells_buffer().wgpu_buffer(),
        cell_index * 16 + 8,
        &1.0f32.to_le_bytes(),
    );
    fluids.scatter_mechanical_response(&accelerator);
    let readback = accelerator
        .wgpu_device()
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("fluid particle response check"),
            size: 32,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
    let mut encoder =
        accelerator
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("fluid particle response check"),
            });
    encoder.copy_buffer_to_buffer(fluids.particles_buffer().wgpu_buffer(), 0, &readback, 0, 32);
    accelerator.wgpu_queue().submit(Some(encoder.finish()));
    let (sender, receiver) = mpsc::sync_channel(1);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    let started = Instant::now();
    loop {
        accelerator.poll().unwrap();
        if receiver.try_recv().is_ok() {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::yield_now();
    }
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    let particle_velocity = f32::from_le_bytes(mapped[16..20].try_into().unwrap());
    assert!(
        (particle_velocity - 1.0).abs() < 0.01,
        "mechanical response did not persist: {particle_velocity}"
    );
}
