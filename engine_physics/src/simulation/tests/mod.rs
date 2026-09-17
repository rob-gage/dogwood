// Copyright Rob Gage 2026

mod accelerator_test_lock;

pub(crate) use accelerator_test_lock::new_accelerator_test;

use crate::simulation::test_rigid_phase_readback_len;

fn transitioned_temperature(
    amount: f32,
    source_cp: f32,
    target_cp: f32,
    threshold: f32,
    latent: f32,
    temperature: f32,
    hot: bool,
) -> Option<f32> {
    let sensible = amount
        * source_cp
        * if hot {
            (temperature - threshold).max(0.0)
        } else {
            (threshold - temperature).max(0.0)
        };
    let required = amount * latent.max(0.0);
    if amount <= 0.0 || target_cp <= 0.0 || (latent > 0.0 && sensible < required) {
        return None;
    }
    Some(
        (threshold
            + if hot { 1.0 } else { -1.0 } * (sensible - required).max(0.0) / (amount * target_cp))
            .max(0.0),
    )
}

#[test]
fn test_rigid_readback_size_scales_with_submitted_rigid_count() {
    assert_eq!(test_rigid_phase_readback_len(0), 256);
    assert_eq!(test_rigid_phase_readback_len(128), 256 + 128 * 40);
    assert!(test_rigid_phase_readback_len(128) < test_rigid_phase_readback_len(100_000));
}

#[test]
fn test_latent_arithmetic_is_symmetric() {
    assert_eq!(
        transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 11.0, true),
        None
    );
    assert_eq!(
        transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 11.5, true),
        Some(10.0)
    );
    assert_eq!(
        transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 13.5, true),
        Some(11.0)
    );
    assert_eq!(
        transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 9.0, false),
        None
    );
    assert_eq!(
        transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 8.5, false),
        Some(10.0)
    );
    assert_eq!(
        transitioned_temperature(1.0, 2.0, 4.0, 10.0, 3.0, 6.5, false),
        Some(9.0)
    );
    assert_eq!(
        transitioned_temperature(1.0, 2.0, 4.0, 10.0, 0.0, 11.0, true),
        Some(10.5)
    );
}

use crate::simulation::{
    RigidCellularBody, RigidCellularBodyCell, ThermalConduction, ThermalEdits,
};
use crate::tiles::CellularAppearance;
use std::{
    collections::BTreeMap,
    sync::mpsc::sync_channel,
    time::{Duration, Instant},
};

#[test]
fn test_hot_rigid_field_cell_conducts_into_cold_canonical_neighbor() {
    let (_accelerator_test_lock, accelerator) = new_accelerator_test();
    let interaction = accelerator.allocate::<[f32; 4]>(64);
    let mut values: Vec<[f32; 4]> = vec![[0.0; 4]; 64];
    values[0] = [1.0, 100.0, 1.0, 100.0];
    values[1] = [1.0, 0.0, 1.0, 0.0];
    accelerator.wgpu_queue().write_buffer(
        interaction.wgpu_buffer(),
        0,
        &values
            .iter()
            .flat_map(|value| value.iter().flat_map(|x| x.to_le_bytes()))
            .collect::<Vec<_>>(),
    );
    let conduction = ThermalConduction::new(&accelerator, &interaction, 64);
    conduction.conduct(&accelerator, [0, 0], [1, 1], [0, 0], 1.0);
    let readback = accelerator
        .wgpu_device()
        .create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 32,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
    let mut encoder = accelerator
        .wgpu_device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_buffer_to_buffer(
        conduction.solved_buffer().wgpu_buffer(),
        0,
        &readback,
        0,
        32,
    );
    accelerator.wgpu_queue().submit(Some(encoder.finish()));
    let (sender, receiver) = sync_channel(1);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    let start = Instant::now();
    while receiver.try_recv().is_err() {
        accelerator.poll().unwrap();
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::yield_now();
    }
    let bytes = readback.slice(..).get_mapped_range().unwrap();
    let hot = f32::from_bits(u32::from_le_bytes(bytes[12..16].try_into().unwrap()));
    let cold = f32::from_bits(u32::from_le_bytes(bytes[28..32].try_into().unwrap()));
    assert!(hot < 100.0 && cold > 0.0);
}

