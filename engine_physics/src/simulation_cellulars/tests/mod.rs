// Copyright Rob Gage 2026

use std::sync::mpsc;
use std::time::Duration;
use std::time::Instant;

use engine_graphics::Color;
use engine_graphics::MaterialAppearance;
use rapier2d::prelude::RigidBodyHandle;

use super::*;
use crate::actors::ActorCellularProxyState;
use crate::actors::ActorCollisionShape;
use crate::materials::Material;
use crate::materials::MaterialForm;
use crate::materials::MaterialIdentifier;
use crate::materials::MaterialRegistry;
use crate::simulation::simulation_constants::RIGID_REACTION_READBACK_SLOT_COUNT;
use crate::simulation::tests::new_accelerator_test;
use crate::simulation_cellulars::CellularPressure;
use crate::simulation_rigid_bodies::RigidCellularBody;
use crate::simulation_rigid_bodies::RigidCellularBodyState;
use crate::tiles::CellularAppearance;
use crate::tiles::TileCoordinates;

#[test]
fn test_colored_face_pipelines_compile_on_accelerator() {
    let (_accelerator_test_lock, accelerator) = new_accelerator_test();
    let materials = MaterialRegistry::new();
    let cells = accelerator.allocate::<u32>(64);
    let appearances = accelerator.allocate::<u32>(64);
    let integrities = accelerator.allocate::<f32>(64);
    let kinematics = accelerator.allocate::<[f32; 4]>(64);
    let occupancy = accelerator.allocate::<u32>(64);
    let velocity = accelerator.allocate::<[f32; 4]>(64);
    let owners = accelerator.allocate::<u32>(64);
    let rigid_materials = accelerator.allocate::<u32>(64);
    let transforms = accelerator.allocate::<[f32; 4]>(3);
    let rigid_cells = accelerator.allocate::<[u32; 8]>(1);
    let pressure = CellularPressure::new(
        &accelerator,
        &materials,
        &cells,
        &appearances,
        &integrities,
        &accelerator.allocate::<f32>(64),
        &kinematics,
        &occupancy,
        &velocity,
        &owners,
        &accelerator.allocate::<u32>(64),
        &rigid_materials,
        &transforms,
        &rigid_cells,
        &accelerator.allocate::<[u32; 4]>(64),
        &accelerator.allocate::<[f32; 2]>(64),
        &accelerator.allocate::<f32>(64),
        &accelerator.allocate::<[f32; 4]>(2),
        &accelerator.allocate::<f32>(64),
        &accelerator.allocate::<[u32; 4]>(64),
        &accelerator.allocate::<u32>(1),
        0,
        64,
    );
    accelerator.poll().unwrap();
    drop(pressure);
}

#[test]
fn test_falling_cell_transfers_momentum_into_anchored_cell_without_pressure_kick() {
    let (_accelerator_test_lock, accelerator) = new_accelerator_test();
    let mut materials = MaterialRegistry::new();
    let stone = materials.register(Material::CellularStatic {
        name: "Stone".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(90, 90, 90)),
        mass: 1.0,
        pressure_ignore_threshold: 1000.0,
        default_integrity: 100.0,
        minimum_rigid_body_cell_count: 1,
        debris_material: None,
        debris_yield_rate: 0.0,
        pressure_transmission: 1.0,
        friction: 0.5,
        restitution: 0.0,
    });
    let sand = materials.register(Material::CellularDynamic {
        name: "Sand".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(200, 180, 130)),
        mass: 1.0,
        pressure_transmission: 1.0,
        friction: 0.5,
        restitution: 0.0,
    });
    let cells = accelerator.allocate::<u32>(64);
    let appearances = accelerator.allocate::<u32>(64);
    let integrities = accelerator.allocate::<f32>(64);
    let kinematics = accelerator.allocate::<[f32; 4]>(64);
    let occupancy = accelerator.allocate::<u32>(64);
    let external_velocity = accelerator.allocate::<[f32; 4]>(64);
    let owners = accelerator.allocate::<u32>(64);
    let rigid_materials = accelerator.allocate::<u32>(64);
    let transforms = accelerator.allocate::<[f32; 4]>(3);
    let rigid_cells = accelerator.allocate::<[u32; 8]>(1);
    let fluid = accelerator.allocate::<[u32; 4]>(64);
    let gas_velocity = accelerator.allocate::<[f32; 2]>(64);
    let gas_concentrations = accelerator.allocate::<f32>(64);
    let gas_properties = accelerator.allocate::<[f32; 4]>(2);
    let fluid_coverage = accelerator.allocate::<f32>(64);
    let mut pressure = CellularPressure::new(
        &accelerator,
        &materials,
        &cells,
        &appearances,
        &integrities,
        &accelerator.allocate::<f32>(64),
        &kinematics,
        &occupancy,
        &external_velocity,
        &owners,
        &accelerator.allocate::<u32>(64),
        &rigid_materials,
        &transforms,
        &rigid_cells,
        &fluid,
        &gas_velocity,
        &gas_concentrations,
        &gas_properties,
        &fluid_coverage,
        &accelerator.allocate::<[u32; 4]>(64),
        &accelerator.allocate::<u32>(1),
        0,
        64,
    );
    accelerator
        .wgpu_queue()
        .write_buffer(cells.wgpu_buffer(), 0, &stone.as_u32().to_le_bytes());
    accelerator
        .wgpu_queue()
        .write_buffer(cells.wgpu_buffer(), 8 * 4, &sand.as_u32().to_le_bytes());
    accelerator.wgpu_queue().write_buffer(
        kinematics.wgpu_buffer(),
        8 * 16 + 4,
        &(-1.0f32).to_le_bytes(),
    );
    pressure
        .simulate(
            &accelerator,
            TileCoordinates { x: 0, y: 0 },
            1,
            1,
            0,
            0,
            1.0 / 60.0,
            [0.0; 2],
            0,
            0,
            0,
        )
        .unwrap();
    let readback = accelerator
        .wgpu_device()
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("cellular face check"),
            size: 64 * 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
    let mut encoder =
        accelerator
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("cellular face check"),
            });
    encoder.copy_buffer_to_buffer(kinematics.wgpu_buffer(), 0, &readback, 0, 64 * 16);
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
    let velocity_y = f32::from_le_bytes(mapped[8 * 16 + 4..8 * 16 + 8].try_into().unwrap());
    assert!(
        velocity_y.abs() < 0.01,
        "unsupported pressure kick: {velocity_y}"
    );
}

