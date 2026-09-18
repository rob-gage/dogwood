// Copyright Rob Gage 2026

use std::collections::HashSet;
#[cfg(debug_assertions)]
use std::time::Instant;

use rapier2d::prelude::ColliderBuilder;
use rapier2d::prelude::Pose;
use rapier2d::prelude::SharedShape;
use rapier2d::prelude::Vector;

use super::super::dynamic_tile::RigidDynamicCollisionTile;
use super::super::dynamic_tile_key::RigidDynamicCollisionTileKey;
use super::super::terrain_bridge_statistics::TerrainBridgeStatistics;
use super::super::terrain_patch::StaticTerrainCollisionPatch;
use super::super::terrain_patch_key::StaticTerrainCollisionPatchKey;
use super::ScenePhysicsWorld;
use crate::actors::ActorCellularProxyState;
use crate::actors_utility::ActorPhysicalProxyState;
use crate::simulation::RigidCellularBody;
use crate::simulation::simulation_constants::DYNAMIC_TILE_RETENTION_TICKS;
use crate::simulation::simulation_constants::TERRAIN_COLLISION_PATCH_CELLS;
use crate::simulation::simulation_constants::TERRAIN_PATCH_RETENTION_TICKS;

impl ScenePhysicsWorld {
    fn demand_dynamic_tiles(
        required: &mut HashSet<RigidDynamicCollisionTileKey>,
        lo: Vector,
        hi: Vector,
    ) {
        for tile_y in (lo.y.floor() as i32 - 1)..=(hi.y.floor() as i32 + 1) {
            for tile_x in (lo.x.floor() as i32 - 1)..=(hi.x.floor() as i32 + 1) {
                required.insert(RigidDynamicCollisionTileKey {
                    x: tile_x,
                    y: tile_y,
                });
            }
        }
    }

    fn demand_actor_terrain(
        required_dynamic_tiles: &mut HashSet<RigidDynamicCollisionTileKey>,
        required_terrain_patches: &mut HashSet<StaticTerrainCollisionPatchKey>,
        center: [f32; 2],
        velocity: [f32; 2],
        shape: crate::actors::ActorCollisionShape,
        gravity: [f32; 2],
        delta_time: f32,
    ) {
        let actor_center: Vector = Vector::new(center[0], center[1]);
        let actor_radius: f32 = shape
            .nominal_dimensions()
            .into_iter()
            .fold(0.0f32, f32::max)
            * 0.5;
        let actor_displacement: Vector = Vector::new(velocity[0], velocity[1]) * delta_time
            + Vector::new(gravity[0], gravity[1]) * (0.5 * delta_time * delta_time);
        let minimum: Vector = actor_center.min(actor_center + actor_displacement)
            - Vector::splat(actor_radius + 0.25);
        let maximum: Vector = actor_center.max(actor_center + actor_displacement)
            + Vector::splat(actor_radius + 0.25);
        Self::demand_dynamic_tiles(required_dynamic_tiles, minimum, maximum);
        let patch_x_minimum: i32 =
            ((minimum.x * 8.0).floor() as i32).div_euclid(TERRAIN_COLLISION_PATCH_CELLS) - 1;
        let patch_y_minimum: i32 =
            ((minimum.y * 8.0).floor() as i32).div_euclid(TERRAIN_COLLISION_PATCH_CELLS) - 1;
        let patch_x_maximum: i32 =
            ((maximum.x * 8.0).floor() as i32).div_euclid(TERRAIN_COLLISION_PATCH_CELLS) + 1;
        let patch_y_maximum: i32 =
            ((maximum.y * 8.0).floor() as i32).div_euclid(TERRAIN_COLLISION_PATCH_CELLS) + 1;
        for patch_y in patch_y_minimum..=patch_y_maximum {
            for patch_x in patch_x_minimum..=patch_x_maximum {
                required_terrain_patches.insert(StaticTerrainCollisionPatchKey {
                    x: patch_x,
                    y: patch_y,
                });
            }
        }
    }

    pub(crate) fn prepare_cellular_terrain(
        &mut self,
        bodies: &[RigidCellularBody],
        actors: &[ActorCellularProxyState],
        gravity: [f32; 2],
        delta_time: f32,
    ) {
        self.prepare_cellular_terrain_with_physical(bodies, actors, &[], gravity, delta_time);
    }

