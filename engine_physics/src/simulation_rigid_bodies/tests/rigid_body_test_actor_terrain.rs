// Copyright Rob Gage 2026

use crate::actors::ActorCellularProxyState;
use crate::actors::ActorCollisionShape;
use crate::simulation_rigid_bodies::ScenePhysicsWorld;

pub(super) fn prepare_actor_terrain(physics: &mut ScenePhysicsWorld, shape: ActorCollisionShape) {
    let actor_cellular_proxy_state: ActorCellularProxyState = ActorCellularProxyState {
        center: [0.0, 0.5625],
        velocity: [0.0; 2],
        drive: [0.0; 2],
        shape,
        occupancy_kind: 1,
        mass: 0.0,
    };
    physics.prepare_cellular_terrain(&[], &[actor_cellular_proxy_state], [0.0; 2], 1.0 / 60.0);
    physics.step([0.0; 2], 1.0 / 60.0);
}
