// Copyright Rob Gage 2026

use super::{
    actor_physics_proxy::ActorPhysicsProxy, dynamic_tile::RigidDynamicCollisionTile,
    dynamic_tile_key::RigidDynamicCollisionTileKey,
    terrain_bridge_statistics::TerrainBridgeStatistics, terrain_patch::StaticTerrainCollisionPatch,
    terrain_patch_key::StaticTerrainCollisionPatchKey,
};
use crate::actors::{Actor, ActorCellularProxyState, ActorCollisionShape};
use crate::materials::MaterialRegistry;
use crate::simulation::simulation_constants::*;
use crate::simulation::{CollisionOccupancySnapshot, RigidCellularBody, RigidCellularBodyState};
use rapier2d::parry::query::ShapeCastOptions;
use rapier2d::prelude::{
    ColliderBuilder, ColliderHandle, LockedAxes, PhysicsWorld, Pose, QueryFilter, RigidBodyBuilder,
    RigidBodyHandle, SharedShape, Vector,
};
use std::collections::{HashMap, HashSet};
#[cfg(debug_assertions)]
use std::time::Instant;

/// Owns Rapier rigid bodies and the CPU-readable cellular collision snapshot
pub struct ScenePhysicsWorld {
    step_support: Vec<(RigidBodyHandle, Vector, f32)>,
    step_recovery: Vec<(RigidBodyHandle, Vector, f32)>,
    rapier: PhysicsWorld,
    /// Latest asynchronously completed canonical cellular collision snapshot
    cellular_terrain_snapshot: Option<CollisionOccupancySnapshot>,
    terrain_patches: HashMap<StaticTerrainCollisionPatchKey, StaticTerrainCollisionPatch>,
    required_terrain_patches: HashSet<StaticTerrainCollisionPatchKey>,
    dynamic_tiles: HashMap<RigidDynamicCollisionTileKey, RigidDynamicCollisionTile>,
    required_dynamic_tiles: HashSet<RigidDynamicCollisionTileKey>,
    terrain_tick: u64,
    terrain_statistics: TerrainBridgeStatistics,
    snapshot_updated_this_tick: bool,
    pawn_proxies: HashMap<Actor, ActorPhysicsProxy>,
}

#[path = "scene_physics_world_actor_movement.rs"]
mod scene_physics_world_actor_movement;
#[path = "scene_physics_world_terrain.rs"]
mod scene_physics_world_terrain;

impl Default for ScenePhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl ScenePhysicsWorld {
    /// Creates an empty scene collision world
    pub fn new() -> Self {
        Self {
            step_support: Vec::new(),
            step_recovery: Vec::new(),
            rapier: PhysicsWorld::new(),
            cellular_terrain_snapshot: None,
            terrain_patches: HashMap::new(),
            required_terrain_patches: HashSet::new(),
            dynamic_tiles: HashMap::new(),
            required_dynamic_tiles: HashSet::new(),
            terrain_tick: 0,
            terrain_statistics: TerrainBridgeStatistics::default(),
            snapshot_updated_this_tick: false,
            pawn_proxies: HashMap::new(),
        }
    }

    /// Inserts one dynamic body-local cellular compound
    pub(crate) fn insert_rigid_cellular_body(
        &mut self,
        position: [f32; 2],
        angle: f32,
        materials: &MaterialRegistry,
        cells: Vec<crate::simulation::RigidCellularBodyCell>,
        friction: f32,
        restitution: f32,
        linear_velocity: [f32; 2],
        angular_velocity: f32,
    ) -> RigidCellularBody {
        let mass_properties = RigidCellularBody::mass_properties(&cells, materials);
        let handle: RigidBodyHandle = self.rapier.insert_body(
            RigidBodyBuilder::dynamic()
                .translation(Vector::new(position[0], position[1]))
                .rotation(angle)
                .linvel(Vector::new(linear_velocity[0], linear_velocity[1]))
                .angvel(angular_velocity)
                .additional_mass_properties(mass_properties),
        );
        self.rapier.insert_collider(
            ColliderBuilder::new(RigidCellularBody::collision_shape(&cells))
                .density(0.0)
                .friction(friction)
                .restitution(restitution)
                .collision_groups(Self::rigid_collision_groups())
                .solver_groups(Self::rigid_solver_groups()),
            Some(handle),
        );
        RigidCellularBody {
            id: 0,
            handle,
            cells,
        }
    }

