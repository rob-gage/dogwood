// Copyright Rob Gage 2026

use engine_compute::Accelerator;

use super::*;
use crate::{
    actors::{ActorCellularProxyState, ActorCollisionShape},
    materials::{MaterialForm, MaterialIdentifier},
    simulation_rigid_bodies::{RigidCellularBody, RigidCellularBodyState},
    tiles::{CellularAppearance, TileCoordinates},
};
use rapier2d::prelude::RigidBodyHandle;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

#[test]
fn test_axis_aligned_rigid_block_raster_has_every_cell_once() {
    let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
    let accelerator = Accelerator::new().unwrap();
    let material = MaterialIdentifier::new(MaterialForm::CellularStatic, 3);
    let body = RigidCellularBody {
        id: 0,
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