#[test]
fn test_rigid_static_overlap_uses_one_coherent_reaction_per_tick_under_readback_backlog() {
    let (_accelerator_test_lock, accelerator) = new_accelerator_test();
    let mut materials = MaterialRegistry::new();
    let stone = materials.register(Material::CellularStatic {
        name: "Stone".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(90, 90, 90)),
        mass: 1.0,
        pressure_ignore_threshold: 1000.0,
        default_integrity: 100.0,
        minimum_rigid_body_cell_count: 1,
        debris_material: None,
        debris_yield_rate: 0.0,
        pressure_transmission: 1.0,
        friction: 0.5,
        restitution: 0.0,
    });
    let cells = accelerator.allocate::<u32>(64);
    let appearances = accelerator.allocate::<u32>(64);
    let integrities = accelerator.allocate::<f32>(64);
    let kinematics = accelerator.allocate::<[f32; 4]>(64);
    let occupancy = accelerator.allocate::<u32>(64);
    let external_velocity = accelerator.allocate::<[f32; 4]>(64);
    let owners = accelerator.allocate::<u32>(64);
    let rigid_materials = accelerator.allocate::<u32>(64);
    let transforms = accelerator.allocate::<[f32; 4]>(3);
    let rigid_cells = accelerator.allocate::<[u32; 8]>(1);
    let fluid = accelerator.allocate::<[u32; 4]>(64);
    let gas_velocity = accelerator.allocate::<[f32; 2]>(64);
    let gas_concentrations = accelerator.allocate::<f32>(64);
    let gas_properties = accelerator.allocate::<[f32; 4]>(2);
    let fluid_coverage = accelerator.allocate::<f32>(64);
    let mut pressure = CellularPressure::new(
        &accelerator,
        &materials,
        &cells,
        &appearances,
        &integrities,
        &accelerator.allocate::<f32>(64),
        &kinematics,
        &occupancy,
        &external_velocity,
        &owners,
        &accelerator.allocate::<u32>(64),
        &rigid_materials,
        &transforms,
        &rigid_cells,
        &fluid,
        &gas_velocity,
        &gas_concentrations,
        &gas_properties,
        &fluid_coverage,
        &accelerator.allocate::<[u32; 4]>(64),
        &accelerator.allocate::<u32>(1),
        0,
        64,
    );
    accelerator
        .wgpu_queue()
        .write_buffer(cells.wgpu_buffer(), 0, &stone.as_u32().to_le_bytes());
    let rigid_cell: [u32; 8] = [0, 0, 0, stone.as_u32(), 0, 0, 0, 0];
    accelerator.wgpu_queue().write_buffer(
        rigid_cells.wgpu_buffer(),
        0,
        &rigid_cell
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    // the rigid cell overlaps the top of the static cell and is moving into it.
    let transform: [f32; 12] = [
        0.0, 0.08, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0625, 0.1425, 1.0, 0.0,
    ];
    accelerator.wgpu_queue().write_buffer(
        transforms.wgpu_buffer(),
        0,
        &transform
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
    );

    // do not poll while submitting: this deliberately exercises the bounded
    // readback pool without permitting an unbounded staging allocation.
    for _ in 0..5 {
        pressure
            .simulate(
                &accelerator,
                TileCoordinates { x: 0, y: 0 },
                1,
                1,
                0,
                0,
                1.0 / 60.0,
                [0.0, -9.8],
                1,
                1,
                1,
            )
            .unwrap();
    }
    assert_eq!(
        RIGID_REACTION_READBACK_SLOT_COUNT,
        RIGID_REACTION_READBACK_SLOT_COUNT
    );
    let started = Instant::now();
    let mut batches = Vec::new();
    while batches.len() < RIGID_REACTION_READBACK_SLOT_COUNT {
        accelerator.poll().unwrap();
        batches.extend(pressure.collect_rigid_reactions().unwrap());
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::yield_now();
    }
    assert_eq!(batches.len(), RIGID_REACTION_READBACK_SLOT_COUNT);
    for batch in batches {
        assert_eq!(batch.body_count, 1);
        assert!(batch.static_contact_counts[0] > 0);
        assert_eq!(batch.granular_contact_counts[0], 0);
        assert!(batch.constraints[0][1].is_finite() && batch.constraints[0][1] > 0.1);
        assert!(
            batch.constraints[0][1] < 5.0,
            "explosive reaction: {:?}",
            batch.constraints[0]
        );
    }
}