    /// Returns transform and velocities needed to derive a body's world-cell proxy
    pub(crate) fn rigid_cellular_body_state(
        &self,
        body: &RigidCellularBody,
    ) -> Option<RigidCellularBodyState> {
        let rigid_body = self.rapier.bodies.get(body.handle)?;
        assert!(
            !rigid_body
                .locked_axes()
                .intersects(LockedAxes::TRANSLATION_LOCKED_X | LockedAxes::TRANSLATION_LOCKED_Y,),
            "Rigid cellular Accelerator contact requires unlocked translation axes"
        );
        let position = rigid_body.position();
        let center = rigid_body.center_of_mass();
        Some(RigidCellularBodyState {
            translation: [position.translation.x, position.translation.y],
            angle: position.rotation.angle(),
            linear_velocity: [rigid_body.linvel().x, rigid_body.linvel().y],
            angular_velocity: rigid_body.angvel(),
            sleeping: rigid_body.is_sleeping(),
            center_of_mass: [center.x, center.y],
            inverse_mass: rigid_body.mass_properties().local_mprops.inv_mass,
            inverse_angular_inertia: rigid_body.mass_properties().effective_world_inv_inertia,
        })
    }

    /// Removes one rigid body and all attached colliders
    pub(crate) fn remove_rigid_cellular_body(&mut self, body: &RigidCellularBody) {
        self.rapier.remove_body(body.handle);
    }

    /// Holds a streamed body outside integration until its cellular support is ready.
    pub(crate) fn set_rigid_cellular_body_enabled(
        &mut self,
        body: &RigidCellularBody,
        enabled: bool,
    ) {
        if let Some(rigid_body) = self.rapier.bodies.get_mut(body.handle) {
            rigid_body.set_enabled(enabled);
        }
    }

    pub(crate) fn sleep_rigid_cellular_body(&mut self, body: &RigidCellularBody) {
        if let Some(rigid_body) = self.rapier.bodies.get_mut(body.handle) {
            rigid_body.sleep();
        }
    }

    /// Replaces the latest CPU-readable cellular collision snapshot
    pub fn update_cellular_snapshot(&mut self, snapshot: CollisionOccupancySnapshot) {
        self.cellular_terrain_snapshot = Some(snapshot);
        self.snapshot_updated_this_tick = true;
    }

    pub(crate) fn set_collision_snapshot_age(&mut self, age: u64) {
        self.terrain_statistics.collision_snapshot_age = age;
    }

    pub(crate) fn sync_pawn_proxies(
        &mut self,
        states: &[crate::actors::ActorPhysicsProxyState],
        up: Vector,
    ) {
        let mut live_actors: HashSet<Actor> = HashSet::new();
        for state in states {
            live_actors.insert(state.actor);
            let pose: Pose = state
                .shape
                .pose(Vector::new(state.center[0], state.center[1]), up);
            if let Some(proxy) = self.pawn_proxies.get_mut(&state.actor) {
                if proxy.shape != state.shape {
                    self.rapier.remove_collider(proxy.collider);
                    proxy.collider = self.rapier.insert_collider(
                        ColliderBuilder::new(state.shape.rapier_shape())
                            .collision_groups(Self::pawn_collision_groups())
                            .solver_groups(Self::pawn_solver_groups()),
                        Some(proxy.body),
                    );
                    proxy.shape = state.shape;
                }
                self.rapier
                    .bodies
                    .get_mut(proxy.body)
                    .unwrap()
                    .set_next_kinematic_position(pose);
            } else {
                let body: RigidBodyHandle = self
                    .rapier
                    .insert_body(RigidBodyBuilder::kinematic_position_based().pose(pose));
                let collider: ColliderHandle = self.rapier.insert_collider(
                    ColliderBuilder::new(state.shape.rapier_shape())
                        .collision_groups(Self::pawn_collision_groups())
                        .solver_groups(Self::pawn_solver_groups()),
                    Some(body),
                );
                self.pawn_proxies.insert(
                    state.actor,
                    ActorPhysicsProxy {
                        body,
                        collider,
                        shape: state.shape,
                    },
                );
            }
        }
        let stale_actors: Vec<Actor> = self
            .pawn_proxies
            .keys()
            .filter(|actor| !live_actors.contains(actor))
            .copied()
            .collect();
        for actor in stale_actors {
            if let Some(proxy) = self.pawn_proxies.remove(&actor) {
                self.rapier.remove_body(proxy.body);
            }
        }
    }

