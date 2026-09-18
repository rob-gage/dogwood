// Copyright Rob Gage 2026

use super::rigid_body_test_material::register_test_stone_material;
use super::rigid_body_test_patch_bounds::assert_patch_bounds;
use crate::materials::MaterialIdentifier;
use crate::materials::MaterialRegistry;
use crate::simulation::CollisionOccupancySnapshot;
use crate::simulation::RigidCellularBody;
use crate::simulation::RigidCellularBodyState;
use crate::simulation_rigid_bodies::ScenePhysicsWorld;
use crate::tiles::CellularAppearance;
use crate::tiles::TileCoordinates;

#[test]
fn test_full_and_almost_full_patch_keep_the_same_exterior_bounds() {
    let full: [[u32; 2]; 16] = [[u32::MAX; 2]; 16];
    assert_patch_bounds(full, [0.0, -4.0], [0.0, -4.0, 4.0, 0.0]);
    assert_patch_bounds(full, [-8.0, 12.0], [-8.0, 12.0, -4.0, 16.0]);
    let mut hole: [[u32; 2]; 16] = full;
    hole[5][0] &= !(1 << 9); // Interior cell; exterior must not move.
    assert_patch_bounds(hole, [0.0, -4.0], [0.0, -4.0, 4.0, 0.0]);
}

#[test]
fn test_generated_full_floor_and_one_pixel_edit_have_the_same_rest_height() {
    let mut materials: MaterialRegistry = MaterialRegistry::new();
    let stone: MaterialIdentifier = register_test_stone_material(&mut materials, 1.0, 1.0);
    let mut world: ScenePhysicsWorld = ScenePhysicsWorld::new();
    let snapshot: &dyn Fn(bool) -> CollisionOccupancySnapshot = &|hole: bool| {
        let mut masks: Vec<[u32; 2]> = vec![[u32::MAX; 2]; 16];
        if hole {
            masks[5][0] &= !(1 << 9);
        }
        CollisionOccupancySnapshot {
            sequence: 0,
            origin: TileCoordinates { x: 0, y: -4 },
            width: 4,
            height: 4,
            static_masks: masks.into_boxed_slice(),
            dynamic_masks: vec![[0; 2]; 16].into_boxed_slice(),
        }
    };
    world.update_cellular_snapshot(snapshot(false));
    let body: RigidCellularBody = world.insert_rigid_cellular_body(
        [1.0, 1.0],
        0.0,
        &materials,
        vec![crate::simulation::RigidCellularBodyCell::test_cell(
            [0, 0],
            stone,
            CellularAppearance::NEUTRAL,
        )],
        0.5,
        0.0,
        [0.0, 0.0],
        0.0,
    );
    for _ in 0..180 {
        world.prepare_cellular_terrain(std::slice::from_ref(&body), &[], [0.0, -9.81], 1.0 / 60.0);
        world.step([0.0, -9.81], 1.0 / 60.0);
    }
    let full_height: f32 = world.rigid_cellular_body_state(&body).unwrap().translation[1];
    world.update_cellular_snapshot(snapshot(true));
    for _ in 0..60 {
        world.prepare_cellular_terrain(std::slice::from_ref(&body), &[], [0.0, -9.81], 1.0 / 60.0);
        world.step([0.0, -9.81], 1.0 / 60.0);
    }
    let edited_height: f32 = world.rigid_cellular_body_state(&body).unwrap().translation[1];
    assert!(
        full_height > -0.01 && (full_height - edited_height).abs() < 0.01,
        "full={full_height}, edited={edited_height}"
    );
}

#[test]
fn test_stale_rigid_reaction_cannot_create_energy_without_grid_transfer() {
    let mut materials: MaterialRegistry = MaterialRegistry::new();
    let stone: MaterialIdentifier = register_test_stone_material(&mut materials, 1000.0, 100.0);
    let mut physics: ScenePhysicsWorld = ScenePhysicsWorld::new();
    let body: RigidCellularBody = physics.insert_rigid_cellular_body(
        [0.0, 0.0],
        0.0,
        &materials,
        vec![crate::simulation::RigidCellularBodyCell::test_cell(
            [0, 0],
            stone,
            CellularAppearance::NEUTRAL,
        )],
        0.5,
        0.0,
        [0.0, 0.0],
        0.0,
    );
    assert!(physics.apply_rigid_cellular_body_reaction(&body, [1.0, 0.0], 0.0, 0.0, true));
    physics.step([0.0, 0.0], 1.0 / 60.0);
    let state: RigidCellularBodyState = physics.rigid_cellular_body_state(&body).unwrap();
    assert!(state.linear_velocity[0].abs() < 0.0001);
    assert!(physics.apply_rigid_cellular_body_reaction(&body, [1.0, 0.0], 0.0, 0.5, true));
    physics.step([0.0, 0.0], 1.0 / 60.0);
    let state: RigidCellularBodyState = physics.rigid_cellular_body_state(&body).unwrap();
    assert!(state.linear_velocity[0] > 0.0);
}

#[test]
fn test_rigid_cellular_bodies_still_collide_through_rapier() {
    let mut materials: MaterialRegistry = MaterialRegistry::new();
    let stone: MaterialIdentifier = register_test_stone_material(&mut materials, 1000.0, 100.0);
    let cell: &dyn Fn() -> Vec<crate::simulation::RigidCellularBodyCell> = &|| {
        vec![crate::simulation::RigidCellularBodyCell::test_cell(
            [0, 0],
            stone,
            CellularAppearance::NEUTRAL,
        )]
    };
    let mut physics: ScenePhysicsWorld = ScenePhysicsWorld::new();
    let left: RigidCellularBody = physics.insert_rigid_cellular_body(
        [0.0, 0.0],
        0.0,
        &materials,
        cell(),
        0.5,
        0.0,
        [1.0, 0.0],
        0.0,
    );
    let right: RigidCellularBody = physics.insert_rigid_cellular_body(
        [0.3, 0.0],
        0.0,
        &materials,
        cell(),
        0.5,
        0.0,
        [-1.0, 0.0],
        0.0,
    );
    for _ in 0..20 {
        physics.step([0.0, 0.0], 1.0 / 60.0);
    }
    let left_x: f32 = physics
        .rigid_cellular_body_state(&left)
        .unwrap()
        .translation[0];
    let right_x: f32 = physics
        .rigid_cellular_body_state(&right)
        .unwrap()
        .translation[0];
    assert!(
        left_x + 0.12 <= right_x,
        "rigid bodies interpenetrated: {left_x}, {right_x}"
    );
}
