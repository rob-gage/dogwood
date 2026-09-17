// Copyright Rob Gage 2026

use super::{
    actor_physics_proxy::ActorPhysicsProxy, dynamic_tile::DynamicTile,
    dynamic_tile_key::DynamicTileKey, terrain_bridge_statistics::TerrainBridgeStatistics,
    terrain_patch::TerrainPatch, terrain_patch_key::TerrainPatchKey,
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
    terrain_patches: HashMap<TerrainPatchKey, TerrainPatch>,
    required_terrain_patches: HashSet<TerrainPatchKey>,
    dynamic_tiles: HashMap<DynamicTileKey, DynamicTile>,
    required_dynamic_tiles: HashSet<DynamicTileKey>,
    terrain_tick: u64,
    terrain_statistics: TerrainBridgeStatistics,
    snapshot_updated_this_tick: bool,
    pawn_proxies: HashMap<Actor, ActorPhysicsProxy>,
}

#[path = "scene_physics_world_actor_movement.rs"]
mod scene_physics_world_actor_movement;

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
        let mut live = HashSet::new();
        for state in states {
            live.insert(state.actor);
            let pose = state
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
                let body = self
                    .rapier
                    .insert_body(RigidBodyBuilder::kinematic_position_based().pose(pose));
                let collider = self.rapier.insert_collider(
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
        let stale: Vec<_> = self
            .pawn_proxies
            .keys()
            .filter(|actor| !live.contains(actor))
            .copied()
            .collect();
        for actor in stale {
            if let Some(proxy) = self.pawn_proxies.remove(&actor) {
                self.rapier.remove_body(proxy.body);
            }
        }
    }

    pub(super) fn pawn_collider_at(
        &self,
        shape: ActorCollisionShape,
        position: Vector,
    ) -> Option<ColliderHandle> {
        self.pawn_proxies
            .values()
            .filter_map(|proxy| {
                let body = self.rapier.bodies.get(proxy.body)?;
                (proxy.shape == shape).then_some((
                    (body.position().translation - position).length_squared(),
                    proxy.collider,
                ))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, collider)| collider)
    }

    fn demand_dynamic_tiles(required: &mut HashSet<DynamicTileKey>, lo: Vector, hi: Vector) {
        for y in (lo.y.floor() as i32 - 1)..=(hi.y.floor() as i32 + 1) {
            for x in (lo.x.floor() as i32 - 1)..=(hi.x.floor() as i32 + 1) {
                required.insert(DynamicTileKey { x, y });
            }
        }
    }

    pub(crate) fn prepare_cellular_terrain(
        &mut self,
        bodies: &[RigidCellularBody],
        actors: &[ActorCellularProxyState],
        gravity: [f32; 2],
        dt: f32,
    ) {
        #[cfg(debug_assertions)]
        let started = Instant::now();
        self.terrain_tick += 1;
        if !self.snapshot_updated_this_tick {
            self.terrain_statistics.collision_snapshot_age += 1;
        }
        self.snapshot_updated_this_tick = false;
        self.required_terrain_patches.clear();
        self.required_dynamic_tiles.clear();
        for body in bodies {
            let Some(rigid) = self.rapier.bodies.get(body.handle) else {
                continue;
            };
            for handle in rigid.colliders() {
                let Some(collider) = self.rapier.colliders.get(*handle) else {
                    continue;
                };
                let aabb = collider.compute_aabb();
                let radius = (aabb.maxs - aabb.mins).length() * 0.5;
                let d = rigid.linvel() * dt + Vector::new(gravity[0], gravity[1]) * (0.5 * dt * dt);
                let angular = rigid.angvel().abs() * dt * radius;
                let lo = aabb.mins.min(aabb.mins + d) - Vector::splat(angular + 0.25);
                let hi = aabb.maxs.max(aabb.maxs + d) + Vector::splat(angular + 0.25);
                Self::demand_dynamic_tiles(&mut self.required_dynamic_tiles, lo, hi);
                let x0 =
                    ((lo.x * 8.0).floor() as i32).div_euclid(TERRAIN_COLLISION_PATCH_CELLS) - 1;
                let y0 =
                    ((lo.y * 8.0).floor() as i32).div_euclid(TERRAIN_COLLISION_PATCH_CELLS) - 1;
                let x1 =
                    ((hi.x * 8.0).floor() as i32).div_euclid(TERRAIN_COLLISION_PATCH_CELLS) + 1;
                let y1 =
                    ((hi.y * 8.0).floor() as i32).div_euclid(TERRAIN_COLLISION_PATCH_CELLS) + 1;
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        self.required_terrain_patches
                            .insert(TerrainPatchKey { x, y });
                    }
                }
            }
        }
        for actor in actors {
            let center = Vector::new(actor.center[0], actor.center[1]);
            let radius = actor
                .shape
                .nominal_dimensions()
                .into_iter()
                .fold(0.0f32, f32::max)
                * 0.5;
            let d = Vector::new(actor.velocity[0], actor.velocity[1]) * dt
                + Vector::new(gravity[0], gravity[1]) * (0.5 * dt * dt);
            let lo = center.min(center + d) - Vector::splat(radius + 0.25);
            let hi = center.max(center + d) + Vector::splat(radius + 0.25);
            Self::demand_dynamic_tiles(&mut self.required_dynamic_tiles, lo, hi);
            let x0 = ((lo.x * 8.0).floor() as i32).div_euclid(TERRAIN_COLLISION_PATCH_CELLS) - 1;
            let y0 = ((lo.y * 8.0).floor() as i32).div_euclid(TERRAIN_COLLISION_PATCH_CELLS) - 1;
            let x1 = ((hi.x * 8.0).floor() as i32).div_euclid(TERRAIN_COLLISION_PATCH_CELLS) + 1;
            let y1 = ((hi.y * 8.0).floor() as i32).div_euclid(TERRAIN_COLLISION_PATCH_CELLS) + 1;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    self.required_terrain_patches
                        .insert(TerrainPatchKey { x, y });
                }
            }
        }
        let Some(snapshot) = self.cellular_terrain_snapshot.as_ref() else {
            return;
        };
        self.terrain_statistics.dynamic_required_tiles = self.required_dynamic_tiles.len();
        let dynamic_keys: Vec<_> = self.required_dynamic_tiles.iter().copied().collect();
        let mut changed_dynamic = Vec::new();
        for key in dynamic_keys {
            let mask = snapshot.dynamic_tile_mask(key.x, key.y);
            let tile = self.dynamic_tiles.entry(key).or_insert(DynamicTile {
                collider: None,
                mask: [0; 2],
                last_required_tick: self.terrain_tick,
            });
            tile.last_required_tick = self.terrain_tick;
            if tile.mask == mask {
                continue;
            }
            tile.mask = mask;
            changed_dynamic.push(key);
            self.terrain_statistics.dynamic_mask_changes += 1;
            self.terrain_statistics.dynamic_cells_scanned += 64;
            if mask == [0; 2] {
                if let Some(handle) = tile.collider {
                    self.rapier
                        .colliders
                        .get_mut(handle)
                        .unwrap()
                        .set_enabled(false);
                    self.terrain_statistics.dynamic_enable_disable_changes += 1;
                }
                continue;
            }
            let (shape, rectangles) = Self::dynamic_tile_shape(mask);
            self.terrain_statistics.dynamic_shape_rebuilds += 1;
            self.terrain_statistics.dynamic_rectangles_emitted += rectangles as u64;
            if let Some(handle) = tile.collider {
                let collider = self.rapier.colliders.get_mut(handle).unwrap();
                collider.set_shape(shape.unwrap());
                self.terrain_statistics.dynamic_set_shape_calls += 1;
                if !collider.is_enabled() {
                    collider.set_enabled(true);
                    self.terrain_statistics.dynamic_enable_disable_changes += 1;
                }
            } else {
                tile.collider = Some(
                    self.rapier.insert_collider(
                        ColliderBuilder::new(shape.unwrap())
                            .translation(Vector::new(key.x as f32, key.y as f32))
                            .friction(0.8)
                            .restitution(0.0)
                            .collision_groups(Self::dynamic_collision_groups())
                            .solver_groups(Self::dynamic_solver_groups()),
                        None,
                    ),
                );
            }
        }
        // A support tile can vanish underneath a sleeping body. Wake only bodies touching it.
        for key in changed_dynamic {
            let lo = Vector::new(key.x as f32, key.y as f32) - Vector::splat(0.125);
            let hi = lo + Vector::splat(1.25);
            for body in bodies {
                let Some(rigid) = self.rapier.bodies.get(body.handle) else {
                    continue;
                };
                if !rigid.is_sleeping() {
                    continue;
                }
                let touches = rigid.colliders().iter().any(|handle| {
                    self.rapier.colliders.get(*handle).is_some_and(|c| {
                        let a = c.compute_aabb();
                        a.mins.x <= hi.x && a.maxs.x >= lo.x && a.mins.y <= hi.y && a.maxs.y >= lo.y
                    })
                });
                if touches {
                    self.rapier
                        .bodies
                        .get_mut(body.handle)
                        .unwrap()
                        .wake_up(true);
                }
            }
        }
        let stale_dynamic: Vec<_> = self
            .dynamic_tiles
            .iter()
            .filter_map(|(key, tile)| {
                (self.terrain_tick - tile.last_required_tick > DYNAMIC_TILE_RETENTION_TICKS)
                    .then_some(*key)
            })
            .collect();
        for key in stale_dynamic {
            if let Some(tile) = self.dynamic_tiles.remove(&key) {
                if let Some(handle) = tile.collider {
                    self.rapier.remove_collider(handle);
                }
            }
        }
        self.terrain_statistics.dynamic_cached_tiles = self.dynamic_tiles.len();
        self.terrain_statistics.dynamic_collider_tiles = self
            .dynamic_tiles
            .values()
            .filter(|tile| {
                tile.collider
                    .is_some_and(|h| self.rapier.colliders.get(h).is_some_and(|c| c.is_enabled()))
            })
            .count();
        let keys: Vec<_> = self.required_terrain_patches.iter().copied().collect();
        for key in keys {
            let masks = snapshot.static_patch_masks(key.x, key.y);
            let changed = self
                .terrain_patches
                .get(&key)
                .is_none_or(|p| p.masks != masks);
            let patch = self.terrain_patches.entry(key).or_insert(TerrainPatch {
                collider: None,
                masks,
                last_required_tick: self.terrain_tick,
            });
            patch.last_required_tick = self.terrain_tick;
            if !changed {
                continue;
            }
            patch.masks = masks;
            self.terrain_statistics.patch_rebuilds += 1;
            self.terrain_statistics.patch_cells_scanned += 1024;
            match (patch.collider, Self::terrain_patch_shape(&masks)) {
                (Some(h), Some(s)) => self.rapier.colliders.get_mut(h).unwrap().set_shape(s),
                (Some(h), None) => {
                    self.rapier.remove_collider(h);
                    patch.collider = None;
                }
                (None, Some(s)) => {
                    patch.collider = Some(
                        self.rapier.insert_collider(
                            ColliderBuilder::new(s)
                                .translation(Vector::new(key.x as f32 * 4.0, key.y as f32 * 4.0))
                                .friction(0.8)
                                .restitution(0.0)
                                .collision_groups(Self::terrain_collision_groups())
                                .solver_groups(Self::terrain_solver_groups()),
                            None,
                        ),
                    )
                }
                (None, None) => {}
            }
        }
        let old: Vec<_> = self
            .terrain_patches
            .iter()
            .filter_map(|(k, p)| {
                (self.terrain_tick - p.last_required_tick > TERRAIN_PATCH_RETENTION_TICKS)
                    .then_some(*k)
            })
            .collect();
        for k in old {
            if let Some(p) = self.terrain_patches.remove(&k) {
                if let Some(h) = p.collider {
                    self.rapier.remove_collider(h);
                }
            }
        }
        self.terrain_statistics.active_patches = self.terrain_patches.len();
        self.terrain_statistics.collider_patches = self
            .terrain_patches
            .values()
            .filter(|p| p.collider.is_some())
            .count();
        #[cfg(debug_assertions)]
        tracing::trace!(
            elapsed_us = started.elapsed().as_micros(),
            awake_rigid = bodies
                .iter()
                .filter(|body| self
                    .rapier
                    .bodies
                    .get(body.handle)
                    .is_some_and(|body| !body.is_sleeping()))
                .count(),
            required_dynamic_tiles = self.required_dynamic_tiles.len(),
            required_terrain_patches = self.required_terrain_patches.len(),
            "cellular terrain preparation"
        );
    }

    pub(crate) fn terrain_patch_shape(masks: &[[u32; 2]; 16]) -> Option<SharedShape> {
        let mut rows = [0u32; 32];
        for ty in 0..4 {
            for tx in 0..4 {
                let [lo, hi] = masks[ty * 4 + tx];
                for y in 0..8 {
                    rows[ty * 8 + y] |=
                        ((if y < 4 { lo } else { hi }) >> ((y % 4) * 8) & 0xff) << (tx * 8);
                }
            }
        }
        Self::shape_from_rows(&mut rows, 32).0
    }

    pub(crate) fn dynamic_tile_shape(mask: [u32; 2]) -> (Option<SharedShape>, usize) {
        let mut rows = [0u32; 8];
        for (y, row) in rows.iter_mut().enumerate() {
            *row = (mask[y / 4] >> ((y % 4) * 8)) & 0xff;
        }
        Self::shape_from_rows(&mut rows, 8)
    }

    fn shape_from_rows(rows: &mut [u32], size: usize) -> (Option<SharedShape>, usize) {
        if rows.iter().all(|r| *r == 0) {
            return (None, 0);
        }
        let mut parts = Vec::with_capacity(size);
        for y in 0..size {
            while rows[y] != 0 {
                let x = rows[y].trailing_zeros() as usize;
                let w = (rows[y] >> x).trailing_ones() as usize;
                let mask = if w == 32 {
                    u32::MAX
                } else {
                    (((1u64 << w) - 1) as u32) << x
                };
                let mut h = 1;
                while y + h < size && rows[y + h] & mask == mask {
                    h += 1;
                }
                for row in &mut rows[y..y + h] {
                    *row &= !mask;
                }
                parts.push((
                    Pose::translation(
                        (x + w / 2) as f32 / 8.0 + (w % 2) as f32 / 16.0,
                        (y + h / 2) as f32 / 8.0 + (h % 2) as f32 / 16.0,
                    ),
                    SharedShape::cuboid(w as f32 / 16.0, h as f32 / 16.0),
                ));
            }
        }
        let count = parts.len();
        (Some(SharedShape::compound(parts)), count)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn terrain_bridge_statistics(&self) -> TerrainBridgeStatistics {
        self.terrain_statistics
    }

    /// Advances Rapier's collision world by one fixed scene step
    pub fn step(&mut self, gravity: [f32; 2], delta_time: f32) {
        #[cfg(debug_assertions)]
        let started = Instant::now();
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
            elapsed_us = started.elapsed().as_micros(),
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
        let effective = state.inverse_mass
            * (constraint[0] * constraint[0] + constraint[1] * constraint[1])
            + state.inverse_angular_inertia * constraint[2] * constraint[2];
        if effective <= 1e-12 {
            return self.apply_rigid_cellular_body_reaction(body, [0.0; 2], 0.0, 0.0, wake);
        }
        // The Accelerator impulse defines a velocity target along its generalized contact direction.
        // Re-evaluate that target against current motion instead of replaying stale stopping work.
        let target = source[0] * constraint[0]
            + source[1] * constraint[1]
            + source[2] * constraint[2]
            + effective;
        let target = if source[3] == 0.0 {
            target.max(0.0)
        } else {
            target
        };
        let current = state.linear_velocity[0] * constraint[0]
            + state.linear_velocity[1] * constraint[1]
            + state.angular_velocity * constraint[2];
        let scale = ((target - current) / effective).max(0.0);
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
        let impulse = Vector::new(support[0], support[1]);
        let quadratic = 0.5
            * (rigid.mass_properties().local_mprops.inv_mass * impulse.length_squared()
                + rigid.mass_properties().effective_world_inv_inertia * support[2] * support[2]);
        let linear = rigid.linvel().dot(impulse) + rigid.angvel() * support[2];
        let budget = support[3].max(0.0);
        let scale = if linear + quadratic <= budget {
            1.0
        } else if quadratic > 0.0 {
            ((linear * linear + 4.0 * quadratic * budget).sqrt() - linear) / (2.0 * quadratic)
        } else {
            0.0
        }
        .clamp(0.0, 1.0);
        let force = impulse * (60.0 * scale);
        let torque = support[2] * (60.0 * scale);
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
        let Some(before) = self.rigid_cellular_body_state(body) else {
            return false;
        };
        self.apply_rigid_cellular_body_reaction(
            body,
            [recovery[0], recovery[1]],
            recovery[2],
            recovery[3],
            false,
        );
        let after = self.rigid_cellular_body_state(body).unwrap();
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
            .get(&DynamicTileKey {
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
