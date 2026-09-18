// Copyright Rob Gage 2026

use super::*;

impl ScenePhysicsWorld {
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

    fn demand_dynamic_tiles(
        required: &mut HashSet<RigidDynamicCollisionTileKey>,
        lo: Vector,
        hi: Vector,
    ) {
        for y in (lo.y.floor() as i32 - 1)..=(hi.y.floor() as i32 + 1) {
            for x in (lo.x.floor() as i32 - 1)..=(hi.x.floor() as i32 + 1) {
                required.insert(RigidDynamicCollisionTileKey { x, y });
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
                let d = rigid.linvel() * delta_time
                    + Vector::new(gravity[0], gravity[1]) * (0.5 * delta_time * delta_time);
                let angular = rigid.angvel().abs() * delta_time * radius;
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
                            .insert(StaticTerrainCollisionPatchKey { x, y });
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
            let d = Vector::new(actor.velocity[0], actor.velocity[1]) * delta_time
                + Vector::new(gravity[0], gravity[1]) * (0.5 * delta_time * delta_time);
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
                        .insert(StaticTerrainCollisionPatchKey { x, y });
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
            let tile = self
                .dynamic_tiles
                .entry(key)
                .or_insert(RigidDynamicCollisionTile {
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
        // a support tile can vanish underneath a sleeping body. Wake only bodies touching it.
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
            if let Some(tile) = self.dynamic_tiles.remove(&key)
                && let Some(handle) = tile.collider
            {
                self.rapier.remove_collider(handle);
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
            let patch = self
                .terrain_patches
                .entry(key)
                .or_insert(StaticTerrainCollisionPatch {
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
            if let Some(p) = self.terrain_patches.remove(&k)
                && let Some(h) = p.collider
            {
                self.rapier.remove_collider(h);
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
}
