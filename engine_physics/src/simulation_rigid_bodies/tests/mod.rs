mod rigid_body_collision_tests;
mod rigid_body_test_actor_terrain;
mod rigid_body_test_material;
mod rigid_body_test_patch_bounds;

use crate::simulation_rigid_bodies::ScenePhysicsWorld;
use crate::{
    actors::{Actor, ActorCellularProxyState, ActorCollisionShape, ActorPhysicsProxyState},
    simulation::CollisionOccupancySnapshot,
    tiles::TileCoordinates,
};
use crate::{materials::MaterialRegistry, tiles::CellularAppearance};
use bevy_ecs::entity::Entity;
use rapier2d::prelude::{Pose, Vector};
use rigid_body_test_actor_terrain::prepare_actor_terrain;
use rigid_body_test_material::register_test_stone_material;

#[test]
fn test_every_actor_primitive_casts_against_static_terrain_patches() {
    for shape in [
        ActorCollisionShape::Circle { radius: 0.25 },
        ActorCollisionShape::Capsule {
            radius: 0.25,
            height: 0.75,
        },
        ActorCollisionShape::Rectangle {
            width: 0.5,
            height: 0.75,
        },
    ] {
        let mut physics = ScenePhysicsWorld::new();
        physics.update_cellular_snapshot(CollisionOccupancySnapshot {
            sequence: 0,
            origin: TileCoordinates { x: 0, y: 0 },
            width: 1,
            height: 1,
            static_masks: vec![[0, 1 << 4]].into_boxed_slice(),
            dynamic_masks: vec![[0; 2]].into_boxed_slice(),
        });
        prepare_actor_terrain(&mut physics, shape);
        let (movement, _) = physics.move_actor(
            shape,
            Vector::new(0.0, 0.5625),
            Vector::new(1.0, 0.0),
            Vector::Y,
            0.0,
            0.0,
            &mut |_| {},
        );
        assert!(movement.x < 0.5);
    }
}

#[test]
fn test_dynamic_cells_create_hard_pawn_collision() {
    let mut physics = ScenePhysicsWorld::new();
    physics.update_cellular_snapshot(CollisionOccupancySnapshot {
        sequence: 0,
        origin: TileCoordinates { x: 0, y: 0 },
        width: 1,
        height: 1,
        static_masks: vec![[0, 0]].into_boxed_slice(),
        dynamic_masks: vec![[0, 1]].into_boxed_slice(),
    });
    let shape = ActorCollisionShape::Circle { radius: 0.25 };
    prepare_actor_terrain(&mut physics, shape);
    let (movement, _) = physics.move_actor(
        shape,
        Vector::new(0.5625, 0.5625),
        Vector::new(-0.6, 0.0),
        Vector::Y,
        0.0,
        0.0,
        &mut |_| {},
    );
    assert!(
        movement.x > -0.6,
        "pawn passed through dynamic sand: {:?}",
        movement
    );
    physics.update_cellular_snapshot(CollisionOccupancySnapshot {
        sequence: 1,
        origin: TileCoordinates { x: 0, y: 0 },
        width: 1,
        height: 1,
        static_masks: vec![[0; 2]].into_boxed_slice(),
        dynamic_masks: vec![[0; 2]].into_boxed_slice(),
    });
    let actor = [ActorCellularProxyState {
        center: [0.5625, 0.5625],
        velocity: [0.0; 2],
        drive: [-1.0, 0.0],
        shape,
        occupancy_kind: 1,
        mass: 0.0,
    }];
    physics.prepare_cellular_terrain(&[], &actor, [0.0; 2], 1.0 / 60.0);
    physics.step([0.0; 2], 1.0 / 60.0);
    let (cleared, _) = physics.move_actor(
        shape,
        Vector::new(0.5625, 0.5625),
        Vector::new(-0.6, 0.0),
        Vector::Y,
        0.0,
        0.0,
        &mut |_| {},
    );
    assert!((cleared.x + 0.6).abs() < 1e-4);
}

