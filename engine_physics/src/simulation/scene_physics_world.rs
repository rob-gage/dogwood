// Copyright Rob Gage 2026

use super::{CollisionOccupancySnapshot, RigidCellularBody, RigidCellularBodyState};
use crate::actors::{Actor, ActorCellularProxyState, ActorCollisionShape};
use crate::materials::MaterialRegistry;
use rapier2d::parry::query::ShapeCastOptions;
use rapier2d::prelude::{
    ColliderBuilder, ColliderHandle, Group, InteractionGroups, LockedAxes, PhysicsWorld, Pose,
    QueryFilter, RigidBodyBuilder, RigidBodyHandle, SharedShape, Vector,
};
use std::collections::{HashMap, HashSet};
#[cfg(debug_assertions)]
use std::time::Instant;

const TERRAIN_COLLISION_PATCH_TILES: i32 = 4;
const TERRAIN_COLLISION_PATCH_CELLS: i32 = TERRAIN_COLLISION_PATCH_TILES * 8;
const TERRAIN_PATCH_RETENTION_TICKS: u64 = 120;
const DYNAMIC_TILE_RETENTION_TICKS: u64 = 30;
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct TerrainPatchKey {
    x: i32,
    y: i32,
}
struct TerrainPatch {
    collider: Option<ColliderHandle>,
    masks: [[u32; 2]; 16],
    last_required_tick: u64,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct DynamicTileKey {
    x: i32,
    y: i32,
}
struct DynamicTile {
    collider: Option<ColliderHandle>,
    mask: [u32; 2],
    last_required_tick: u64,
}
struct ActorPhysicsProxy {
    body: RigidBodyHandle,
    collider: ColliderHandle,
    shape: ActorCollisionShape,
}
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TerrainBridgeStatistics {
    pub(crate) active_patches: usize,
    pub(crate) collider_patches: usize,
    pub(crate) patch_rebuilds: u64,
    pub(crate) patch_cells_scanned: u64,
    pub(crate) dynamic_required_tiles: usize,
    pub(crate) dynamic_cached_tiles: usize,
    pub(crate) dynamic_collider_tiles: usize,
    pub(crate) dynamic_mask_changes: u64,
    pub(crate) dynamic_shape_rebuilds: u64,
    pub(crate) dynamic_set_shape_calls: u64,
    pub(crate) dynamic_enable_disable_changes: u64,
    pub(crate) dynamic_cells_scanned: u64,
    pub(crate) dynamic_rectangles_emitted: u64,
    pub(crate) collision_snapshot_age: u64,
}

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
        cells: Vec<super::rigid_cellular_body::RigidCellularBodyCell>,
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
            "Rigid cellular GPU contact requires unlocked translation axes"
        );
        let position = rigid_body.position();
        let center = rigid_body.center_of_mass();
        Some(RigidCellularBodyState {
            translation: [position.translation.x, position.translation.y],
            angle: position.rotation.angle(),
            linear_velocity: [rigid_body.linvel().x, rigid_body.linvel().y],
            angular_velocity: rigid_body.angvel(),
            center_of_mass: [center.x, center.y],
            inverse_mass: rigid_body.mass_properties().local_mprops.inv_mass,
            inverse_angular_inertia: rigid_body.mass_properties().effective_world_inv_inertia,
        })
    }

    /// Removes one rigid body and all attached colliders
    #[allow(dead_code)]
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

    fn pawn_collider_at(
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

    fn terrain_patch_shape(masks: &[[u32; 2]; 16]) -> Option<SharedShape> {
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

    fn dynamic_tile_shape(mask: [u32; 2]) -> (Option<SharedShape>, usize) {
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

    /// Applies one already-integrated GPU impulse batch to its authoritative body
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
        // The GPU impulse defines a velocity target along its generalized contact direction.
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

    /// Applies one already-integrated GPU impulse batch to its authoritative body
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

    fn rigid_solver_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_1)
            .with_filter(Group::ALL)
    }
    fn rigid_collision_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_1)
            .with_filter(Group::ALL)
    }
    fn terrain_collision_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_3)
            .with_filter(Group::GROUP_1)
    }
    fn terrain_solver_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_3)
            .with_filter(Group::GROUP_1)
    }
    fn dynamic_collision_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_4)
            .with_filter(Group::GROUP_1)
    }
    fn dynamic_solver_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_4)
            .with_filter(Group::GROUP_1)
    }
    fn pawn_collision_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_2)
            .with_filter(Group::GROUP_1)
    }
    fn pawn_solver_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_2)
            .with_filter(Group::GROUP_1)
    }
    fn pawn_query_groups() -> InteractionGroups {
        InteractionGroups::all().with_memberships(Group::GROUP_1 | Group::GROUP_3 | Group::GROUP_4)
    }

    /// Resolves authoritative actor motion through Rapier terrain and rigid-body queries.
    pub(crate) fn move_actor(
        &self,
        shape: ActorCollisionShape,
        position: Vector,
        desired: Vector,
        up: Vector,
        walkable_normal: f32,
        snap_distance: f32,
        collisions: &mut impl FnMut(Vector),
    ) -> (Vector, bool) {
        let exclude = self.pawn_collider_at(shape, position);
        self.move_actor_excluding(
            shape,
            position,
            desired,
            up,
            walkable_normal,
            snap_distance,
            exclude,
            collisions,
        )
    }

    pub(crate) fn move_actor_excluding(
        &self,
        shape: ActorCollisionShape,
        position: Vector,
        desired: Vector,
        up: Vector,
        walkable_normal: f32,
        snap_distance: f32,
        exclude: Option<ColliderHandle>,
        collisions: &mut impl FnMut(Vector),
    ) -> (Vector, bool) {
        let (mut translation, mut grounded) = self.resolve_actor_translation(
            shape,
            position,
            desired,
            up,
            walkable_normal,
            exclude,
            collisions,
        );
        if snap_distance > 0.0 && desired.dot(up) <= 0.0 && !grounded {
            let (snap, snapped) = self.resolve_actor_support_excluding(
                shape,
                position + translation,
                snap_distance,
                up,
                exclude,
            );
            if snapped {
                translation += snap;
                grounded = true;
            }
        }
        (translation, grounded)
    }

    fn resolve_actor_translation(
        &self,
        shape: ActorCollisionShape,
        position: Vector,
        desired: Vector,
        up: Vector,
        walkable_normal: f32,
        exclude: Option<ColliderHandle>,
        collisions: &mut impl FnMut(Vector),
    ) -> (Vector, bool) {
        #[cfg(debug_assertions)]
        let started = Instant::now();
        let offset = 1.0 / 1024.0;
        let primitive = shape.rapier_shape();
        let mut consumed = Vector::ZERO;
        let mut remaining = desired;
        let mut grounded = false;
        for _ in 0..4 {
            if remaining.length_squared() < 1e-12 {
                break;
            }
            let from = position + consumed;
            let moving_pose = shape.pose(from, up);
            let options = ShapeCastOptions {
                max_time_of_impact: 1.0,
                target_distance: offset,
                stop_at_penetration: false,
                compute_impact_geometry_on_penetration: true,
            };
            let mut earliest = 2.0f32;
            let mut normals = [Vector::ZERO; 4];
            let mut normal_count = 0usize;
            if let Some((_handle, hit)) = self.rapier.cast_shape(
                &moving_pose,
                remaining,
                primitive.as_ref(),
                options,
                exclude.map_or_else(
                    || QueryFilter::default().groups(Self::pawn_query_groups()),
                    |h| {
                        QueryFilter::default()
                            .groups(Self::pawn_query_groups())
                            .exclude_collider(h)
                    },
                ),
            ) {
                if hit.time_of_impact + 1e-4 < earliest {
                    earliest = hit.time_of_impact;
                    normal_count = 0;
                }
                if (hit.time_of_impact - earliest).abs() <= 1e-4
                    && normal_count < normals.len()
                    && !normals[..normal_count]
                        .iter()
                        .any(|current| current.dot(hit.normal1) > 0.999)
                {
                    normals[normal_count] = hit.normal1;
                    normal_count += 1;
                }
            }
            if earliest > 1.0 {
                consumed += remaining;
                break;
            }
            let advance = remaining * earliest.max(0.0);
            consumed += advance;
            remaining -= advance;
            let mut active_normals = 0usize;
            for index in 0..normal_count {
                let normal = normals[index];
                active_normals += 1;
                collisions(normal);
                grounded |= normal.dot(up) >= walkable_normal;
                let inward = remaining.dot(normal);
                if inward < 0.0 {
                    remaining -= normal * inward;
                }
            }
            if active_normals == 0 {
                consumed += remaining;
                break;
            }
        }
        #[cfg(debug_assertions)]
        tracing::trace!(
            elapsed_us = started.elapsed().as_micros(),
            "pawn rigid/terrain shape casts"
        );
        (consumed, grounded)
    }

    /// Casts only along gravity-relative down without allowing a support correction to slide.
    pub(crate) fn resolve_actor_support(
        &self,
        shape: ActorCollisionShape,
        position: Vector,
        distance: f32,
        up: Vector,
    ) -> (Vector, bool) {
        self.resolve_actor_support_excluding(shape, position, distance, up, None)
    }

    fn resolve_actor_support_excluding(
        &self,
        shape: ActorCollisionShape,
        position: Vector,
        distance: f32,
        up: Vector,
        exclude: Option<ColliderHandle>,
    ) -> (Vector, bool) {
        #[cfg(debug_assertions)]
        let started = Instant::now();
        if !distance.is_finite() || distance <= 0.0 {
            return (Vector::ZERO, false);
        }
        let offset = 1.0 / 1024.0;
        let desired = -up * distance;
        let primitive = shape.rapier_shape();
        let moving_pose = shape.pose(position, up);
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            target_distance: offset,
            stop_at_penetration: false,
            compute_impact_geometry_on_penetration: true,
        };
        let mut earliest = 1.0f32;
        let mut support = false;
        if let Some((_handle, hit)) = self.rapier.cast_shape(
            &moving_pose,
            desired,
            primitive.as_ref(),
            options,
            exclude.map_or_else(
                || QueryFilter::default().groups(Self::pawn_query_groups()),
                |h| {
                    QueryFilter::default()
                        .groups(Self::pawn_query_groups())
                        .exclude_collider(h)
                },
            ),
        ) {
            if hit.normal1.dot(up) > 1e-4 && hit.time_of_impact <= earliest {
                earliest = hit.time_of_impact;
                support = true;
            }
        }
        #[cfg(debug_assertions)]
        tracing::trace!(
            elapsed_us = started.elapsed().as_micros(),
            "pawn support shape cast"
        );
        (-up * (distance * earliest), support)
    }
}

