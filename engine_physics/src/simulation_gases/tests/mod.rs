// Copyright Rob Gage 2026

use std::sync::mpsc;
use std::time::Duration;
use std::time::Instant;

use engine_compute::Accelerator;
use engine_compute::AcceleratorBuffer;
use engine_graphics::Color;
use engine_graphics::MaterialAppearance;

use super::*;
use crate::chunks::ChunkGasCell;
use crate::materials::Material;
use crate::materials::MaterialForm;
use crate::materials::MaterialIdentifier;
use crate::materials::MaterialRegistry;
use crate::scenes::GasDownload;
use crate::simulation::tests::new_accelerator_test;
use crate::tiles::TileArea;
use crate::tiles::TileCoordinates;

fn read_species(accelerator: &Accelerator, gases: &Gases, species: u32) -> Vec<f32> {
    let count = gases.test_buffered_cell_count() as usize;
    let readback = accelerator
        .wgpu_device()
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("gas concentration test readback"),
            size: count as u64 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
    let mut encoder = accelerator
        .wgpu_device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_buffer_to_buffer(
        gases.concentrations_buffer().wgpu_buffer(),
        u64::from(species) * count as u64 * 4,
        &readback,
        0,
        count as u64 * 4,
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
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    let values = mapped
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect();
    drop(mapped);
    readback.unmap();
    values
}

#[test]
fn test_gas_obstacles_displace_without_destroying_inventory() {
    let (_accelerator_test_lock, accelerator) = new_accelerator_test();
    let mut materials = MaterialRegistry::new();
    let vapor = materials.register(Material::Gas {
        name: "Vapor".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(120, 160, 190)),
        density: 0.65,
        diffusivity: 0.0,
        extinction: 0.0,
        dissipation: 0.0,
        compressibility: 0.0,
    });
    let graphics = materials.build_material_graphics(&accelerator);
    let width = 3u16;
    let height = 3u16;
    let count = usize::from(width) * usize::from(height) * 64;
    let cellular = accelerator.allocate::<u32>(count);
    let body = accelerator.allocate::<u32>(count);
    let fluid = accelerator.allocate::<f32>(count);
    let gases = Gases::new(
        &accelerator,
        &materials,
        &cellular,
        &body,
        &fluid,
        &graphics.gas_properties,
        count,
        293.15,
    );
    let physical = |x: u32, y: u32| -> usize {
        ((y / 8 * u32::from(width) + x / 8) * 64 + (y % 8) * 8 + x % 8) as usize
    };
    let center = physical(8, 8);
    let all_cells: Vec<usize> = (0..count).collect();
    gases.apply_edits(&accelerator, &[], &all_cells);
    gases.apply_edits(&accelerator, &[(center, vapor.index(), 293.15)], &[]);
    let solid = MaterialIdentifier::new(MaterialForm::CellularStatic, 0).as_u32();
    accelerator.wgpu_queue().write_buffer(
        cellular.wgpu_buffer(),
        center as u64 * 4,
        &solid.to_le_bytes(),
    );
    gases.simulate(
        &accelerator,
        TileCoordinates { x: 0, y: 0 },
        width,
        height,
        0,
        0,
        [0.0, 0.0],
        1.0 / 60.0,
    );
    let displaced = read_species(&accelerator, &gases, 0);
    let total: f32 = displaced.iter().sum();
    assert!(displaced[center] < 0.01);
    assert!(displaced[physical(7, 8)] > 0.1 || displaced[physical(9, 8)] > 0.1);
    assert!((total - 1.0).abs() < 0.02);

    accelerator.wgpu_queue().write_buffer(
        cellular.wgpu_buffer(),
        center as u64 * 4,
        &0u32.to_le_bytes(),
    );
    accelerator.wgpu_queue().write_buffer(
        fluid.wgpu_buffer(),
        center as u64 * 4,
        &1.0f32.to_le_bytes(),
    );
    gases.apply_edits(&accelerator, &[], &all_cells);
    gases.apply_edits(&accelerator, &[(center, vapor.index(), 293.15)], &[]);
    gases.simulate(
        &accelerator,
        TileCoordinates { x: 0, y: 0 },
        width,
        height,
        0,
        0,
        [0.0, 0.0],
        1.0 / 60.0,
    );
    let fluid_displaced = read_species(&accelerator, &gases, 0);
    assert!((fluid_displaced.iter().sum::<f32>() - total).abs() < 0.02);

    gases.apply_edits(&accelerator, &[(center, vapor.index(), 293.15)], &all_cells);
    for step in 0..8u32 {
        let obstacle = physical(8 + step, 8);
        accelerator.wgpu_queue().write_buffer(
            fluid.wgpu_buffer(),
            obstacle as u64 * 4,
            &1.0f32.to_le_bytes(),
        );
        gases.simulate(
            &accelerator,
            TileCoordinates { x: 0, y: 0 },
            width,
            height,
            0,
            0,
            [0.0, 0.0],
            1.0 / 60.0,
        );
        accelerator.wgpu_queue().write_buffer(
            fluid.wgpu_buffer(),
            obstacle as u64 * 4,
            &0.0f32.to_le_bytes(),
        );
    }
    let moving = read_species(&accelerator, &gases, 0);
    assert!((moving.iter().sum::<f32>() - 1.0).abs() < 0.02);

    let neighbors = [
        physical(7, 8),
        physical(9, 8),
        physical(8, 7),
        physical(8, 9),
    ];
    gases.apply_edits(&accelerator, &[(center, vapor.index(), 293.15)], &all_cells);
    for index in [center].into_iter().chain(neighbors) {
        accelerator.wgpu_queue().write_buffer(
            cellular.wgpu_buffer(),
            index as u64 * 4,
            &solid.to_le_bytes(),
        );
    }
    gases.simulate(
        &accelerator,
        TileCoordinates { x: 0, y: 0 },
        width,
        height,
        0,
        0,
        [0.0, 0.0],
        1.0 / 60.0,
    );
    let trapped = read_species(&accelerator, &gases, 0);
    assert!(trapped[center] > 0.95);
}

