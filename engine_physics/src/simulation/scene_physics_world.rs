// Copyright Rob Gage 2026

use super::{
    CollisionOccupancySnapshot,
    RigidCellularBody,
    RigidCellularBodyState,
};
use crate::actors::ActorCollisionShape;
use crate::materials::MaterialRegistry;
use rapier2d::{
    prelude::{
        ColliderBuilder, ColliderHandle,
        Group,
        InteractionGroups,
        LockedAxes,
        PhysicsWorld,
        Pose,
        RigidBodyBuilder,
        RigidBodyHandle,
        SharedShape,
        Vector,
    },
};
use rapier2d::{
    parry::query::{cast_shapes, ShapeCastOptions},
};
use std::collections::{HashMap, HashSet};

const TERRAIN_COLLISION_PATCH_TILES: i32 = 4;
const TERRAIN_COLLISION_PATCH_CELLS: i32 = TERRAIN_COLLISION_PATCH_TILES * 8;
const TERRAIN_PATCH_RETENTION_TICKS: u64 = 120;
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)] struct TerrainPatchKey { x: i32, y: i32 }
struct TerrainPatch { collider: Option<ColliderHandle>, masks: [[u32; 2]; 16], last_required_tick: u64 }
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, Default)] pub(crate) struct TerrainBridgeStatistics {
    pub(crate) active_patches: usize, pub(crate) collider_patches: usize,
    pub(crate) patch_rebuilds: u64, pub(crate) patch_cells_scanned: u64,
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
    terrain_tick: u64,
    terrain_statistics: TerrainBridgeStatistics,
}

impl ScenePhysicsWorld {

    /// Creates an empty scene collision world
    pub fn new() -> Self {
        Self {
            step_support: Vec::new(),
            step_recovery: Vec::new(),
            rapier: PhysicsWorld::new(),
            cellular_terrain_snapshot: None,
            terrain_patches: HashMap::new(), required_terrain_patches: HashSet::new(),
            terrain_tick: 0, terrain_statistics: TerrainBridgeStatistics::default(),
        }
    }

    /// Inserts one dynamic body-local cellular compound
    pub(crate) fn insert_rigid_cellular_body(
        &mut self,
        position: [f32; 2],
        angle: f32,
        materials: &MaterialRegistry,
        cells: Vec<([i32; 2], crate::materials::MaterialIdentifier,
            crate::tiles::CellularAppearance)>,
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
                .ccd_enabled(true)
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
        RigidCellularBody { handle, cells }
    }