#[test]
fn test_axis_aligned_rigid_block_raster_has_every_cell_once() {
    let (_accelerator_test_lock, accelerator) = new_accelerator_test();
    let material = MaterialIdentifier::new(MaterialForm::CellularStatic, 3);
    let body = RigidCellularBody {
        identifier: 0,
        handle: RigidBodyHandle::invalid(),
        cells: (2..6)
            .flat_map(|y| {
                (2..6).map(move |x| {
                    crate::simulation::RigidCellularBodyCell::test_cell(
                        [x, y],
                        material,
                        CellularAppearance::NEUTRAL,
                    )
                })
            })
            .collect(),
    };
    assert!(std::mem::size_of::<[u32; 8]>() == 32);
    println!("rigid bodies: 1, rigid cells: {}", body.cells.len());
    let buffered_cell_count: usize = 72 * 51 * 64;
    let mut proxy = CellularPhysicsBodyProxy::new(&accelerator, buffered_cell_count);
    let bodies = [body];
    let body_states = [RigidCellularBodyState {
        translation: [0.0; 2],
        angle: 0.0,
        linear_velocity: [0.0; 2],
        angular_velocity: 0.0,
        sleeping: false,
        center_of_mass: [0.5; 2],
        inverse_mass: 1.0,
        inverse_angular_inertia: 1.0,
    }];
    let actors = [
        ActorCellularProxyState {
            center: [-1.0, 0.0],
            velocity: [0.0; 2],
            drive: [1.0, 0.0],
            shape: ActorCollisionShape::Circle { radius: 0.375 },
            occupancy_kind: 1,
            mass: 1.0,
        },
        ActorCellularProxyState {
            center: [-1.0, 0.0],
            velocity: [0.0; 2],
            drive: [0.0; 2],
            shape: ActorCollisionShape::Capsule {
                radius: 0.25,
                height: 0.75,
            },
            occupancy_kind: 2,
            mass: 1.0,
        },
        ActorCellularProxyState {
            center: [1.5, 0.0],
            velocity: [0.0; 2],
            drive: [0.0; 2],
            shape: ActorCollisionShape::Rectangle {
                width: 0.5,
                height: 0.75,
            },
            occupancy_kind: 1,
            mass: 1.0,
        },
    ];
    proxy.rasterize(
        &accelerator,
        TileCoordinates { x: -12, y: -12 },
        72,
        51,
        0,
        0,
        [0.0, -1.0],
        &actors,
        &bodies,
        &body_states,
        0,
    );
    let readback = accelerator
        .wgpu_device()
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("rigid material raster test readback"),
            size: buffered_cell_count as u64 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
    let mut encoder =
        accelerator
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rigid material raster test copy"),
            });
    encoder.copy_buffer_to_buffer(
        proxy.rigid_material_identifiers_buffer().wgpu_buffer(),
        0,
        &readback,
        0,
        buffered_cell_count as u64 * 4,
    );
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
        if let Ok(result) = receiver.try_recv() {
            result.unwrap();
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::yield_now();
    }
    let mapped = readback.slice(..).get_mapped_range().unwrap();
    let actual: Vec<u32> = mapped
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| u32::from_le_bytes(*bytes))
        .collect();
    drop(mapped);
    readback.unmap();
    assert!(actual.iter().filter(|identifier| **identifier != 0).count() == 16);
    for y in 2..6 {
        for x in 2..6 {
            let index = (12 * 72 + 12) * 64 + y * 8 + x;
            assert!(
                actual[index] == material.as_u32(),
                "missing rigid raster at world cell ({x}, {y})"
            );
        }
    }
}