    pub(crate) fn prepare_cellular_terrain_with_physical(
        &mut self,
        bodies: &[RigidCellularBody],
        actors: &[ActorCellularProxyState],
        physical_actors: &[ActorPhysicalProxyState],
        gravity: [f32; 2],
        delta_time: f32,
    ) {
        #[cfg(debug_assertions)]
        let started: Instant = Instant::now();
        self.terrain_tick += 1;
        if !self.snapshot_updated_this_tick {
            self.terrain_statistics.collision_snapshot_age += 1;
        }
        self.snapshot_updated_this_tick = false;
        self.required_terrain_patches.clear();
        self.required_dynamic_tiles.clear();
        for rigid_cellular_body in bodies {
            let Some(rigid_body) = self.rapier.bodies.get(rigid_cellular_body.handle) else {
                continue;
            };
            for collider_handle in rigid_body.colliders() {
                let Some(collider) = self.rapier.colliders.get(*collider_handle) else {
                    continue;
                };
                let aabb: rapier2d::parry::bounding_volume::Aabb = collider.compute_aabb();
                let collision_radius: f32 = (aabb.maxs - aabb.mins).length() * 0.5;
                let displacement: Vector = rigid_body.linvel() * delta_time
                    + Vector::new(gravity[0], gravity[1]) * (0.5 * delta_time * delta_time);
                let angular_displacement: f32 =
                    rigid_body.angvel().abs() * delta_time * collision_radius;
                let minimum: Vector = aabb.mins.min(aabb.mins + displacement)
                    - Vector::splat(angular_displacement + 0.25);
                let maximum: Vector = aabb.maxs.max(aabb.maxs + displacement)
                    + Vector::splat(angular_displacement + 0.25);
                Self::demand_dynamic_tiles(&mut self.required_dynamic_tiles, minimum, maximum);
                let patch_x_minimum: i32 = ((minimum.x * 8.0).floor() as i32)
                    .div_euclid(TERRAIN_COLLISION_PATCH_CELLS)
                    - 1;
                let patch_y_minimum: i32 = ((minimum.y * 8.0).floor() as i32)
                    .div_euclid(TERRAIN_COLLISION_PATCH_CELLS)
                    - 1;
                let patch_x_maximum: i32 = ((maximum.x * 8.0).floor() as i32)
                    .div_euclid(TERRAIN_COLLISION_PATCH_CELLS)
                    + 1;
                let patch_y_maximum: i32 = ((maximum.y * 8.0).floor() as i32)
                    .div_euclid(TERRAIN_COLLISION_PATCH_CELLS)
                    + 1;
                for patch_y in patch_y_minimum..=patch_y_maximum {
                    for patch_x in patch_x_minimum..=patch_x_maximum {
                        self.required_terrain_patches
                            .insert(StaticTerrainCollisionPatchKey {
                                x: patch_x,
                                y: patch_y,
                            });
                    }
                }
            }
        }
        for actor in actors {
            Self::demand_actor_terrain(
                &mut self.required_dynamic_tiles,
                &mut self.required_terrain_patches,
                actor.center,
                actor.velocity,
                actor.shape,
                gravity,
                delta_time,
            );
        }
        for actor in physical_actors {
            Self::demand_actor_terrain(
                &mut self.required_dynamic_tiles,
                &mut self.required_terrain_patches,
                actor.center,
                actor.velocity,
                actor.shape,
                gravity,
                delta_time,
            );
        }
        let Some(snapshot) = self.cellular_terrain_snapshot.as_ref() else {
            return;
        };
        self.terrain_statistics.dynamic_required_tiles = self.required_dynamic_tiles.len();
        let dynamic_keys: Vec<RigidDynamicCollisionTileKey> =
            self.required_dynamic_tiles.iter().copied().collect();
        let mut changed_dynamic: Vec<RigidDynamicCollisionTileKey> = Vec::new();
        for dynamic_tile_key in dynamic_keys {
            let dynamic_tile_mask: [u32; 2] =
                snapshot.dynamic_tile_mask(dynamic_tile_key.x, dynamic_tile_key.y);
            let dynamic_tile: &mut RigidDynamicCollisionTile = self
                .dynamic_tiles
                .entry(dynamic_tile_key)
                .or_insert(RigidDynamicCollisionTile {
                    collider: None,
                    mask: [0; 2],
                    last_required_tick: self.terrain_tick,
                });
            dynamic_tile.last_required_tick = self.terrain_tick;
            if dynamic_tile.mask == dynamic_tile_mask {
                continue;
            }
            dynamic_tile.mask = dynamic_tile_mask;
            changed_dynamic.push(dynamic_tile_key);
            self.terrain_statistics.dynamic_mask_changes += 1;
            self.terrain_statistics.dynamic_cells_scanned += 64;
            if dynamic_tile_mask == [0; 2] {
                if let Some(collider_handle) = dynamic_tile.collider {
                    self.rapier
                        .colliders
                        .get_mut(collider_handle)
                        .unwrap()
                        .set_enabled(false);
                    self.terrain_statistics.dynamic_enable_disable_changes += 1;
                }
                continue;
            }
            let (shape, rectangles): (Option<SharedShape>, usize) =
                Self::dynamic_tile_shape(dynamic_tile_mask);
            self.terrain_statistics.dynamic_shape_rebuilds += 1;
            self.terrain_statistics.dynamic_rectangles_emitted += rectangles as u64;
            if let Some(collider_handle) = dynamic_tile.collider {
                let collider: &mut rapier2d::prelude::Collider =
                    self.rapier.colliders.get_mut(collider_handle).unwrap();
                collider.set_shape(shape.unwrap());
                self.terrain_statistics.dynamic_set_shape_calls += 1;
                if !collider.is_enabled() {
                    collider.set_enabled(true);
                    self.terrain_statistics.dynamic_enable_disable_changes += 1;
                }
            } else {
                dynamic_tile.collider = Some(
                    self.rapier.insert_collider(
                        ColliderBuilder::new(shape.unwrap())
                            .translation(Vector::new(
                                dynamic_tile_key.x as f32,
                                dynamic_tile_key.y as f32,
                            ))
                            .friction(0.8)
                            .restitution(0.0)
                            .collision_groups(Self::dynamic_collision_groups())
                            .solver_groups(Self::dynamic_solver_groups()),
                        None,
                    ),
                );
            }
        }
        // a support tile can vanish underneath a sleeping body. Wake only bodies touching it.
        for dynamic_tile_key in changed_dynamic {
            let minimum: Vector = Vector::new(dynamic_tile_key.x as f32, dynamic_tile_key.y as f32)
                - Vector::splat(0.125);
            let maximum: Vector = minimum + Vector::splat(1.25);
            for rigid_cellular_body in bodies {
                let Some(rigid_body) = self.rapier.bodies.get(rigid_cellular_body.handle) else {
                    continue;
                };
                if !rigid_body.is_sleeping() {
                    continue;
                }
                let touches: bool = rigid_body.colliders().iter().any(|collider_handle| {
                    self.rapier
                        .colliders
                        .get(*collider_handle)
                        .is_some_and(|collider| {
                            let collider_aabb: rapier2d::parry::bounding_volume::Aabb =
                                collider.compute_aabb();
                            collider_aabb.mins.x <= maximum.x
                                && collider_aabb.maxs.x >= minimum.x
                                && collider_aabb.mins.y <= maximum.y
                                && collider_aabb.maxs.y >= minimum.y
                        })
                });
                if touches {
                    self.rapier
                        .bodies
                        .get_mut(rigid_cellular_body.handle)
                        .unwrap()
                        .wake_up(true);
                }
            }
        }
        let stale_dynamic: Vec<RigidDynamicCollisionTileKey> = self
            .dynamic_tiles
            .iter()
            .filter_map(|(key, tile)| {
                (self.terrain_tick - tile.last_required_tick > DYNAMIC_TILE_RETENTION_TICKS)
                    .then_some(*key)
            })
            .collect();
        for dynamic_tile_key in stale_dynamic {
            if let Some(dynamic_tile) = self.dynamic_tiles.remove(&dynamic_tile_key)
                && let Some(collider_handle) = dynamic_tile.collider
            {
                self.rapier.remove_collider(collider_handle);
            }
        }
        self.terrain_statistics.dynamic_cached_tiles = self.dynamic_tiles.len();
        self.terrain_statistics.dynamic_collider_tiles = self
            .dynamic_tiles
            .values()
            .filter(|dynamic_tile| {
                dynamic_tile.collider.is_some_and(|collider_handle| {
                    self.rapier
                        .colliders
                        .get(collider_handle)
                        .is_some_and(|collider| collider.is_enabled())
                })
            })
            .count();
        let terrain_patch_keys: Vec<StaticTerrainCollisionPatchKey> =
            self.required_terrain_patches.iter().copied().collect();
        for terrain_patch_key in terrain_patch_keys {
            let terrain_patch_masks: [[u32; 2]; 16] =
                snapshot.static_patch_masks(terrain_patch_key.x, terrain_patch_key.y);
            let changed: bool = self
                .terrain_patches
                .get(&terrain_patch_key)
                .is_none_or(|terrain_patch| terrain_patch.masks != terrain_patch_masks);
            let terrain_patch: &mut StaticTerrainCollisionPatch = self
                .terrain_patches
                .entry(terrain_patch_key)
                .or_insert(StaticTerrainCollisionPatch {
                    collider: None,
                    masks: terrain_patch_masks,
                    last_required_tick: self.terrain_tick,
                });
            terrain_patch.last_required_tick = self.terrain_tick;
            if !changed {
                continue;
            }
            terrain_patch.masks = terrain_patch_masks;
            self.terrain_statistics.patch_rebuilds += 1;
            self.terrain_statistics.patch_cells_scanned += 1024;
            match (
                terrain_patch.collider,
                Self::terrain_patch_shape(&terrain_patch_masks),
            ) {
                (Some(collider_handle), Some(shape)) => self
                    .rapier
                    .colliders
                    .get_mut(collider_handle)
                    .unwrap()
                    .set_shape(shape),
                (Some(collider_handle), None) => {
                    self.rapier.remove_collider(collider_handle);
                    terrain_patch.collider = None;
                }
                (None, Some(shape)) => {
                    terrain_patch.collider = Some(
                        self.rapier.insert_collider(
                            ColliderBuilder::new(shape)
                                .translation(Vector::new(
                                    terrain_patch_key.x as f32 * 4.0,
                                    terrain_patch_key.y as f32 * 4.0,
                                ))
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
        let stale_terrain_patch_keys: Vec<StaticTerrainCollisionPatchKey> = self
            .terrain_patches
            .iter()
            .filter_map(|(terrain_patch_key, terrain_patch)| {
                (self.terrain_tick - terrain_patch.last_required_tick
                    > TERRAIN_PATCH_RETENTION_TICKS)
                    .then_some(*terrain_patch_key)
            })
            .collect();
        for terrain_patch_key in stale_terrain_patch_keys {
            if let Some(terrain_patch) = self.terrain_patches.remove(&terrain_patch_key)
                && let Some(collider_handle) = terrain_patch.collider
            {
                self.rapier.remove_collider(collider_handle);
            }
        }
        self.terrain_statistics.active_patches = self.terrain_patches.len();
        self.terrain_statistics.collider_patches = self
            .terrain_patches
            .values()
            .filter(|terrain_patch| terrain_patch.collider.is_some())
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
        let mut rows: [u32; 32] = [0u32; 32];
        for patch_y in 0..4 {
            for patch_x in 0..4 {
                let [low_mask, high_mask] = masks[patch_y * 4 + patch_x];
                for cell_y in 0..8 {
                    rows[patch_y * 8 + cell_y] |= ((if cell_y < 4 { low_mask } else { high_mask })
                        >> ((cell_y % 4) * 8)
                        & 0xff)
                        << (patch_x * 8);
                }
            }
        }
        Self::shape_from_rows(&mut rows, 32).0
    }

    pub(crate) fn dynamic_tile_shape(mask: [u32; 2]) -> (Option<SharedShape>, usize) {
        let mut rows: [u32; 8] = [0u32; 8];
        for (cell_y, row) in rows.iter_mut().enumerate() {
            *row = (mask[cell_y / 4] >> ((cell_y % 4) * 8)) & 0xff;
        }
        Self::shape_from_rows(&mut rows, 8)
    }

    fn shape_from_rows(rows: &mut [u32], size: usize) -> (Option<SharedShape>, usize) {
        if rows.iter().all(|row| *row == 0) {
            return (None, 0);
        }
        let mut rectangle_parts: Vec<(Pose, SharedShape)> = Vec::with_capacity(size);
        for cell_y in 0..size {
            while rows[cell_y] != 0 {
                let cell_x: usize = rows[cell_y].trailing_zeros() as usize;
                let rectangle_width: usize = (rows[cell_y] >> cell_x).trailing_ones() as usize;
                let rectangle_mask: u32 = if rectangle_width == 32 {
                    u32::MAX
                } else {
                    (((1u64 << rectangle_width) - 1) as u32) << cell_x
                };
                let mut rectangle_height: usize = 1;
                while cell_y + rectangle_height < size
                    && rows[cell_y + rectangle_height] & rectangle_mask == rectangle_mask
                {
                    rectangle_height += 1;
                }
                for row in &mut rows[cell_y..cell_y + rectangle_height] {
                    *row &= !rectangle_mask;
                }
                rectangle_parts.push((
                    Pose::translation(
                        (cell_x + rectangle_width / 2) as f32 / 8.0
                            + (rectangle_width % 2) as f32 / 16.0,
                        (cell_y + rectangle_height / 2) as f32 / 8.0
                            + (rectangle_height % 2) as f32 / 16.0,
                    ),
                    SharedShape::cuboid(
                        rectangle_width as f32 / 16.0,
                        rectangle_height as f32 / 16.0,
                    ),
                ));
            }
        }
        let rectangle_count: usize = rectangle_parts.len();
        (
            Some(SharedShape::compound(rectangle_parts)),
            rectangle_count,
        )
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn terrain_bridge_statistics(&self) -> TerrainBridgeStatistics {
        self.terrain_statistics
    }
}
