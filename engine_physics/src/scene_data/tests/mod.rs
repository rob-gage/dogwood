// Copyright Rob Gage 2026

use super::*;
use crate::materials::{Material, MaterialIdentifier, MaterialRegistry};
use crate::tiles::CellularAppearance;
use engine_graphics::{Color, MaterialAppearance};
use std::{collections::HashSet, io};

fn test_materials() -> (MaterialRegistry, MaterialIdentifier) {
    let mut registry = MaterialRegistry::new();
    let identifier = registry.register(Material::CellularStatic {
        name: "test".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(1, 2, 3)),
        mass: 1.0,
        pressure_ignore_threshold: 0.0,
        default_integrity: 1.0,
        minimum_rigid_body_cell_count: 1,
        debris_material: None,
        debris_yield_rate: 0.0,
        pressure_transmission: 0.0,
        friction: 0.0,
        restitution: 0.0,
    });
    (registry, identifier)
}

#[test]
fn test_dormant_rigid_serialization_preserves_authoritative_state() {
    let (materials, material) = test_materials();
    let body = SceneDormantRigidBody {
        identifier: 9,
        position: [1.25, -2.5],
        rotation: 0.75,
        linear_velocity: [3.0, -4.0],
        angular_velocity: 5.0,
        sleeping: true,
        cells: vec![
            SceneDormantRigidCell {
                local: [-2, 3],
                material,
                appearance: CellularAppearance(0x1234_5678),
                integrity: 0.25,
                amount: 0.5,
                temperature: 456.0,
            },
            SceneDormantRigidCell {
                local: [4, 5],
                material,
                appearance: CellularAppearance(7),
                integrity: 0.75,
                amount: 1.0,
                temperature: 789.0,
            },
        ],
    };
    let mut bytes = Vec::new();
    body.serialize(&mut bytes, &materials).unwrap();
    let loaded = SceneDormantRigidBody::deserialize(&mut bytes.as_slice(), &materials).unwrap();
    assert_eq!(loaded.identifier, body.identifier);
    assert_eq!(
        loaded.position.map(f32::to_bits),
        body.position.map(f32::to_bits)
    );
    assert_eq!(loaded.rotation.to_bits(), body.rotation.to_bits());
    assert_eq!(
        loaded.linear_velocity.map(f32::to_bits),
        body.linear_velocity.map(f32::to_bits)
    );
    assert_eq!(
        loaded.angular_velocity.to_bits(),
        body.angular_velocity.to_bits()
    );
    assert_eq!(loaded.sleeping, body.sleeping);
    assert_eq!(loaded.cells[0].local, body.cells[0].local);
    assert_eq!(loaded.cells[0].appearance.0, body.cells[0].appearance.0);
    assert_eq!(loaded.cells[0].integrity, body.cells[0].integrity);
    assert_eq!(loaded.cells[1].amount, body.cells[1].amount);
    assert_eq!(loaded.cells[1].temperature, body.cells[1].temperature);
}

#[test]
fn test_malformed_dormant_rigid_is_rejected() {
    let (materials, material) = test_materials();
    let body = SceneDormantRigidBody {
        identifier: 1,
        position: [0.0, 0.0],
        rotation: 0.0,
        linear_velocity: [0.0; 2],
        angular_velocity: 0.0,
        sleeping: false,
        cells: vec![SceneDormantRigidCell {
            local: [0, 0],
            material,
            appearance: CellularAppearance::NEUTRAL,
            integrity: 1.0,
            amount: 0.0,
            temperature: 1.0,
        }],
    };
    assert_eq!(
        body.validate(&materials).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn test_rotated_geometry_bounds_drive_owner() {
    let bounds = world_aabb([10.0, 10.0], std::f32::consts::FRAC_PI_4, [[0, 0], [8, 0]]).unwrap();
    assert!(bounds.0[0] < 10.0 && bounds.1[0] > 10.0);
    let owner = owner_chunk([10.0, 10.0], std::f32::consts::FRAC_PI_4, [[0, 0], [8, 0]]).unwrap();
    assert_eq!((owner.x, owner.y), (0, 0));
}

#[test]
fn test_owner_mutations_preserve_current_records() {
    let (materials, material) = test_materials();
    let record = |identifier| SceneDormantRigidBody {
        identifier,
        position: [0.0, 0.0],
        rotation: 0.0,
        linear_velocity: [0.0; 2],
        angular_velocity: 0.0,
        sleeping: false,
        cells: vec![SceneDormantRigidCell {
            local: [0, 0],
            material,
            appearance: CellularAppearance::NEUTRAL,
            integrity: 1.0,
            amount: 1.0,
            temperature: 1.0,
        }],
    };
    let mut records = vec![record(1), record(2)];
    append_record(&mut records, record(3)).unwrap();
    remove_ids(&mut records, &[1]);
    assert_eq!(
        records
            .iter()
            .map(|record| record.identifier)
            .collect::<HashSet<_>>(),
        [2, 3].into_iter().collect()
    );
    assert!(append_record(&mut records, record(3)).is_err());
    records
        .iter()
        .try_for_each(|record| record.validate(&materials))
        .unwrap();
}