#[test]
fn test_dynamic_tile_geometry_and_cache_are_local() {
    assert!(ScenePhysicsWorld::dynamic_tile_shape([0; 2]).0.is_none());
    let negative = CollisionOccupancySnapshot {
        sequence: 0,
        origin: TileCoordinates { x: -3, y: -2 },
        width: 1,
        height: 1,
        static_masks: vec![[0; 2]].into_boxed_slice(),
        dynamic_masks: vec![[u32::MAX; 2]].into_boxed_slice(),
    };
    assert_eq!(negative.dynamic_tile_mask(-3, -2), [u32::MAX; 2]);
    assert_eq!(negative.dynamic_tile_mask(-4, -2), [0; 2]);
    for (mask, expected, rectangles) in [
        ([u32::MAX; 2], [-3.0, -2.0, -2.0, -1.0], 1),
        ([u32::MAX, 0], [-3.0, -2.0, -2.0, -1.5], 1),
    ] {
        let (shape, count) = ScenePhysicsWorld::dynamic_tile_shape(mask);
        assert_eq!(count, rectangles);
        let aabb = shape.unwrap().compute_aabb(&Pose::translation(-3.0, -2.0));
        assert!((aabb.mins.x - expected[0]).abs() < 1e-5);
        assert!((aabb.mins.y - expected[1]).abs() < 1e-5);
        assert!((aabb.maxs.x - expected[2]).abs() < 1e-5);
        assert!((aabb.maxs.y - expected[3]).abs() < 1e-5);
    }
    let mut world = ScenePhysicsWorld::new();
    let actor = ActorCellularProxyState {
        center: [0.5, 0.5],
        velocity: [0.0; 2],
        drive: [0.0; 2],
        shape: ActorCollisionShape::Circle { radius: 0.25 },
        occupancy_kind: 1,
        mass: 0.0,
    };
    let actors = [actor];
    let snapshot = |near: [u32; 2], far: [u32; 2]| CollisionOccupancySnapshot {
        sequence: 0,
        origin: TileCoordinates { x: 0, y: 0 },
        width: 16,
        height: 1,
        static_masks: vec![[0; 2]; 16].into_boxed_slice(),
        dynamic_masks: {
            let mut v = vec![[0; 2]; 16];
            v[0] = near;
            v[15] = far;
            v.into_boxed_slice()
        },
    };
    world.update_cellular_snapshot(snapshot([1, 0], [0; 2]));
    world.prepare_cellular_terrain(&[], &actors, [0.0; 2], 1.0 / 60.0);
    let first = world.terrain_bridge_statistics();
    assert_eq!(first.dynamic_shape_rebuilds, 1);
    assert_eq!(first.dynamic_cells_scanned, 64);
    world.prepare_cellular_terrain(&[], &actors, [0.0; 2], 1.0 / 60.0);
    assert_eq!(world.terrain_bridge_statistics().dynamic_shape_rebuilds, 1);
    world.update_cellular_snapshot(snapshot([1, 0], [1, 0]));
    world.prepare_cellular_terrain(&[], &actors, [0.0; 2], 1.0 / 60.0);
    assert_eq!(world.terrain_bridge_statistics().dynamic_shape_rebuilds, 1);
    world.update_cellular_snapshot(snapshot([3, 0], [1, 0]));
    world.prepare_cellular_terrain(&[], &actors, [0.0; 2], 1.0 / 60.0);
    let last = world.terrain_bridge_statistics();
    assert_eq!(last.dynamic_shape_rebuilds, 2);
    assert_eq!(last.dynamic_set_shape_calls, 1);
    assert_eq!(last.dynamic_cells_scanned, 128);
    let handle = world.test_dynamic_tile_collider([0, 0]).unwrap();
    world.update_cellular_snapshot(snapshot([0; 2], [1, 0]));
    world.prepare_cellular_terrain(&[], &actors, [0.0; 2], 1.0 / 60.0);
    assert!(!world.test_collider_is_enabled(handle));
    world.update_cellular_snapshot(snapshot([1, 0], [1, 0]));
    world.prepare_cellular_terrain(&[], &actors, [0.0; 2], 1.0 / 60.0);
    assert_eq!(world.test_dynamic_tile_collider([0, 0]), Some(handle));
    assert!(world.test_collider_is_enabled(handle));
}