#[test]
fn test_coexisting_species_remain_spread_inside_a_circular_enclosure_for_one_minute() {
    let (_accelerator_test_lock, accelerator) = new_accelerator_test();
    let mut materials: MaterialRegistry = MaterialRegistry::new();
    let vapor: MaterialIdentifier = materials.register(Material::Gas {
        name: "Vapor".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(120, 160, 190)),
        density: 0.65,
        diffusivity: 0.8,
        extinction: 0.08,
        dissipation: 0.0,
        compressibility: 0.05,
    });
    let tracer: MaterialIdentifier = materials.register(Material::Gas {
        name: "Tracer".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(190, 120, 140)),
        density: 0.65,
        diffusivity: 0.8,
        extinction: 0.2,
        dissipation: 0.0001,
        compressibility: 0.1,
    });
    let graphics = materials.build_material_graphics(&accelerator);
    let width: u16 = 4;
    let height: u16 = 4;
    let cell_count: usize = usize::from(width) * usize::from(height) * 64;
    let cellular: AcceleratorBuffer = accelerator.allocate::<u32>(cell_count);
    let body: AcceleratorBuffer = accelerator.allocate::<u32>(cell_count);
    let fluid: AcceleratorBuffer = accelerator.allocate::<f32>(cell_count);
    let gases: Gases = Gases::new(
        &accelerator,
        &materials,
        &cellular,
        &body,
        &fluid,
        &graphics.gas_properties,
        cell_count,
        293.15,
    );
    let physical_index = |x: u32, y: u32| -> usize {
        let tile_x: u32 = x / 8;
        let tile_y: u32 = y / 8;
        ((tile_y * u32::from(width) + tile_x) * 64 + (y % 8) * 8 + x % 8) as usize
    };
    let mut edits: Vec<(usize, u32, f32)> = Vec::new();
    for y in 5..11 {
        for x in 12..20 {
            edits.push((physical_index(x, y), vapor.index(), 400.0));
            edits.push((physical_index(x, y), tracer.index(), 300.0));
        }
    }
    gases.apply_edits(&accelerator, &edits, &[]);
    let solid: u32 = MaterialIdentifier::new(MaterialForm::CellularStatic, 0).as_u32();
    for y in 0..32 {
        for x in 0..32 {
            let offset_x: i32 = x as i32 - 16;
            let offset_y: i32 = y as i32 - 16;
            if offset_x * offset_x + offset_y * offset_y < 169 {
                continue;
            }
            accelerator.wgpu_queue().write_buffer(
                cellular.wgpu_buffer(),
                physical_index(x, y) as u64 * 4,
                &solid.to_le_bytes(),
            );
        }
    }
    for _ in 0..3600 {
        gases.simulate(
            &accelerator,
            TileCoordinates { x: 0, y: 0 },
            width,
            height,
            0,
            0,
            [0.0, -18.0],
            1.0 / 60.0,
        );
    }
    let area: TileArea = TileArea::new(TileCoordinates { x: 0, y: 0 }, width, height);
    let download: GasDownload =
        GasDownload::new(&accelerator, area, cell_count as u32, gases.gas_count());
    gases.export(
        &accelerator,
        &download,
        TileCoordinates { x: 0, y: 0 },
        width,
        height,
        0,
        0,
    );
    let byte_count: u64 = cell_count as u64 * u64::from(5 + gases.gas_count()) * 4;
    let (sender, receiver) = mpsc::sync_channel(1);
    download
        .buffer
        .slice(0..byte_count)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    let started: Instant = Instant::now();
    loop {
        accelerator.poll().unwrap();
        if let Ok(result) = receiver.try_recv() {
            result.unwrap();
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::yield_now();
    }
    let mapped = download
        .buffer
        .slice(0..byte_count)
        .get_mapped_range()
        .unwrap();
    let bytes: Vec<u8> = mapped.to_vec();
    drop(mapped);
    download.buffer.unmap();
    let cells: Vec<ChunkGasCell> =
        GasDownload::deserialize(&bytes, area, &[vapor, tracer]).unwrap();
    assert!(!cells.is_empty());
    let total: f32 = cells
        .iter()
        .flat_map(|cell| cell.species.iter())
        .filter(|(identifier, _)| *identifier == vapor)
        .map(|(_, value)| value)
        .sum();
    let tracer_total: f32 = cells
        .iter()
        .flat_map(|cell| cell.species.iter())
        .filter(|(identifier, _)| *identifier == tracer)
        .map(|(_, value)| value)
        .sum();
    let maximum: f32 = cells
        .iter()
        .flat_map(|cell| cell.species.iter())
        .filter(|(identifier, _)| *identifier == vapor)
        .map(|(_, value)| *value)
        .fold(0.0, f32::max);
    let center_y: f32 = cells
        .iter()
        .map(|cell| {
            let concentration: f32 = cell
                .species
                .iter()
                .filter(|(identifier, _)| *identifier == vapor)
                .map(|(_, value)| value)
                .sum();
            (cell.coordinates.y as f32 + 0.5) * concentration
        })
        .sum::<f32>()
        / total;
    let occupied_y: Vec<i32> = cells
        .iter()
        .filter_map(|cell| {
            cell.species
                .iter()
                .find(|(identifier, concentration)| *identifier == vapor && *concentration > 0.01)
                .map(|_| cell.coordinates.y)
        })
        .collect();
    assert!(total > 47.0);
    assert!(maximum < 1.0);
    assert!(center_y > 8.5);
    assert!(cells.iter().all(|cell| {
        let offset_x: i32 = cell.coordinates.x - 16;
        let offset_y: i32 = cell.coordinates.y - 16;
        offset_x * offset_x + offset_y * offset_y < 169
    }));
    assert!(occupied_y.iter().max().unwrap() - occupied_y.iter().min().unwrap() >= 6);
    assert!(
        cells
            .iter()
            .flat_map(|cell| &cell.species)
            .any(|(_, concentration)| (0.01..0.99).contains(concentration))
    );
    assert!(cells.iter().any(|cell| cell.species.len() == 2));
    assert!(cells.iter().any(|cell| cell.velocity[0] < -0.01));
    assert!(cells.iter().any(|cell| cell.velocity[0] > 0.01));
    assert!(tracer_total > total * 0.98);
    assert!(tracer_total < total);
    cellular.free();
    body.free();
    fluid.free();
}