#[test]
fn test_dispatch_applies_cellular_delta() {
    let (_accelerator_test_lock, accelerator) = new_accelerator_test();
    let alloc = |count: usize| accelerator.allocate::<u32>(count);
    let materials = alloc(1);
    let temperatures = accelerator.allocate::<f32>(1);
    let gas_temperatures = accelerator.allocate::<f32>(1);
    let particles = accelerator.allocate::<[u32; 10]>(1);
    let claims = alloc(1);
    let rigid_cells = accelerator.allocate::<[u32; 8]>(1);
    let rigid_temperatures = accelerator.allocate::<f32>(1);
    accelerator
        .wgpu_queue()
        .write_buffer(materials.wgpu_buffer(), 0, &1u32.to_le_bytes());
    accelerator.wgpu_queue().write_buffer(
        temperatures.wgpu_buffer(),
        0,
        &300.0f32.to_bits().to_le_bytes(),
    );
    accelerator
        .wgpu_queue()
        .write_buffer(claims.wgpu_buffer(), 0, &0xffffffffu32.to_le_bytes());
    let edits = ThermalEdits::new(
        &accelerator,
        &materials,
        &temperatures,
        &gas_temperatures,
        &particles,
        &claims,
        &rigid_cells,
        &rigid_temperatures,
        1,
        1,
        1,
    );
    edits.apply(
        &accelerator,
        &[0],
        &[(0usize, 10.0f32)].into_iter().collect::<BTreeMap<_, _>>(),
        [0, 0],
        [1, 1],
        [0, 0],
    );
    accelerator.poll().unwrap();
    let readback = accelerator
        .wgpu_device()
        .create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
    let mut encoder = accelerator
        .wgpu_device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_buffer_to_buffer(temperatures.wgpu_buffer(), 0, &readback, 0, 4);
    accelerator.wgpu_queue().submit(Some(encoder.finish()));
    let (sender, receiver) = sync_channel(1);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    while receiver.try_recv().is_err() {
        accelerator.poll().unwrap();
        std::thread::yield_now();
    }
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    let value = f32::from_bits(u32::from_le_bytes(mapped[..4].try_into().unwrap()));
    assert_eq!(value, 310.0);
}

#[test]
fn test_dispatch_applies_one_delta_to_a_multi_claim_rigid_state() {
    let (_accelerator_test_lock, accelerator) = new_accelerator_test();
    let materials = accelerator.allocate::<u32>(2);
    let temperatures = accelerator.allocate::<f32>(2);
    let gas_temperatures = accelerator.allocate::<f32>(2);
    let particles = accelerator.allocate::<[u32; 10]>(1);
    let claims = accelerator.allocate::<u32>(2);
    let rigid_cells = accelerator.allocate::<[u32; 8]>(2);
    let rigid_temperatures = accelerator.allocate::<f32>(1);
    accelerator.wgpu_queue().write_buffer(
        claims.wgpu_buffer(),
        0,
        &0u32
            .to_le_bytes()
            .into_iter()
            .chain(1u32.to_le_bytes())
            .collect::<Vec<_>>(),
    );
    let cells = [[0, 0, 0, 1, 0, 0, 0, 0], [0, 0, 0, 1, 0, 0, 0, 0]];
    accelerator.wgpu_queue().write_buffer(
        rigid_cells.wgpu_buffer(),
        0,
        &cells
            .into_iter()
            .flatten()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    accelerator.wgpu_queue().write_buffer(
        rigid_temperatures.wgpu_buffer(),
        0,
        &263.15f32.to_le_bytes(),
    );
    let edits = ThermalEdits::new(
        &accelerator,
        &materials,
        &temperatures,
        &gas_temperatures,
        &particles,
        &claims,
        &rigid_cells,
        &rigid_temperatures,
        2,
        1,
        1,
    );
    edits.apply(
        &accelerator,
        &[0, 1],
        &[(0usize, 20.0f32), (1usize, 20.0f32)]
            .into_iter()
            .collect::<BTreeMap<_, _>>(),
        [0, 0],
        [2, 1],
        [0, 0],
    );
    accelerator.poll().unwrap();
    let readback = accelerator
        .wgpu_device()
        .create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
    let mut encoder = accelerator
        .wgpu_device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_buffer_to_buffer(rigid_temperatures.wgpu_buffer(), 0, &readback, 0, 4);
    accelerator.wgpu_queue().submit(Some(encoder.finish()));
    let (sender, receiver) = sync_channel(1);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    while receiver.try_recv().is_err() {
        accelerator.poll().unwrap();
        std::thread::yield_now();
    }
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    let value = f32::from_bits(u32::from_le_bytes(mapped[..4].try_into().unwrap()));
    assert!((value - 283.15).abs() < 0.001, "{value}");
}

#[test]
fn test_removed_bridge_splits_body_local_cells() {
    let material = crate::materials::MaterialIdentifier::new(
        crate::materials::MaterialForm::CellularStatic,
        0,
    );
    let cells = [[0, 0], [1, 0], [2, 0]]
        .into_iter()
        .map(|local| RigidCellularBodyCell::test_cell(local, material, CellularAppearance::NEUTRAL))
        .filter(|cell| cell.local != [1, 0])
        .collect();
    let components = RigidCellularBody::connected_components(cells);
    assert!(components.len() == 2);
    assert!(components.iter().all(|component| component.len() == 1));
}