#[test]
fn test_settled_dynamic_floor_supports_rigid_and_removed_floor_releases_it() {
    let mut materials = MaterialRegistry::new();
    let stone = register_test_stone_material(&mut materials, 1000.0, 100.0);
    let mut world = ScenePhysicsWorld::new();
    let snapshot = |mask| CollisionOccupancySnapshot {
        sequence: 0,
        origin: TileCoordinates { x: 0, y: 0 },
        width: 1,
        height: 1,
        static_masks: vec![[0; 2]].into_boxed_slice(),
        dynamic_masks: vec![mask].into_boxed_slice(),
    };
    world.update_cellular_snapshot(snapshot([u32::MAX; 2]));
    let body = world.insert_rigid_cellular_body(
        [0.5, 1.5],
        0.0,
        &materials,
        vec![crate::simulation::RigidCellularBodyCell::test_cell(
            [0, 0],
            stone,
            CellularAppearance::NEUTRAL,
        )],
        0.5,
        0.0,
        [0.0; 2],
        0.0,
    );
    for _ in 0..180 {
        world.prepare_cellular_terrain(std::slice::from_ref(&body), &[], [0.0, -9.81], 1.0 / 60.0);
        world.step([0.0, -9.81], 1.0 / 60.0);
    }
    let supported = world.rigid_cellular_body_state(&body).unwrap().translation[1];
    assert!(supported > 0.95, "rigid sank through sand: {supported}");
    for _ in 0..60 {
        world.test_apply_rigid_cellular_body_impulse(&body, [0.0, -0.001]);
        world.prepare_cellular_terrain(std::slice::from_ref(&body), &[], [0.0, -9.81], 1.0 / 60.0);
        world.step([0.0, -9.81], 1.0 / 60.0);
    }
    assert!(world.rigid_cellular_body_state(&body).unwrap().translation[1] > 0.95);
    let mut minimum_moving = f32::MAX;
    let mut maximum_moving = f32::MIN;
    for tick in 0..60 {
        world.update_cellular_snapshot(snapshot([
            u32::MAX ^ if tick % 2 == 0 { 1 << 3 } else { 0 },
            u32::MAX,
        ]));
        world.prepare_cellular_terrain(std::slice::from_ref(&body), &[], [0.0, -9.81], 1.0 / 60.0);
        world.step([0.0, -9.81], 1.0 / 60.0);
        let height = world.rigid_cellular_body_state(&body).unwrap().translation[1];
        minimum_moving = minimum_moving.min(height);
        maximum_moving = maximum_moving.max(height);
    }
    assert!(
        minimum_moving > 0.95 && maximum_moving - minimum_moving < 0.01,
        "rigid jittered or sank on moving sand: {minimum_moving}..{maximum_moving}"
    );
    world.update_cellular_snapshot(snapshot([u32::MAX; 2]));
    world.prepare_cellular_terrain(std::slice::from_ref(&body), &[], [0.0, -9.81], 1.0 / 60.0);
    let rebuilds = world.terrain_bridge_statistics().dynamic_shape_rebuilds;
    for _ in 0..10 {
        world.prepare_cellular_terrain(std::slice::from_ref(&body), &[], [0.0, -9.81], 1.0 / 60.0);
        world.step([0.0, -9.81], 1.0 / 60.0);
    }
    assert_eq!(
        world.terrain_bridge_statistics().dynamic_shape_rebuilds,
        rebuilds
    );
    world.test_sleep_rigid_cellular_body(&body);
    world.update_cellular_snapshot(snapshot([0; 2]));
    world.prepare_cellular_terrain(std::slice::from_ref(&body), &[], [0.0, -9.81], 1.0 / 60.0);
    assert!(!world.rigid_cellular_body_state(&body).unwrap().sleeping);
    for _ in 0..30 {
        world.step([0.0, -9.81], 1.0 / 60.0);
    }
    assert!(world.rigid_cellular_body_state(&body).unwrap().translation[1] < supported - 0.1);
}

#[test]
fn test_distant_unsettled_sand_needs_no_rapier_geometry() {
    let mut world = ScenePhysicsWorld::new();
    for tick in 0..100 {
        world.update_cellular_snapshot(CollisionOccupancySnapshot {
            sequence: tick,
            origin: TileCoordinates { x: -64, y: -64 },
            width: 128,
            height: 128,
            static_masks: vec![[0; 2]; 128 * 128].into_boxed_slice(),
            dynamic_masks: vec![[if tick % 2 == 0 { u32::MAX } else { 0 }; 2]; 128 * 128]
                .into_boxed_slice(),
        });
        world.prepare_cellular_terrain(&[], &[], [0.0, -9.81], 1.0 / 60.0);
    }
    let stats = world.terrain_bridge_statistics();
    assert_eq!(stats.dynamic_required_tiles, 0);
    assert_eq!(stats.dynamic_shape_rebuilds, 0);
    assert_eq!(stats.dynamic_cells_scanned, 0);
}

