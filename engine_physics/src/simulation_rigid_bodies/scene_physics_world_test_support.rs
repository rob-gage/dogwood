// Copyright Rob Gage 2026

use rapier2d::prelude::ColliderHandle;
use rapier2d::prelude::Vector;

use super::RigidDynamicCollisionTileKey;
use super::ScenePhysicsWorld;
use crate::simulation::RigidCellularBody;

impl ScenePhysicsWorld {
    pub(crate) fn test_dynamic_tile_collider(
        &self,
        coordinates: [i32; 2],
    ) -> Option<ColliderHandle> {
        self.dynamic_tiles
            .get(&RigidDynamicCollisionTileKey {
                x: coordinates[0],
                y: coordinates[1],
            })
            .and_then(|tile| tile.collider)
    }

    pub(crate) fn test_collider_is_enabled(&self, handle: ColliderHandle) -> bool {
        self.rapier
            .colliders
            .get(handle)
            .is_some_and(|collider| collider.is_enabled())
    }

    pub(crate) fn test_sleep_rigid_cellular_body(&mut self, body: &RigidCellularBody) {
        if let Some(rigid_body) = self.rapier.bodies.get_mut(body.handle) {
            rigid_body.sleep();
        }
    }

    pub(crate) fn test_apply_rigid_cellular_body_impulse(
        &mut self,
        body: &RigidCellularBody,
        impulse: [f32; 2],
    ) {
        if let Some(rigid_body) = self.rapier.bodies.get_mut(body.handle) {
            rigid_body.apply_impulse(Vector::new(impulse[0], impulse[1]), true);
        }
    }
}