#[test]
#[ignore = "full-size Accelerator performance smoke"]
fn test_full_demo_sized_gas_field_runs_sixty_ticks() {
    let (_accelerator_test_lock, accelerator) = new_accelerator_test();
    eprintln!(
        "gas performance adapter: {:?}",
        accelerator.wgpu_adapter().get_info()
    );
    let mut materials: MaterialRegistry = MaterialRegistry::new();
    materials.register(Material::Gas {
        name: "Vapor".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(120, 160, 190)),
        density: 0.65,
        diffusivity: 0.12,
        extinction: 0.08,
        dissipation: 0.0,
        compressibility: 0.05,
    });
    materials.register(Material::Gas {
        name: "Smoke".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(68, 72, 76)),
        density: 0.85,
        diffusivity: 0.04,
        extinction: 2.5,
        dissipation: 0.0001,
        compressibility: 0.1,
    });
    let graphics = materials.build_material_graphics(&accelerator);
    let width: u16 = 72;
    let height: u16 = 51;
    let cell_count: usize = usize::from(width) * usize::from(height) * 64;
    let cellular: AcceleratorBuffer = accelerator.allocate::<u32>(cell_count);
    let body: AcceleratorBuffer = accelerator.allocate::<u32>(cell_count);
    let fluid: AcceleratorBuffer = accelerator.allocate::<f32>(cell_count);
    let gases: Gases = Gases::new(
        &accelerator,
        &materials,
        &cellular,
        &body,
        &fluid,
        &graphics.gas_properties,
        cell_count,
        293.15,
    );
    let started: Instant = Instant::now();
    for _ in 0..60 {
        gases.simulate(
            &accelerator,
            TileCoordinates { x: -12, y: -12 },
            width,
            height,
            0,
            0,
            [0.0, -18.0],
            1.0 / 60.0,
        );
    }
    accelerator
        .wgpu_device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let elapsed: Duration = started.elapsed();
    eprintln!("full demo-sized gas 60-tick Accelerator smoke: {elapsed:?}");
    assert!(elapsed < Duration::from_secs(30));
    cellular.free();
    body.free();
    fluid.free();
}