#[test]
fn test_dynamic_group_filters_and_neighbor_tiles_have_no_gap() {
    let rigid = ScenePhysicsWorld::rigid_collision_groups();
    let dynamic = ScenePhysicsWorld::dynamic_collision_groups();
    let static_terrain = ScenePhysicsWorld::terrain_collision_groups();
    assert!(rigid.test(dynamic));
    assert!(rigid.test(ScenePhysicsWorld::pawn_solver_groups()));
    assert!(
        !ScenePhysicsWorld::pawn_solver_groups().test(ScenePhysicsWorld::terrain_solver_groups())
    );
    assert!(
        !ScenePhysicsWorld::pawn_solver_groups().test(ScenePhysicsWorld::dynamic_solver_groups())
    );
    assert!(
        ScenePhysicsWorld::rigid_solver_groups().test(ScenePhysicsWorld::dynamic_solver_groups())
    );
    assert!(rigid.test(ScenePhysicsWorld::pawn_query_groups()));
    assert!(static_terrain.test(ScenePhysicsWorld::pawn_query_groups()));
    assert!(dynamic.test(ScenePhysicsWorld::pawn_query_groups()));
    assert!(
        !ScenePhysicsWorld::pawn_collision_groups().test(ScenePhysicsWorld::pawn_query_groups())
    );
    assert!(!dynamic.test(static_terrain));
    assert!(!dynamic.test(dynamic));
    let shape = ScenePhysicsWorld::dynamic_tile_shape([u32::MAX; 2])
        .0
        .unwrap();
    let left = shape.compute_aabb(&Pose::translation(-2.0, 0.0));
    let right = shape.compute_aabb(&Pose::translation(-1.0, 0.0));
    assert!((left.maxs.x - right.mins.x).abs() < 1e-5);
}

#[test]
fn test_pawn_proxy_is_ignored_by_locomotion_casts() {
    let mut world = ScenePhysicsWorld::new();
    let shape = ActorCollisionShape::Circle { radius: 0.25 };
    world.sync_pawn_proxies(
        &[ActorPhysicsProxyState {
            actor: Actor::from_bevy_entity(Entity::from_raw_u32(1).unwrap()),
            center: [0.75, 0.0],
            shape,
        }],
        Vector::Y,
    );
    let (movement, _) = world.move_actor(
        shape,
        Vector::ZERO,
        Vector::new(0.5, 0.0),
        Vector::Y,
        0.0,
        0.0,
        &mut |_| {},
    );
    assert!(
        (movement.x - 0.5).abs() < 1e-5,
        "pawn proxy blocked cast: {movement:?}"
    );
}

#[test]
fn test_rigid_body_still_blocks_locomotion_casts() {
    let mut materials = MaterialRegistry::new();
    let stone = register_test_stone_material(&mut materials, 1000.0, 100.0);
    let mut world = ScenePhysicsWorld::new();
    world.insert_rigid_cellular_body(
        [1.0, 0.5],
        0.0,
        &materials,
        vec![crate::simulation::RigidCellularBodyCell::test_cell(
            [0, 0],
            stone,
            CellularAppearance::NEUTRAL,
        )],
        0.5,
        0.0,
        [0.0; 2],
        0.0,
    );
    world.step([0.0; 2], 1.0 / 60.0);
    let (movement, _) = world.move_actor(
        ActorCollisionShape::Circle { radius: 0.25 },
        Vector::new(0.0, 0.5),
        Vector::new(2.0, 0.0),
        Vector::Y,
        0.0,
        0.0,
        &mut |_| {},
    );
    assert!(
        movement.x < 2.0,
        "pawn passed through rigid body: {movement:?}"
    );
}