#[cfg(test)]
mod tests {
    use super::ScenePhysicsWorld;
    use crate::{
        actors::{ActorCellularProxyState, ActorCollisionShape},
        simulation::CollisionOccupancySnapshot,
        tiles::TileCoordinates,
    };
    use crate::{
        materials::{Material, MaterialRegistry},
        tiles::CellularAppearance,
    };
    use engine_graphics::{Color, MaterialAppearance};
    use rapier2d::prelude::{Pose, Vector};

    fn assert_patch_bounds(masks: [[u32; 2]; 16], origin: [f32; 2], expected: [f32; 4]) {
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

    #[test]
    fn full_and_almost_full_patch_keep_the_same_exterior_bounds() {
        let full = [[u32::MAX; 2]; 16];
        assert_patch_bounds(full, [0.0, -4.0], [0.0, -4.0, 4.0, 0.0]);
        assert_patch_bounds(full, [-8.0, 12.0], [-8.0, 12.0, -4.0, 16.0]);
        let mut hole = full;
        hole[5][0] &= !(1 << 9); // Interior cell; exterior must not move.
        assert_patch_bounds(hole, [0.0, -4.0], [0.0, -4.0, 4.0, 0.0]);
    }

    #[test]
    fn generated_full_floor_and_one_pixel_edit_have_the_same_rest_height() {
        let mut materials = MaterialRegistry::new();
        let stone = materials.register(Material::CellularStatic {
            name: "Stone".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(90, 90, 90)),
            mass: 1.0,
            pressure_ignore_threshold: 1.0,
            default_integrity: 1.0,
            minimum_rigid_body_cell_count: 1,
            debris_material: None,
            debris_yield_rate: 0.0,
            pressure_transmission: 1.0,
            friction: 0.5,
            restitution: 0.0,
        });
        let mut world = ScenePhysicsWorld::new();
        let snapshot = |hole: bool| {
            let mut masks = vec![[u32::MAX; 2]; 16];
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
        let body = world.insert_rigid_cellular_body(
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
            world.prepare_cellular_terrain(
                std::slice::from_ref(&body),
                &[],
                [0.0, -9.81],
                1.0 / 60.0,
            );
            world.step([0.0, -9.81], 1.0 / 60.0);
        }
        let full_height = world.rigid_cellular_body_state(&body).unwrap().translation[1];
        world.update_cellular_snapshot(snapshot(true));
        for _ in 0..60 {
            world.prepare_cellular_terrain(
                std::slice::from_ref(&body),
                &[],
                [0.0, -9.81],
                1.0 / 60.0,
            );
            world.step([0.0, -9.81], 1.0 / 60.0);
        }
        let edited_height = world.rigid_cellular_body_state(&body).unwrap().translation[1];
        assert!(
            full_height > -0.01 && (full_height - edited_height).abs() < 0.01,
            "full={full_height}, edited={edited_height}"
        );
    }

    #[test]
    fn stale_rigid_reaction_cannot_create_energy_without_grid_transfer() {
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
        let mut physics = ScenePhysicsWorld::new();
        let body = physics.insert_rigid_cellular_body(
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
        let state = physics.rigid_cellular_body_state(&body).unwrap();
        assert!(state.linear_velocity[0].abs() < 0.0001);
        assert!(physics.apply_rigid_cellular_body_reaction(&body, [1.0, 0.0], 0.0, 0.5, true));
        physics.step([0.0, 0.0], 1.0 / 60.0);
        let state = physics.rigid_cellular_body_state(&body).unwrap();
        assert!(state.linear_velocity[0] > 0.0);
    }

    #[test]
    fn rigid_cellular_bodies_still_collide_through_rapier() {
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
        let cell = || {
            vec![crate::simulation::RigidCellularBodyCell::test_cell(
                [0, 0],
                stone,
                CellularAppearance::NEUTRAL,
            )]
        };
        let mut physics = ScenePhysicsWorld::new();
        let left = physics.insert_rigid_cellular_body(
            [0.0, 0.0],
            0.0,
            &materials,
            cell(),
            0.5,
            0.0,
            [1.0, 0.0],
            0.0,
        );
        let right = physics.insert_rigid_cellular_body(
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
        let left_x = physics
            .rigid_cellular_body_state(&left)
            .unwrap()
            .translation[0];
        let right_x = physics
            .rigid_cellular_body_state(&right)
            .unwrap()
            .translation[0];
        assert!(
            left_x + 0.12 <= right_x,
            "rigid bodies interpenetrated: {left_x}, {right_x}"
        );
    }

    fn prepare_actor_terrain(physics: &mut ScenePhysicsWorld, shape: ActorCollisionShape) {
        let actor = ActorCellularProxyState {
            center: [0.0, 0.5625],
            velocity: [0.0; 2],
            drive: [0.0; 2],
            shape,
            occupancy_kind: 1,
            mass: 0.0,
        };
        physics.prepare_cellular_terrain(&[], &[actor], [0.0; 2], 1.0 / 60.0);
        physics.step([0.0; 2], 1.0 / 60.0);
    }

    #[test]
    fn every_actor_primitive_casts_against_static_terrain_patches() {
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
    fn dynamic_cells_create_hard_pawn_collision() {
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
    fn dynamic_tile_geometry_and_cache_are_local() {
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
        let key = super::DynamicTileKey { x: 0, y: 0 };
        let handle = world.dynamic_tiles[&key].collider.unwrap();
        world.update_cellular_snapshot(snapshot([0; 2], [1, 0]));
        world.prepare_cellular_terrain(&[], &actors, [0.0; 2], 1.0 / 60.0);
        assert!(!world.rapier.colliders.get(handle).unwrap().is_enabled());
        world.update_cellular_snapshot(snapshot([1, 0], [1, 0]));
        world.prepare_cellular_terrain(&[], &actors, [0.0; 2], 1.0 / 60.0);
        assert_eq!(world.dynamic_tiles[&key].collider, Some(handle));
        assert!(world.rapier.colliders.get(handle).unwrap().is_enabled());
    }

    #[test]
    fn settled_dynamic_floor_supports_rigid_and_removed_floor_releases_it() {
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
            world.prepare_cellular_terrain(
                std::slice::from_ref(&body),
                &[],
                [0.0, -9.81],
                1.0 / 60.0,
            );
            world.step([0.0, -9.81], 1.0 / 60.0);
        }
        let supported = world.rigid_cellular_body_state(&body).unwrap().translation[1];
        assert!(supported > 0.95, "rigid sank through sand: {supported}");
        for _ in 0..60 {
            world
                .rapier
                .bodies
                .get_mut(body.handle)
                .unwrap()
                .apply_impulse(Vector::new(0.0, -0.001), true);
            world.prepare_cellular_terrain(
                std::slice::from_ref(&body),
                &[],
                [0.0, -9.81],
                1.0 / 60.0,
            );
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
            world.prepare_cellular_terrain(
                std::slice::from_ref(&body),
                &[],
                [0.0, -9.81],
                1.0 / 60.0,
            );
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
            world.prepare_cellular_terrain(
                std::slice::from_ref(&body),
                &[],
                [0.0, -9.81],
                1.0 / 60.0,
            );
            world.step([0.0, -9.81], 1.0 / 60.0);
        }
        assert_eq!(
            world.terrain_bridge_statistics().dynamic_shape_rebuilds,
            rebuilds
        );
        world.rapier.bodies.get_mut(body.handle).unwrap().sleep();
        world.update_cellular_snapshot(snapshot([0; 2]));
        world.prepare_cellular_terrain(std::slice::from_ref(&body), &[], [0.0, -9.81], 1.0 / 60.0);
        assert!(!world.rapier.bodies.get(body.handle).unwrap().is_sleeping());
        for _ in 0..30 {
            world.step([0.0, -9.81], 1.0 / 60.0);
        }
        assert!(world.rigid_cellular_body_state(&body).unwrap().translation[1] < supported - 0.1);
    }

    #[test]
    fn distant_unsettled_sand_needs_no_rapier_geometry() {
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
    fn dynamic_group_filters_and_neighbor_tiles_have_no_gap() {
        let rigid = ScenePhysicsWorld::rigid_collision_groups();
        let dynamic = ScenePhysicsWorld::dynamic_collision_groups();
        let static_terrain = ScenePhysicsWorld::terrain_collision_groups();
        assert!(rigid.test(dynamic));
        assert!(rigid.test(ScenePhysicsWorld::pawn_solver_groups()));
        assert!(
            !ScenePhysicsWorld::pawn_solver_groups()
                .test(ScenePhysicsWorld::terrain_solver_groups())
        );
        assert!(
            !ScenePhysicsWorld::pawn_solver_groups()
                .test(ScenePhysicsWorld::dynamic_solver_groups())
        );
        assert!(
            ScenePhysicsWorld::rigid_solver_groups()
                .test(ScenePhysicsWorld::dynamic_solver_groups())
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
    fn pawn_is_supported_by_settled_dynamic_sand() {
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
    fn distributed_rigids_demand_local_dynamic_tiles() {
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
    fn moving_sand_shape_work_stays_near_pawn() {
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
            max_required =
                max_required.max(world.terrain_bridge_statistics().dynamic_required_tiles);
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
}