    /// Advances Rapier's collision world by one fixed scene step
    pub fn step(&mut self, gravity: [f32; 2], delta_time: f32) {
        #[cfg(debug_assertions)]
        let start_time: Instant = Instant::now();
        self.rapier.gravity = Vector::new(gravity[0], gravity[1]);
        self.rapier.integration_parameters.dt = delta_time;
        self.rapier.step();
        for (handle, velocity, angular) in self.step_recovery.drain(..) {
            if let Some(body) = self.rapier.bodies.get_mut(handle) {
                body.set_linvel(body.linvel() - velocity, false);
                body.set_angvel(body.angvel() - angular, false);
            }
        }
        for (handle, force, torque) in self.step_support.drain(..) {
            if let Some(body) = self.rapier.bodies.get_mut(handle) {
                body.add_force(-force, false);
                body.add_torque(-torque, false);
            }
        }
        #[cfg(debug_assertions)]
        tracing::trace!(
            elapsed_us = start_time.elapsed().as_micros(),
            "rapier rigid step"
        );
    }

    /// Applies one already-integrated Accelerator impulse batch to its authoritative body
    pub(crate) fn apply_rigid_constraint(
        &mut self,
        body: &RigidCellularBody,
        constraint: [f32; 4],
        source: [f32; 4],
        wake: bool,
    ) -> bool {
        let Some(state) = self.rigid_cellular_body_state(body) else {
            return false;
        };
        let effective: f32 = state.inverse_mass
            * (constraint[0] * constraint[0] + constraint[1] * constraint[1])
            + state.inverse_angular_inertia * constraint[2] * constraint[2];
        if effective <= 1e-12 {
            return self.apply_rigid_cellular_body_reaction(body, [0.0; 2], 0.0, 0.0, wake);
        }
        // The Accelerator impulse defines a velocity target along its generalized contact direction.
        // Re-evaluate that target against current motion instead of replaying stale stopping work.
        let target: f32 = source[0] * constraint[0]
            + source[1] * constraint[1]
            + source[2] * constraint[2]
            + effective;
        let target: f32 = if source[3] == 0.0 {
            target.max(0.0)
        } else {
            target
        };
        let current: f32 = state.linear_velocity[0] * constraint[0]
            + state.linear_velocity[1] * constraint[1]
            + state.angular_velocity * constraint[2];
        let scale: f32 = ((target - current) / effective).max(0.0);
        self.apply_rigid_cellular_body_reaction(
            body,
            [constraint[0] * scale, constraint[1] * scale],
            constraint[2] * scale,
            constraint[3],
            wake,
        )
    }

