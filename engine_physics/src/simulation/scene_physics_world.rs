// Copyright Rob Gage 2026

use super::{
    CollisionOccupancySnapshot,
    RigidCellularBody,
    RigidCellularBodyState,
};
use crate::actors::ActorCollisionShape;
use rapier2d::{
    prelude::{
        ColliderBuilder,
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

/// Owns Rapier rigid bodies and the CPU-readable cellular collision snapshot
pub struct ScenePhysicsWorld {
    rapier: PhysicsWorld,
    /// Latest asynchronously completed canonical cellular collision snapshot
    cellular_terrain_snapshot: Option<CollisionOccupancySnapshot>,
}

impl ScenePhysicsWorld {

    /// Creates an empty scene collision world
    pub fn new() -> Self {
        Self {
            rapier: PhysicsWorld::new(),
            cellular_terrain_snapshot: None,
        }
    }

    /// Inserts one dynamic body-local cellular compound
    pub(crate) fn insert_rigid_cellular_body(
        &mut self,
        position: [f32; 2],
        angle: f32,
        cells: Vec<([i32; 2], crate::materials::MaterialIdentifier,
            crate::tiles::CellularAppearance)>,
        friction: f32,
        restitution: f32,
        linear_velocity: [f32; 2],
        angular_velocity: f32,
    ) -> RigidCellularBody {
        let handle: RigidBodyHandle = self.rapier.insert_body(
            RigidBodyBuilder::dynamic()
                .translation(Vector::new(position[0], position[1]))
                .rotation(angle)
                .linvel(Vector::new(linear_velocity[0], linear_velocity[1]))
                .angvel(angular_velocity),
        );
        self.rapier.insert_collider(
            ColliderBuilder::new(RigidCellularBody::collision_shape(&cells))
                .density(64.0)
                .friction(friction)
                .restitution(restitution)
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

    /// Advances Rapier's collision world by one fixed scene step
    pub fn step(&mut self, gravity: [f32; 2], delta_time: f32) {
        self.rapier.gravity = Vector::new(gravity[0], gravity[1]);
        self.rapier.integration_parameters.dt = delta_time;
        self.rapier.step();
    }

    /// Applies one already-integrated GPU impulse batch to its authoritative body
    pub(crate) fn apply_rigid_cellular_body_reaction(
        &mut self,
        body: &RigidCellularBody,
        impulse: [f32; 2],
        angular_impulse: f32,
        wake: bool,
    ) -> bool {
        let Some(rigid_body) = self.rapier.bodies.get_mut(body.handle) else { return false; };
        if impulse != [0.0; 2] {
            rigid_body.apply_impulse(Vector::new(impulse[0], impulse[1]), wake);
        }
        if angular_impulse != 0.0 { rigid_body.apply_torque_impulse(angular_impulse, wake); }
        true
    }

    /// Wakes one authoritative rigid body for granular support maintenance
    pub(crate) fn wake_rigid_cellular_body(&mut self, body: &RigidCellularBody) -> bool {
        let Some(rigid_body) = self.rapier.bodies.get_mut(body.handle) else { return false; };
        rigid_body.wake_up(true);
        true
    }

    fn rigid_solver_groups() -> InteractionGroups {
        InteractionGroups::all()
            .with_memberships(Group::GROUP_1)
            .with_filter(Group::ALL & !Group::GROUP_2)
    }

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
    use rapier2d::prelude::Vector;

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