#[test]
fn test_pawn_is_supported_by_settled_dynamic_sand() {
    let mut world = ScenePhysicsWorld::new();
    world.update_cellular_snapshot(CollisionOccupancySnapshot {
        sequence: 0,
        origin: TileCoordinates { x: 0, y: 0 },
        width: 1,
        height: 1,
        static_masks: vec![[0; 2]].into_boxed_slice(),
        dynamic_masks: vec![[u32::MAX; 2]].into_boxed_slice(),
    });
    let shape = ActorCollisionShape::Circle { radius: 0.25 };
    let actor = [ActorCellularProxyState {
        center: [0.5, 1.5],
        velocity: [0.0; 2],
        drive: [0.0; 2],
        shape,
        occupancy_kind: 1,
        mass: 0.0,
    }];
    world.prepare_cellular_terrain(&[], &actor, [0.0, -9.81], 1.0 / 60.0);
    world.step([0.0, -9.81], 1.0 / 60.0);
    let (motion, grounded) = world.move_actor(
        shape,
        Vector::new(0.5, 1.5),
        Vector::new(0.0, -0.6),
        Vector::Y,
        0.5,
        0.0,
        &mut |_| {},
    );
    assert!(
        grounded && motion.y > -0.6,
        "pawn fell through sand: {motion:?}"
    );
}

#[test]
fn test_distributed_rigids_demand_local_dynamic_tiles() {
    let mut materials = MaterialRegistry::new();
    let stone = register_test_stone_material(&mut materials, 1000.0, 100.0);
    let mut world = ScenePhysicsWorld::new();
    world.update_cellular_snapshot(CollisionOccupancySnapshot {
        sequence: 0,
        origin: TileCoordinates { x: -64, y: -64 },
        width: 128,
        height: 128,
        static_masks: vec![[0; 2]; 128 * 128].into_boxed_slice(),
        dynamic_masks: vec![[u32::MAX; 2]; 128 * 128].into_boxed_slice(),
    });
    let bodies: Vec<_> = [[-30.0, -30.0], [-30.0, 30.0], [30.0, -30.0], [30.0, 30.0]]
        .into_iter()
        .map(|position| {
            world.insert_rigid_cellular_body(
                position,
                0.0,
                &materials,
                vec![crate::simulation::RigidCellularBodyCell::test_cell(
                    [0, 0],
                    stone,
                    CellularAppearance::NEUTRAL,
                )],
                0.5,
                0.0,
                [0.0; 2],
                0.0,
            )
        })
        .collect();
    world.prepare_cellular_terrain(&bodies, &[], [0.0, -9.81], 1.0 / 60.0);
    let stats = world.terrain_bridge_statistics();
    assert!(stats.dynamic_required_tiles <= 100 && stats.dynamic_required_tiles > 4);
    assert_eq!(
        stats.dynamic_shape_rebuilds as usize,
        stats.dynamic_required_tiles
    );
    assert_eq!(
        stats.dynamic_cells_scanned as usize,
        stats.dynamic_required_tiles * 64
    );
}

#[test]
fn test_moving_sand_shape_work_stays_near_pawn() {
    let mut world = ScenePhysicsWorld::new();
    let actor = [ActorCellularProxyState {
        center: [0.5, 0.5],
        velocity: [0.0; 2],
        drive: [0.0; 2],
        shape: ActorCollisionShape::Circle { radius: 0.25 },
        occupancy_kind: 1,
        mass: 0.0,
    }];
    let mut total = std::time::Duration::ZERO;
    let mut max_required = 0;
    for tick in 0..50 {
        world.update_cellular_snapshot(CollisionOccupancySnapshot {
            sequence: tick,
            origin: TileCoordinates { x: -64, y: -64 },
            width: 128,
            height: 128,
            static_masks: vec![[0; 2]; 128 * 128].into_boxed_slice(),
            dynamic_masks: vec![[if tick % 2 == 0 { u32::MAX } else { 0 }; 2]; 128 * 128]
                .into_boxed_slice(),
        });
        let start = std::time::Instant::now();
        world.prepare_cellular_terrain(&[], &actor, [0.0, -9.81], 1.0 / 60.0);
        total += start.elapsed();
        max_required = max_required.max(world.terrain_bridge_statistics().dynamic_required_tiles);
    }
    let stats = world.terrain_bridge_statistics();
    assert!(
        max_required < 64,
        "demand expanded beyond pawn neighborhood: {max_required}"
    );
    assert!(stats.dynamic_shape_rebuilds < 50 * 64);
    assert!(stats.dynamic_cells_scanned <= stats.dynamic_mask_changes * 64);
    eprintln!(
        "moving-sand bridge: {:.3} ms/tick, max required {}, rebuilds {}, set_shape {}, scanned {}",
        total.as_secs_f64() * 1000.0 / 50.0,
        max_required,
        stats.dynamic_shape_rebuilds,
        stats.dynamic_set_shape_calls,
        stats.dynamic_cells_scanned
    );
}
