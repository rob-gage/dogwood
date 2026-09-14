// Copyright Rob Gage 2026

use super::{
    CollisionOccupancySnapshot,
    RigidCellularBody,
};
use rapier2d::{
    control::{
        CharacterCollision,
        EffectiveCharacterMovement,
        KinematicCharacterController,
    },
    prelude::{
        ColliderBuilder,
        ColliderHandle,
        PhysicsWorld,
        Pose,
        RigidBodyBuilder,
        RigidBodyHandle,
        Shape,
        SharedShape,
        Vector,
    },
};

/// Owns the Rapier collision world and its derived cellular terrain
pub struct ScenePhysicsWorld {
    rapier: PhysicsWorld,
    /// World-scale static cellular terrain, rebuilt only when static occupancy changes
    static_cellular_terrain: Option<ColliderHandle>,
    /// One small dynamic cellular collider near each pawn collision region
    dynamic_cellular_terrain: Vec<Option<ColliderHandle>>,
    /// The collision snapshot used to derive static and actor-local dynamic terrain
    cellular_terrain_snapshot: Option<CollisionOccupancySnapshot>,
    /// Dynamic snapshot sequence and pawn regions currently represented in Rapier
    dynamic_cellular_terrain_state: Option<(u64, Vec<[i32; 4]>)>,
}

impl ScenePhysicsWorld {

    /// Creates an empty scene collision world
    pub fn new() -> Self {
        Self {
            rapier: PhysicsWorld::new(),
            static_cellular_terrain: None,
            dynamic_cellular_terrain: Vec::new(),
            cellular_terrain_snapshot: None,
            dynamic_cellular_terrain_state: None,
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
                .restitution(restitution),
            Some(handle),
        );
        RigidCellularBody { handle, cells }
    }

    /// Returns transform and velocities needed to derive a body's world-cell proxy
    pub(crate) fn rigid_cellular_body_state(
        &self,
        body: &RigidCellularBody,
    ) -> Option<([f32; 2], f32, [f32; 2], f32, [f32; 2])> {
        let rigid_body = self.rapier.bodies.get(body.handle)?;
        let position = rigid_body.position();
        let center = rigid_body.center_of_mass();
        Some((
            [position.translation.x, position.translation.y],
            position.rotation.angle(),
            [rigid_body.linvel().x, rigid_body.linvel().y],
            rigid_body.angvel(),
            [center.x, center.y],
        ))
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
                false,
            );
            Self::replace_cellular_collider(
                &mut self.rapier,
                &mut self.static_cellular_terrain,
                shape,
            );
        }
        self.cellular_terrain_snapshot = Some(snapshot);
    }

    /// Updates dynamic cellular collision only inside current pawn query regions
    pub(crate) fn update_dynamic_cellular_terrain(&mut self, regions: &[[i32; 4]]) {
        let Some(snapshot) = self.cellular_terrain_snapshot.as_ref() else { return; };
        if self.dynamic_cellular_terrain_state.as_ref().is_some_and(|(sequence, current)| {
            *sequence == snapshot.sequence && current == regions
        }) { return; }
        let shapes: Vec<Option<SharedShape>> = regions.iter().map(|region| {
            Self::cellular_collision_shape(snapshot, *region, true)
        }).collect();
        while self.dynamic_cellular_terrain.len() > shapes.len() {
            if let Some(handle) = self.dynamic_cellular_terrain.pop().flatten() {
                self.rapier.remove_collider(handle);
            }
        }
        self.dynamic_cellular_terrain.resize_with(shapes.len(), || None);
        for (handle, shape) in self.dynamic_cellular_terrain.iter_mut().zip(shapes) {
            Self::replace_cellular_collider(&mut self.rapier, handle, shape);
        }
        self.dynamic_cellular_terrain_state = Some((snapshot.sequence, regions.to_vec()));
    }

    /// Builds greedy cellular rectangles inside one world-cell area
    fn cellular_collision_shape(
        snapshot: &CollisionOccupancySnapshot,
        area: [i32; 4],
        dynamic: bool,
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
        let occupied = |x: i32, y: i32| if dynamic {
            snapshot.is_dynamic_cell_occupied(x, y) == Some(true)
        } else {
            snapshot.is_static_cell_occupied(x, y) == Some(true)
        };
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
    ) {
        match (shape, *handle) {
            (Some(shape), Some(handle)) => rapier.colliders[handle].set_shape(shape),
            (Some(shape), None) => {
                *handle = Some(rapier.insert_collider(ColliderBuilder::new(shape).build(), None));
            }
            (None, Some(current)) => {
                rapier.remove_collider(current);
                *handle = None;
            }
            (None, None) => (),
        }
    }

    /// Advances Rapier's collision world by one fixed scene step
    pub fn step(&mut self, gravity: [f32; 2], delta_time: f32) {
        self.rapier.gravity = Vector::new(gravity[0], gravity[1]);
        self.rapier.integration_parameters.dt = delta_time;
        self.rapier.step();
    }

    /// Resolves a character shape's desired translation against the scene collision world
    pub fn move_character(
        &self,
        controller: &KinematicCharacterController,
        delta_time: f32,
        shape: &dyn Shape,
        position: &Pose,
        desired_translation: Vector,
        collisions: impl FnMut(CharacterCollision),
    ) -> EffectiveCharacterMovement {
        controller.move_shape(
            delta_time,
            &self.rapier.query_pipeline(),
            shape,
            position,
            desired_translation,
            collisions,
        )
    }

}

#[cfg(test)]
mod tests {
    use super::ScenePhysicsWorld;
    use crate::{
        simulation::CollisionOccupancySnapshot,
        tiles::TileCoordinates,
    };

    #[test]
    fn cellular_terrain_separates_static_world_from_actor_local_dynamic_cells() {
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

        physics.update_dynamic_cellular_terrain(&[[0, 1, 2, 2]]);
        let dynamic_handle = physics.dynamic_cellular_terrain[0]
            .expect("local dynamic terrain collider");
        let dynamic_parts = physics.rapier.colliders[dynamic_handle].shape().as_compound()
            .expect("compound dynamic terrain").shapes();
        assert!(dynamic_parts.len() == 1);
        assert!(dynamic_parts[0].0.translation.x == 0.0625);
        assert!(dynamic_parts[0].0.translation.y == 0.1875);
    }

}
