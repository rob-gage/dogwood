// Copyright Rob Gage 2026

use super::{
    CollisionOccupancySnapshot,
    RigidCellularBody,
    RigidCellularBodyState,
};
use crate::actors::ActorCollisionShape;
use rapier2d::{
    control::{
        CharacterLength,
        EffectiveCharacterMovement,
        KinematicCharacterController,
    },
    prelude::{
        CCDSolver,
        ColliderBuilder,
        ColliderHandle,
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
use rapier2d::parry::query::{cast_shapes, ShapeCastOptions};

/// Owns the Rapier collision world and its derived cellular terrain
pub struct ScenePhysicsWorld {
    rapier: PhysicsWorld,
    /// World-scale static cellular terrain, rebuilt only when static occupancy changes
    static_cellular_terrain: Option<ColliderHandle>,
    /// Latest asynchronously completed canonical cellular collision snapshot
    cellular_terrain_snapshot: Option<CollisionOccupancySnapshot>,
    /// Whether Rapier's CCD fixed-target cache predates current cellular terrain
    ccd_fixed_targets_dirty: bool,
}

impl ScenePhysicsWorld {

    /// Creates an empty scene collision world
    pub fn new() -> Self {
        Self {
            rapier: PhysicsWorld::new(),
            static_cellular_terrain: None,
            cellular_terrain_snapshot: None,
            ccd_fixed_targets_dirty: false,
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

    /// Updates world-scale Rapier terrain only when static cellular occupancy changes
    pub fn update_cellular_terrain(
        &mut self,
        snapshot: CollisionOccupancySnapshot,
    ) {
        let static_changed: bool = self.cellular_terrain_snapshot.as_ref().is_none_or(|current| {
            current.origin != snapshot.origin || current.width != snapshot.width ||
                current.height != snapshot.height || current.static_masks != snapshot.static_masks
        });
        if static_changed {
            let origin_x: i32 = snapshot.origin.x * 8;
            let origin_y: i32 = snapshot.origin.y * 8;
            let shape: Option<SharedShape> = Self::cellular_collision_shape(
                &snapshot,
                [
                    origin_x,
                    origin_y,
                    origin_x + i32::from(snapshot.width) * 8,
                    origin_y + i32::from(snapshot.height) * 8,
                ],
            );
            self.ccd_fixed_targets_dirty |= Self::replace_cellular_collider(
                &mut self.rapier,
                &mut self.static_cellular_terrain,
                shape,
                InteractionGroups::all(),
            );
        }
        self.cellular_terrain_snapshot = Some(snapshot);
    }

    /// Builds greedy cellular rectangles inside one world-cell area
    fn cellular_collision_shape(
        snapshot: &CollisionOccupancySnapshot,
        area: [i32; 4],
    ) -> Option<SharedShape> {
        let snapshot_x: i32 = snapshot.origin.x * 8;
        let snapshot_y: i32 = snapshot.origin.y * 8;
        let minimum_x: i32 = area[0].max(snapshot_x);
        let minimum_y: i32 = area[1].max(snapshot_y);
        let maximum_x: i32 = area[2].min(snapshot_x + i32::from(snapshot.width) * 8);
        let maximum_y: i32 = area[3].min(snapshot_y + i32::from(snapshot.height) * 8);
        let width: i32 = maximum_x - minimum_x;
        let height: i32 = maximum_y - minimum_y;
        if width <= 0 || height <= 0 { return None; }
        let mut consumed: Vec<bool> = vec![false; (width * height) as usize];
        let occupied = |x: i32, y: i32| snapshot.is_static_cell_occupied(x, y) == Some(true);
        let mut parts: Vec<(Pose, SharedShape)> = Vec::new();
        for relative_y in 0..height {
            let mut relative_x: i32 = 0;
            while relative_x < width {
                let index: usize = (relative_y * width + relative_x) as usize;
                if consumed[index] || !occupied(
                    minimum_x + relative_x,
                    minimum_y + relative_y,
                ) {
                    relative_x += 1;
                    continue;
                }
                let rectangle_x: i32 = relative_x;
                let mut rectangle_width: i32 = 1;
                while rectangle_x + rectangle_width < width {
                    let next_x: i32 = rectangle_x + rectangle_width;
                    let next_index: usize = (relative_y * width + next_x) as usize;
                    if consumed[next_index] || !occupied(
                        minimum_x + next_x,
                        minimum_y + relative_y,
                    ) { break; }
                    rectangle_width += 1;
                }
                let mut rectangle_height: i32 = 1;
                while relative_y + rectangle_height < height &&
                        (rectangle_x..rectangle_x + rectangle_width).all(|x| {
                            let index: usize =
                                ((relative_y + rectangle_height) * width + x) as usize;
                            !consumed[index] && occupied(
                                minimum_x + x,
                                minimum_y + relative_y + rectangle_height,
                            )
                        }) {
                    rectangle_height += 1;
                }
                for y in relative_y..relative_y + rectangle_height {
                    for x in rectangle_x..rectangle_x + rectangle_width {
                        consumed[(y * width + x) as usize] = true;
                    }
                }
                let world_x: i32 = minimum_x + rectangle_x;
                let world_y: i32 = minimum_y + relative_y;
                parts.push((
                    Pose::translation(
                        world_x as f32 / 8.0 + rectangle_width as f32 / 16.0,
                        world_y as f32 / 8.0 + rectangle_height as f32 / 16.0,
                    ),
                    SharedShape::cuboid(
                        rectangle_width as f32 / 16.0,
                        rectangle_height as f32 / 16.0,
                    ),
                ));
                relative_x += rectangle_width;
            }
        }
        (!parts.is_empty()).then(|| SharedShape::compound(parts))
    }

    /// Changes one persistent collider handle to represent an optional derived shape
    fn replace_cellular_collider(
        rapier: &mut PhysicsWorld,
        handle: &mut Option<ColliderHandle>,
        shape: Option<SharedShape>,
        solver_groups: InteractionGroups,
    ) -> bool {
        match (shape, *handle) {
            (Some(shape), Some(handle)) => {
                rapier.colliders[handle].set_shape(shape);
                true
            }
            (Some(shape), None) => {
                *handle = Some(rapier.insert_collider(
                    ColliderBuilder::new(shape).solver_groups(solver_groups).build(), None,
                ));
                true
            }
            (None, Some(current)) => {
                rapier.remove_collider(current);
                *handle = None;
                true
            }
            (None, None) => false,
        }
    }

    /// Advances Rapier's collision world by one fixed scene step
    pub fn step(&mut self, gravity: [f32; 2], delta_time: f32) {
        self.rapier.gravity = Vector::new(gravity[0], gravity[1]);
        self.rapier.integration_parameters.dt = delta_time;
        if self.ccd_fixed_targets_dirty {
            self.rapier.ccd_solver = CCDSolver::new();
            self.ccd_fixed_targets_dirty = false;
        }
        self.rapier.step();
    }

    /// Applies one already-integrated GPU impulse batch to its authoritative body
    pub(crate) fn apply_rigid_cellular_body_reaction(
        &mut self,
        body: &RigidCellularBody,
        impulse: [f32; 2],
        angular_impulse: f32,
    ) -> bool {
        let Some(rigid_body) = self.rapier.bodies.get_mut(body.handle) else { return false; };
        if impulse != [0.0; 2] { rigid_body.apply_impulse(Vector::new(impulse[0], impulse[1]), true); }
        if angular_impulse != 0.0 { rigid_body.apply_torque_impulse(angular_impulse, true); }
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

    /// Resolves a character shape's desired translation against the scene collision world
    pub fn move_character(
        &self,
        controller: &KinematicCharacterController,
        delta_time: f32,
        shape: ActorCollisionShape,
        position: &Pose,
        desired_translation: Vector,
        mut collisions: impl FnMut(Vector),
    ) -> EffectiveCharacterMovement {
        let up = controller.up;
        let primitive = shape.rapier_shape();
        let rapier_pose = shape.pose(position.translation, up);
        let mut candidate = desired_translation;
        let mut grounded = false;
        let mut sliding = false;
        for _ in 0..3 {
            let rapier = controller.move_shape(delta_time, &self.rapier.query_pipeline(),
                primitive.as_ref(), &rapier_pose, candidate, |collision| {
                    collisions(collision.hit.normal1);
                });
            grounded |= rapier.grounded;
            sliding |= rapier.is_sliding_down_slope;
            let dynamic = self.resolve_dynamic_cells(shape, position, rapier.translation,
                1.0 / 1024.0, up, &mut collisions);
            grounded |= dynamic.1;
            if (dynamic.0 - rapier.translation).length_squared() < 1e-10 {
                candidate = dynamic.0;
                break;
            }
            candidate = dynamic.0;
        }
        if desired_translation.dot(up) <= 0.0 {
            let snap = match controller.snap_to_ground {
                Some(CharacterLength::Absolute(distance)) => distance,
                Some(CharacterLength::Relative(scale)) =>
                    shape.nominal_dimensions()[1] * scale,
                None => 0.0,
            };
            if snap > 0.0 && !grounded {
                let snap_position = Pose::new(position.translation + candidate, position.rotation.angle());
                let snapped = self.resolve_dynamic_cells(shape, &snap_position, -up * snap,
                    1.0 / 1024.0, up, &mut collisions);
                if snapped.1 {
                    candidate += snapped.0;
                    grounded = true;
                }
            }
        }
        EffectiveCharacterMovement { translation: candidate, grounded,
            is_sliding_down_slope: sliding }
    }

    fn resolve_dynamic_cells(
        &self,
        shape: ActorCollisionShape,
        position: &Pose,
        desired: Vector,
        offset: f32,
        up: Vector,
        collisions: &mut impl FnMut(Vector),
    ) -> (Vector, bool) {
        let Some(snapshot) = self.cellular_terrain_snapshot.as_ref() else {
            return (desired, false);
        };
        let primitive = shape.rapier_shape();
        let start = position.translation;
        let extent = shape.world_extent(up) + Vector::splat(offset);
        let cell = SharedShape::cuboid(1.0 / 16.0, 1.0 / 16.0);
        let mut consumed = Vector::ZERO;
        let mut remaining = desired;
        let mut grounded = false;
        for _ in 0..4 {
            if remaining.length_squared() < 1e-12 { break; }
            let from = start + consumed;
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
            let mut normal_count = 0usize;
            for y in bounds[1]..bounds[3] {
                for x in bounds[0]..bounds[2] {
                    if snapshot.is_dynamic_cell_occupied(x, y) != Some(true) { continue; }
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
                            normal_count += 1;
                        }
                    }
                }
            }
            if earliest > 1.0 { consumed += remaining; break; }
            let advance = remaining * earliest.max(0.0);
            consumed += advance;
            remaining -= advance;
            for normal in &normals[..normal_count] {
                collisions(*normal);
                grounded |= normal.dot(up) >= 0.5;
                let inward = remaining.dot(*normal);
                if inward < 0.0 { remaining -= *normal * inward; }
            }
        }
        (consumed, grounded)
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
    use rapier2d::{
        control::KinematicCharacterController,
        prelude::{Pose, Vector},
    };

    #[test]
    fn cellular_terrain_builds_only_static_world_geometry() {
        let mut physics: ScenePhysicsWorld = ScenePhysicsWorld::new();
        physics.update_cellular_terrain(CollisionOccupancySnapshot {
            sequence: 0,
            origin: TileCoordinates { x: 0, y: 0 },
            width: 1,
            height: 1,
            static_masks: vec![[0b11, 0]].into_boxed_slice(),
            dynamic_masks: vec![[1 << 8, 1 << 31]].into_boxed_slice(),
        });
        let static_handle = physics.static_cellular_terrain.expect("static terrain collider");
        let static_parts = physics.rapier.colliders[static_handle].shape().as_compound()
            .expect("compound static terrain").shapes();
        assert!(static_parts.len() == 1);
        assert!(static_parts[0].1.as_cuboid().is_some_and(|cuboid| {
            cuboid.half_extents.x == 0.125 && cuboid.half_extents.y == 0.0625
        }));
    }

    #[test]
    fn every_actor_primitive_casts_directly_against_dynamic_cells() {
        for shape in [
            ActorCollisionShape::Circle { radius: 0.25 },
            ActorCollisionShape::Capsule { radius: 0.25, height: 0.75 },
            ActorCollisionShape::Rectangle { width: 0.5, height: 0.75 },
        ] {
            let mut physics = ScenePhysicsWorld::new();
            physics.update_cellular_terrain(CollisionOccupancySnapshot {
                sequence: 0,
                origin: TileCoordinates { x: 0, y: 0 },
                width: 1,
                height: 1,
                static_masks: vec![[0, 0]].into_boxed_slice(),
                dynamic_masks: vec![[0, 1 << 4]].into_boxed_slice(),
            });
            let movement = physics.move_character(
                &KinematicCharacterController::default(),
                1.0 / 60.0,
                shape,
                &Pose::translation(0.0, 0.5625),
                Vector::new(1.0, 0.0),
                |_| { },
            );
            assert!(movement.translation.x < 0.5);
        }
    }

    #[test]
    fn starting_overlap_allows_separating_dynamic_cell_motion() {
        let mut physics = ScenePhysicsWorld::new();
        physics.update_cellular_terrain(CollisionOccupancySnapshot {
            sequence: 0,
            origin: TileCoordinates { x: 0, y: 0 },
            width: 1,
            height: 1,
            static_masks: vec![[0, 0]].into_boxed_slice(),
            dynamic_masks: vec![[0, 1 << 4]].into_boxed_slice(),
        });
        let movement = physics.move_character(
            &KinematicCharacterController::default(),
            1.0 / 60.0,
            ActorCollisionShape::Circle { radius: 0.25 },
            &Pose::translation(0.5625, 0.5625),
            Vector::new(-0.25, 0.0),
            |_| { },
        );
        assert!(movement.translation.x < -0.2);
    }

}
