// Copyright Rob Gage 2026

use crate::simulation_rigid_bodies::ScenePhysicsWorld;
use rapier2d::prelude::Pose;

pub(super) fn assert_patch_bounds(masks: [[u32; 2]; 16], origin: [f32; 2], expected: [f32; 4]) {
    let shape = ScenePhysicsWorld::terrain_patch_shape(&masks).unwrap();
    let aabb = shape.compute_aabb(&Pose::translation(origin[0], origin[1]));
    assert!(
        (aabb.mins.x - expected[0]).abs() < 1e-5
            && (aabb.mins.y - expected[1]).abs() < 1e-5
            && (aabb.maxs.x - expected[2]).abs() < 1e-5
            && (aabb.maxs.y - expected[3]).abs() < 1e-5,
        "actual {:?}..{:?}",
        aabb.mins,
        aabb.maxs
    );
}