    pub(crate) fn apply_rigid_support(
        &mut self,
        body: &RigidCellularBody,
        support: [f32; 4],
    ) -> bool {
        let Some(rigid) = self.rapier.bodies.get_mut(body.handle) else {
            return false;
        };
        if rigid.is_sleeping() {
            return true;
        }
        // Spread the confirmed one-step impulse through Rapier's integration substeps.
        // An upfront velocity kick cancels final gravity velocity but introduces position drift.
        let impulse: Vector = Vector::new(support[0], support[1]);
        let quadratic: f32 = 0.5
            * (rigid.mass_properties().local_mprops.inv_mass * impulse.length_squared()
                + rigid.mass_properties().effective_world_inv_inertia * support[2] * support[2]);
        let linear: f32 = rigid.linvel().dot(impulse) + rigid.angvel() * support[2];
        let budget: f32 = support[3].max(0.0);
        let scale: f32 = if linear + quadratic <= budget {
            1.0
        } else if quadratic > 0.0 {
            ((linear * linear + 4.0 * quadratic * budget).sqrt() - linear) / (2.0 * quadratic)
        } else {
            0.0
        }
        .clamp(0.0, 1.0);
        let force: Vector = impulse * (60.0 * scale);
        let torque: f32 = support[2] * (60.0 * scale);
        rigid.add_force(force, false);
        rigid.add_torque(torque, false);
        self.step_support.push((body.handle, force, torque));
        true
    }

    pub(crate) fn apply_rigid_recovery(
        &mut self,
        body: &RigidCellularBody,
        recovery: [f32; 4],
    ) -> bool {
        let Some(before): Option<RigidCellularBodyState> = self.rigid_cellular_body_state(body)
        else {
            return false;
        };
        self.apply_rigid_cellular_body_reaction(
            body,
            [recovery[0], recovery[1]],
            recovery[2],
            recovery[3],
            false,
        );
        let after: RigidCellularBodyState = self.rigid_cellular_body_state(body).unwrap();
        self.step_recovery.push((
            body.handle,
            Vector::new(
                after.linear_velocity[0] - before.linear_velocity[0],
                after.linear_velocity[1] - before.linear_velocity[1],
            ),
            after.angular_velocity - before.angular_velocity,
        ));
        true
    }

    /// Applies one already-integrated Accelerator impulse batch to its authoritative body
    pub(crate) fn apply_rigid_cellular_body_reaction(
        &mut self,
        body: &RigidCellularBody,
        impulse: [f32; 2],
        angular_impulse: f32,
        energy_budget: f32,
        wake: bool,
    ) -> bool {
        let Some(rigid_body) = self.rapier.bodies.get_mut(body.handle) else {
            return false;
        };
        let inverse_mass: f32 = rigid_body.mass_properties().local_mprops.inv_mass;
        let inverse_inertia: f32 = rigid_body.mass_properties().effective_world_inv_inertia;
        let linear: Vector = Vector::new(impulse[0], impulse[1]);
        let quadratic: f32 = 0.5
            * (inverse_mass * linear.length_squared()
                + inverse_inertia * angular_impulse * angular_impulse);
        let linear_term: f32 =
            rigid_body.linvel().dot(linear) + rigid_body.angvel() * angular_impulse;
        let budget: f32 = energy_budget.max(0.0);
        let scale: f32 = if linear_term + quadratic <= budget {
            1.0
        } else if quadratic > 0.0 {
            ((linear_term * linear_term + 4.0 * quadratic * budget).sqrt() - linear_term)
                / (2.0 * quadratic)
        } else if linear_term > 0.0 {
            budget / linear_term
        } else {
            1.0
        }
        .clamp(0.0, 1.0);
        if rigid_body.is_sleeping() && !wake {
            return true;
        }
        if wake {
            rigid_body.wake_up(true);
        }
        if scale > 0.0 && impulse != [0.0; 2] {
            rigid_body.apply_impulse(linear * scale, wake);
        }
        if scale > 0.0 && angular_impulse != 0.0 {
            rigid_body.apply_torque_impulse(angular_impulse * scale, wake);
        }
        true
    }

    #[cfg(test)]
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

    #[cfg(test)]
    pub(crate) fn test_collider_is_enabled(&self, handle: ColliderHandle) -> bool {
        self.rapier
            .colliders
            .get(handle)
            .is_some_and(|collider| collider.is_enabled())
    }

    #[cfg(test)]
    pub(crate) fn test_sleep_rigid_cellular_body(&mut self, body: &RigidCellularBody) {
        if let Some(rigid_body) = self.rapier.bodies.get_mut(body.handle) {
            rigid_body.sleep();
        }
    }

    #[cfg(test)]
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