    /// Returns transform and velocities needed to derive a body's world-cell proxy
    pub(crate) fn rigid_cellular_body_state(
        &self,
        body: &RigidCellularBody,
    ) -> Option<RigidCellularBodyState> {
        let rigid_body = self.rapier.bodies.get(body.handle)?;
        assert!(!rigid_body.locked_axes().intersects(
            LockedAxes::TRANSLATION_LOCKED_X | LockedAxes::TRANSLATION_LOCKED_Y,
        ), "Rigid cellular GPU contact requires unlocked translation axes");
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

    /// Replaces the latest CPU-readable cellular collision snapshot
    pub fn update_cellular_snapshot(
        &mut self,
        snapshot: CollisionOccupancySnapshot,
    ) {
        self.cellular_terrain_snapshot = Some(snapshot);
    }

    pub(crate) fn prepare_rigid_cellular_terrain(&mut self, bodies: &[RigidCellularBody], gravity: [f32; 2], dt: f32) {
        self.terrain_tick += 1; self.required_terrain_patches.clear();
        for body in bodies { let Some(rigid) = self.rapier.bodies.get(body.handle) else { continue; };
            for handle in rigid.colliders() { let Some(collider) = self.rapier.colliders.get(*handle) else { continue; };
                let aabb = collider.compute_aabb(); let radius = (aabb.maxs - aabb.mins).length() * 0.5;
                let d = rigid.linvel() * dt + Vector::new(gravity[0], gravity[1]) * (0.5 * dt * dt);
                let angular = rigid.angvel().abs() * dt * radius;
                let lo = aabb.mins.min(aabb.mins + d) - Vector::splat(angular + 0.25);
                let hi = aabb.maxs.max(aabb.maxs + d) + Vector::splat(angular + 0.25);
                let x0 = ((lo.x * 8.0).floor() as i32).div_euclid(TERRAIN_COLLISION_PATCH_CELLS) - 1;
                let y0 = ((lo.y * 8.0).floor() as i32).div_euclid(TERRAIN_COLLISION_PATCH_CELLS) - 1;
                let x1 = ((hi.x * 8.0).floor() as i32).div_euclid(TERRAIN_COLLISION_PATCH_CELLS) + 1;
                let y1 = ((hi.y * 8.0).floor() as i32).div_euclid(TERRAIN_COLLISION_PATCH_CELLS) + 1;
                for y in y0..=y1 { for x in x0..=x1 { self.required_terrain_patches.insert(TerrainPatchKey { x, y }); }}
            }
        }
        let Some(snapshot) = self.cellular_terrain_snapshot.as_ref() else { return; };
        let keys: Vec<_> = self.required_terrain_patches.iter().copied().collect();
        for key in keys { let masks = snapshot.static_patch_masks(key.x, key.y);
            let changed = self.terrain_patches.get(&key).is_none_or(|p| p.masks != masks);
            let patch = self.terrain_patches.entry(key).or_insert(TerrainPatch { collider: None, masks, last_required_tick: self.terrain_tick });
            patch.last_required_tick = self.terrain_tick; if !changed { continue; }
            patch.masks = masks; self.terrain_statistics.patch_rebuilds += 1; self.terrain_statistics.patch_cells_scanned += 1024;
            match (patch.collider, Self::terrain_patch_shape(&masks)) {
                (Some(h), Some(s)) => self.rapier.colliders.get_mut(h).unwrap().set_shape(s),
                (Some(h), None) => { self.rapier.remove_collider(h); patch.collider = None; }
                (None, Some(s)) => patch.collider = Some(self.rapier.insert_collider(ColliderBuilder::new(s)
                    .translation(Vector::new(key.x as f32 * 4.0, key.y as f32 * 4.0)).friction(0.8).restitution(0.0)
                    .collision_groups(Self::terrain_collision_groups()).solver_groups(Self::terrain_solver_groups()), None)),
                (None, None) => {}
            }
        }
        let old: Vec<_> = self.terrain_patches.iter().filter_map(|(k, p)|
            (self.terrain_tick - p.last_required_tick > TERRAIN_PATCH_RETENTION_TICKS).then_some(*k)).collect();
        for k in old { if let Some(p) = self.terrain_patches.remove(&k) { if let Some(h) = p.collider { self.rapier.remove_collider(h); } }}
        self.terrain_statistics.active_patches = self.terrain_patches.len();
        self.terrain_statistics.collider_patches = self.terrain_patches.values().filter(|p| p.collider.is_some()).count();
    }

    fn terrain_patch_shape(masks: &[[u32; 2]; 16]) -> Option<SharedShape> {
        let mut rows = [0u32; 32];
        for ty in 0..4 { for tx in 0..4 { let [lo, hi] = masks[ty * 4 + tx]; for y in 0..8 {
            rows[ty * 8 + y] |= ((if y < 4 { lo } else { hi }) >> ((y % 4) * 8) & 0xff) << (tx * 8);
        }}}
        if rows.iter().all(|r| *r == 0) { return None; }
        let mut parts = Vec::new();
        for y in 0..32 { while rows[y] != 0 { let x = rows[y].trailing_zeros() as usize; let w = (rows[y] >> x).trailing_ones() as usize;
            let mask = if w == 32 { u32::MAX } else { (((1u64 << w) - 1) as u32) << x }; let mut h = 1;
            while y + h < 32 && rows[y + h] & mask == mask { h += 1; } for row in &mut rows[y..y + h] { *row &= !mask; }
            parts.push((Pose::translation((x + w / 2) as f32 / 8.0 + (w % 2) as f32 / 16.0, (y + h / 2) as f32 / 8.0 + (h % 2) as f32 / 16.0), SharedShape::cuboid(w as f32 / 16.0, h as f32 / 16.0)));
        }} Some(SharedShape::compound(parts))
    }

    #[cfg(test)] pub(crate) fn terrain_bridge_statistics(&self) -> TerrainBridgeStatistics { self.terrain_statistics }

    /// Advances Rapier's collision world by one fixed scene step
    pub fn step(&mut self, gravity: [f32; 2], delta_time: f32) {
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
    }

    /// Applies one already-integrated GPU impulse batch to its authoritative body
    pub(crate) fn apply_rigid_constraint(
        &mut self, body: &RigidCellularBody, constraint: [f32; 4], source: [f32; 4], wake: bool,
    ) -> bool {
        let Some(state) = self.rigid_cellular_body_state(body) else { return false; };
        let effective = state.inverse_mass * (constraint[0] * constraint[0] + constraint[1] * constraint[1]) +
            state.inverse_angular_inertia * constraint[2] * constraint[2];
        if effective <= 1e-12 {
            return self.apply_rigid_cellular_body_reaction(body, [0.0; 2], 0.0, 0.0, wake);
        }
        // The GPU impulse defines a velocity target along its generalized contact direction.
        // Re-evaluate that target against current motion instead of replaying stale stopping work.
        let target = source[0] * constraint[0] + source[1] * constraint[1] + source[2] * constraint[2] + effective;
        let target = if source[3] == 0.0 { target.max(0.0) } else { target };
        let current = state.linear_velocity[0] * constraint[0] + state.linear_velocity[1] * constraint[1] + state.angular_velocity * constraint[2];
        let scale = ((target - current) / effective).max(0.0);
        self.apply_rigid_cellular_body_reaction(body,
            [constraint[0] * scale, constraint[1] * scale], constraint[2] * scale, constraint[3], wake)
    }

    pub(crate) fn apply_rigid_support(&mut self, body: &RigidCellularBody, support: [f32; 4]) -> bool {
        let Some(rigid) = self.rapier.bodies.get_mut(body.handle) else { return false; };
        if rigid.is_sleeping() { return true; }
        // Spread the confirmed one-step impulse through Rapier's integration substeps.
        // An upfront velocity kick cancels final gravity velocity but introduces position drift.
        let impulse = Vector::new(support[0], support[1]);
        let quadratic = 0.5 * (rigid.mass_properties().local_mprops.inv_mass * impulse.length_squared() +
            rigid.mass_properties().effective_world_inv_inertia * support[2] * support[2]);
        let linear = rigid.linvel().dot(impulse) + rigid.angvel() * support[2];
        let budget = support[3].max(0.0);
        let scale = if linear + quadratic <= budget { 1.0 } else if quadratic > 0.0 {
            ((linear * linear + 4.0 * quadratic * budget).sqrt() - linear) / (2.0 * quadratic)
        } else { 0.0 }.clamp(0.0, 1.0);
        let force = impulse * (60.0 * scale);
        let torque = support[2] * (60.0 * scale);
        rigid.add_force(force, false);
        rigid.add_torque(torque, false);
        self.step_support.push((body.handle, force, torque));
        true
    }

    pub(crate) fn apply_rigid_recovery(&mut self, body: &RigidCellularBody, recovery: [f32; 4]) -> bool {
        let Some(before) = self.rigid_cellular_body_state(body) else { return false; };
        self.apply_rigid_cellular_body_reaction(body, [recovery[0], recovery[1]], recovery[2], recovery[3], false);
        let after = self.rigid_cellular_body_state(body).unwrap();
        self.step_recovery.push((body.handle,
            Vector::new(after.linear_velocity[0] - before.linear_velocity[0], after.linear_velocity[1] - before.linear_velocity[1]),
            after.angular_velocity - before.angular_velocity));
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
        let Some(rigid_body) = self.rapier.bodies.get_mut(body.handle) else { return false; };
        let inverse_mass: f32 = rigid_body.mass_properties().local_mprops.inv_mass;
        let inverse_inertia: f32 = rigid_body.mass_properties().effective_world_inv_inertia;
        let linear: Vector = Vector::new(impulse[0], impulse[1]);
        let quadratic: f32 = 0.5 * (inverse_mass * linear.length_squared() +
            inverse_inertia * angular_impulse * angular_impulse);
        let linear_term: f32 = rigid_body.linvel().dot(linear) +
            rigid_body.angvel() * angular_impulse;
        let budget: f32 = energy_budget.max(0.0);
        let scale: f32 = if linear_term + quadratic <= budget {
            1.0
        } else if quadratic > 0.0 {
            ((linear_term * linear_term + 4.0 * quadratic * budget).sqrt() -
                linear_term) / (2.0 * quadratic)
        } else if linear_term > 0.0 {
            budget / linear_term
        } else {
            1.0
        }.clamp(0.0, 1.0);
        if rigid_body.is_sleeping() && !wake { return true; }
        if wake { rigid_body.wake_up(true); }
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
            .with_filter(Group::ALL & !Group::GROUP_2)
    }
    fn rigid_collision_groups() -> InteractionGroups { InteractionGroups::all().with_memberships(Group::GROUP_1).with_filter(Group::ALL & !Group::GROUP_2) }
    fn terrain_collision_groups() -> InteractionGroups { InteractionGroups::all().with_memberships(Group::GROUP_3).with_filter(Group::GROUP_1) }
    fn terrain_solver_groups() -> InteractionGroups { InteractionGroups::all().with_memberships(Group::GROUP_3).with_filter(Group::GROUP_1) }

    /// Resolves authoritative actor motion against cellular occupancy and non-cellular Rapier bodies.
    pub(crate) fn move_actor(
        &self,
        shape: ActorCollisionShape,
        position: Vector,
        desired: Vector,
        up: Vector,
        walkable_normal: f32,
        snap_distance: f32,
        ignored_cell_normal: Option<Vector>,
        collisions: &mut impl FnMut(Vector),
    ) -> (Vector, bool) {
        let (mut translation, mut grounded) = self.resolve_actor_translation(
            shape, position, desired, up, walkable_normal, ignored_cell_normal, collisions,
        );
        if snap_distance > 0.0 && desired.dot(up) <= 0.0 && !grounded {
            let (snap, snapped) = self.resolve_actor_support(shape, position + translation,
                snap_distance, up);
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
        ignored_cell_normal: Option<Vector>,
        collisions: &mut impl FnMut(Vector),
    ) -> (Vector, bool) {
        let offset = 1.0 / 1024.0;
        let primitive = shape.rapier_shape();
        let extent = shape.world_extent(up) + Vector::splat(offset);
        let cell = SharedShape::cuboid(1.0 / 16.0, 1.0 / 16.0);
        let mut consumed = Vector::ZERO;
        let mut remaining = desired;
        let mut grounded = false;
        for _ in 0..4 {
            if remaining.length_squared() < 1e-12 { break; }
            let from = position + consumed;
            let to = from + remaining;
            let minimum = from.min(to) - extent;
            let maximum = from.max(to) + extent;
            let bounds = [(minimum.x * 8.0).floor() as i32, (minimum.y * 8.0).floor() as i32,
                (maximum.x * 8.0).ceil() as i32, (maximum.y * 8.0).ceil() as i32];
            let moving_pose = shape.pose(from, up);
            let options = ShapeCastOptions { max_time_of_impact: 1.0,
                target_distance: offset, stop_at_penetration: false,
                compute_impact_geometry_on_penetration: true };
            let mut earliest = 2.0f32;
            let mut normals = [Vector::ZERO; 4];
            let mut cellular_normals = [false; 4];
            let mut normal_count = 0usize;
            for y in bounds[1]..bounds[3] {
                for x in bounds[0]..bounds[2] {
                    let occupied = self.cellular_terrain_snapshot.as_ref().is_some_and(|snapshot|
                        snapshot.is_static_cell_occupied(x, y) == Some(true) ||
                        snapshot.is_dynamic_cell_occupied(x, y) == Some(true));
                    if !occupied { continue; }
                    let cell_pose = Pose::translation(x as f32 / 8.0 + 1.0 / 16.0,
                        y as f32 / 8.0 + 1.0 / 16.0);
                    let Ok(Some(hit)) = cast_shapes(&moving_pose, remaining, primitive.as_ref(),
                        &cell_pose, Vector::ZERO, cell.as_ref(), options) else { continue; };
                    if hit.time_of_impact + 1e-4 < earliest {
                        earliest = hit.time_of_impact;
                        normal_count = 0;
                    }
                    if (hit.time_of_impact - earliest).abs() <= 1e-4 && normal_count < normals.len() {
                        let normal = moving_pose.rotation * hit.normal2;
                        if !normals[..normal_count].iter().any(|current| current.dot(normal) > 0.999) {
                            normals[normal_count] = normal;
                            cellular_normals[normal_count] = true;
                            normal_count += 1;
                        }
                    }
                }
            }
            let query = self.rapier.query_pipeline();
            if let Some((_handle, hit)) = query.cast_shape(&moving_pose, remaining,
                    primitive.as_ref(), options) {
                if hit.time_of_impact + 1e-4 < earliest {
                    earliest = hit.time_of_impact;
                    normal_count = 0;
                }
                if (hit.time_of_impact - earliest).abs() <= 1e-4 && normal_count < normals.len() &&
                        !normals[..normal_count].iter().any(|current| current.dot(hit.normal1) > 0.999) {
                    normals[normal_count] = hit.normal1;
                    cellular_normals[normal_count] = false;
                    normal_count += 1;
                }
            }
            if earliest > 1.0 { consumed += remaining; break; }
            let advance = remaining * earliest.max(0.0);
            consumed += advance;
            remaining -= advance;
            let mut active_normals = 0usize;
            for index in 0..normal_count {
                let normal = normals[index];
                if cellular_normals[index] && ignored_cell_normal.is_some_and(|ignored|
                        normal.dot(ignored) > 0.999) {
                    continue;
                }
                active_normals += 1;
                collisions(normal);
                grounded |= normal.dot(up) >= walkable_normal;
                let inward = remaining.dot(normal);
                if inward < 0.0 { remaining -= normal * inward; }
            }
            if active_normals == 0 { consumed += remaining; break; }
        }
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
        if !distance.is_finite() || distance <= 0.0 { return (Vector::ZERO, false); }
        let offset = 1.0 / 1024.0;
        let desired = -up * distance;
        let primitive = shape.rapier_shape();
        let extent = shape.world_extent(up) + Vector::splat(offset);
        let minimum = (position + desired).min(position) - extent;
        let maximum = (position + desired).max(position) + extent;
        let bounds = [(minimum.x * 8.0).floor() as i32, (minimum.y * 8.0).floor() as i32,
            (maximum.x * 8.0).ceil() as i32, (maximum.y * 8.0).ceil() as i32];
        let moving_pose = shape.pose(position, up);
        let cell = SharedShape::cuboid(1.0 / 16.0, 1.0 / 16.0);
        let options = ShapeCastOptions { max_time_of_impact: 1.0,
            target_distance: offset, stop_at_penetration: false,
            compute_impact_geometry_on_penetration: true };
        let mut earliest = 1.0f32;
        let mut support = false;
        for y in bounds[1]..bounds[3] {
            for x in bounds[0]..bounds[2] {
                let occupied = self.cellular_terrain_snapshot.as_ref().is_some_and(|snapshot|
                    snapshot.is_static_cell_occupied(x, y) == Some(true) ||
                    snapshot.is_dynamic_cell_occupied(x, y) == Some(true));
                if !occupied { continue; }
                let cell_pose = Pose::translation(x as f32 / 8.0 + 1.0 / 16.0,
                    y as f32 / 8.0 + 1.0 / 16.0);
                let Ok(Some(hit)) = cast_shapes(&moving_pose, desired, primitive.as_ref(),
                    &cell_pose, Vector::ZERO, cell.as_ref(), options) else { continue; };
                if (moving_pose.rotation * hit.normal2).dot(up) > 1e-4 &&
                        hit.time_of_impact <= earliest {
                    earliest = hit.time_of_impact;
                    support = true;
                }
            }
        }
        let query = self.rapier.query_pipeline();
        if let Some((_handle, hit)) = query.cast_shape(&moving_pose, desired, primitive.as_ref(),
                options) {
            if hit.normal1.dot(up) > 1e-4 && hit.time_of_impact <= earliest {
                earliest = hit.time_of_impact;
                support = true;
            }
        }
        (-up * (distance * earliest), support)
    }

}

#[cfg(test)]
mod tests {
    use super::ScenePhysicsWorld;
    use crate::{
        actors::ActorCollisionShape,
        simulation::CollisionOccupancySnapshot,
        tiles::TileCoordinates,
    };
    use rapier2d::prelude::{Pose, Vector};
    use crate::{materials::{Material, MaterialRegistry}, tiles::CellularAppearance};
    use engine_graphics::{Color, MaterialAppearance};

    fn assert_patch_bounds(masks: [[u32; 2]; 16], origin: [f32; 2], expected: [f32; 4]) {
        let shape = ScenePhysicsWorld::terrain_patch_shape(&masks).unwrap();
        let aabb = shape.compute_aabb(&Pose::translation(origin[0], origin[1]));
        assert!((aabb.mins.x - expected[0]).abs() < 1e-5 && (aabb.mins.y - expected[1]).abs() < 1e-5 &&
            (aabb.maxs.x - expected[2]).abs() < 1e-5 && (aabb.maxs.y - expected[3]).abs() < 1e-5,
            "actual {:?}..{:?}", aabb.mins, aabb.maxs);
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
        let stone = materials.register(Material::CellularStatic { name: "Stone".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(90, 90, 90)), mass: 1.0,
            pressure_ignore_threshold: 1.0, default_integrity: 1.0, debris_material: None,
            debris_yield_rate: 0.0, pressure_transmission: 1.0, friction: 0.5, restitution: 0.0 });
        let mut world = ScenePhysicsWorld::new();
        let snapshot = |hole: bool| { let mut masks = vec![[u32::MAX; 2]; 16];
            if hole { masks[5][0] &= !(1 << 9); }
            CollisionOccupancySnapshot { sequence: 0, origin: TileCoordinates { x: 0, y: -4 }, width: 4, height: 4,
                static_masks: masks.into_boxed_slice(), dynamic_masks: vec![[0; 2]; 16].into_boxed_slice() }
        };
        world.update_cellular_snapshot(snapshot(false));
        let body = world.insert_rigid_cellular_body([1.0, 1.0], 0.0, &materials,
            vec![([0, 0], stone, CellularAppearance::NEUTRAL)], 0.5, 0.0, [0.0, 0.0], 0.0);
        for _ in 0..180 { world.prepare_rigid_cellular_terrain(std::slice::from_ref(&body), [0.0, -9.81], 1.0 / 60.0); world.step([0.0, -9.81], 1.0 / 60.0); }
        let full_height = world.rigid_cellular_body_state(&body).unwrap().translation[1];
        world.update_cellular_snapshot(snapshot(true));
        for _ in 0..60 { world.prepare_rigid_cellular_terrain(std::slice::from_ref(&body), [0.0, -9.81], 1.0 / 60.0); world.step([0.0, -9.81], 1.0 / 60.0); }
        let edited_height = world.rigid_cellular_body_state(&body).unwrap().translation[1];
        assert!(full_height > -0.01 && (full_height - edited_height).abs() < 0.01,
            "full={full_height}, edited={edited_height}");
    }

    #[test]
    fn stale_rigid_reaction_cannot_create_energy_without_grid_transfer() {
        let mut materials = MaterialRegistry::new();
        let stone = materials.register(Material::CellularStatic {
            name: "Stone".into(),
            graphics: MaterialAppearance::from_color(Color::new_rgb(90, 90, 90)),
            mass: 1.0, pressure_ignore_threshold: 1000.0, default_integrity: 100.0,
            debris_material: None, debris_yield_rate: 0.0,
            pressure_transmission: 1.0, friction: 0.5, restitution: 0.0,
        });
        let mut physics = ScenePhysicsWorld::new();
        let body = physics.insert_rigid_cellular_body([0.0, 0.0], 0.0,
            &materials, vec![([0, 0], stone, CellularAppearance::NEUTRAL)],
            0.5, 0.0, [0.0, 0.0], 0.0);
        assert!(physics.apply_rigid_cellular_body_reaction(&body,
            [1.0, 0.0], 0.0, 0.0, true));
        physics.step([0.0, 0.0], 1.0 / 60.0);
        let state = physics.rigid_cellular_body_state(&body).unwrap();
        assert!(state.linear_velocity[0].abs() < 0.0001);
        assert!(physics.apply_rigid_cellular_body_reaction(&body,
            [1.0, 0.0], 0.0, 0.5, true));
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
            mass: 1.0, pressure_ignore_threshold: 1000.0, default_integrity: 100.0,
            debris_material: None, debris_yield_rate: 0.0,
            pressure_transmission: 1.0, friction: 0.5, restitution: 0.0,
        });
        let cell = || vec![([0, 0], stone, CellularAppearance::NEUTRAL)];
        let mut physics = ScenePhysicsWorld::new();
        let left = physics.insert_rigid_cellular_body(
            [0.0, 0.0], 0.0, &materials, cell(), 0.5, 0.0, [1.0, 0.0], 0.0,
        );
        let right = physics.insert_rigid_cellular_body(
            [0.3, 0.0], 0.0, &materials, cell(), 0.5, 0.0, [-1.0, 0.0], 0.0,
        );
        for _ in 0..20 { physics.step([0.0, 0.0], 1.0 / 60.0); }
        let left_x = physics.rigid_cellular_body_state(&left).unwrap().translation[0];
        let right_x = physics.rigid_cellular_body_state(&right).unwrap().translation[0];
        assert!(left_x + 0.12 <= right_x, "rigid bodies interpenetrated: {left_x}, {right_x}");
    }

    #[test]
    fn every_actor_primitive_casts_directly_against_cellular_occupancy() {
        for shape in [
            ActorCollisionShape::Circle { radius: 0.25 },
            ActorCollisionShape::Capsule { radius: 0.25, height: 0.75 },
            ActorCollisionShape::Rectangle { width: 0.5, height: 0.75 },
        ] {
            for static_occupancy in [true, false] {
                let mut physics = ScenePhysicsWorld::new();
                physics.update_cellular_snapshot(CollisionOccupancySnapshot {
                    sequence: 0,
                    origin: TileCoordinates { x: 0, y: 0 },
                    width: 1,
                    height: 1,
                    static_masks: vec![[0, if static_occupancy { 1 << 4 } else { 0 }]]
                        .into_boxed_slice(),
                    dynamic_masks: vec![[0, if static_occupancy { 0 } else { 1 << 4 }]]
                        .into_boxed_slice(),
                });
                let (movement, _) = physics.move_actor(shape, Vector::new(0.0, 0.5625),
                    Vector::new(1.0, 0.0), Vector::Y, 0.0, 0.0, None, &mut |_| { });
                assert!(movement.x < 0.5);
            }
        }
    }

    #[test]
    fn starting_overlap_allows_separating_dynamic_cell_motion() {
        let mut physics = ScenePhysicsWorld::new();
        physics.update_cellular_snapshot(CollisionOccupancySnapshot {
            sequence: 0,
            origin: TileCoordinates { x: 0, y: 0 },
            width: 1,
            height: 1,
            static_masks: vec![[0, 0]].into_boxed_slice(),
            dynamic_masks: vec![[0, 1 << 4]].into_boxed_slice(),
        });
        let (movement, _) = physics.move_actor(ActorCollisionShape::Circle { radius: 0.25 },
            Vector::new(0.5625, 0.5625), Vector::new(-0.25, 0.0), Vector::Y, 0.0, 0.0,
            None, &mut |_| { });
        assert!(movement.x < -0.2);
    }

}
