// Copyright Rob Gage 2026

use super::CollisionOccupancySnapshot;
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
        Shape,
        Vector,
    },
};

/// Owns the Rapier collision world and its derived cellular terrain
pub struct ScenePhysicsWorld {
    rapier: PhysicsWorld,
    cellular_terrain: Vec<ColliderHandle>,
    cellular_terrain_snapshot: Option<CollisionOccupancySnapshot>,
}

impl ScenePhysicsWorld {

    /// Creates an empty scene collision world
    pub fn new() -> Self {
        Self {
            rapier: PhysicsWorld::new(),
            cellular_terrain: Vec::new(),
            cellular_terrain_snapshot: None,
        }
    }

    /// Replaces fixed Rapier terrain when cellular occupancy or its logical layout changes
    pub fn update_cellular_terrain(
        &mut self,
        snapshot: CollisionOccupancySnapshot,
    ) {
        if self.cellular_terrain_snapshot.as_ref().is_some_and(|current| {
            current.origin == snapshot.origin && current.width == snapshot.width &&
                current.height == snapshot.height && current.masks == snapshot.masks
        }) { return; }
        for handle in self.cellular_terrain.drain(..) {
            self.rapier.remove_collider(handle);
        }
        let cell_width: i32 = i32::from(snapshot.width) * 8;
        let cell_height: i32 = i32::from(snapshot.height) * 8;
        let origin_x: i32 = snapshot.origin.x * 8;
        let origin_y: i32 = snapshot.origin.y * 8;
        let mut consumed: Vec<bool> = vec![false; (cell_width * cell_height) as usize];
        for relative_y in 0..cell_height {
            let mut relative_x: i32 = 0;
            while relative_x < cell_width {
                let index: usize = (relative_y * cell_width + relative_x) as usize;
                if consumed[index] || snapshot.is_cell_occupied(
                    origin_x + relative_x,
                    origin_y + relative_y,
                ) != Some(true) {
                    relative_x += 1;
                    continue;
                }
                let rectangle_x: i32 = relative_x;
                let mut rectangle_width: i32 = 1;
                while rectangle_x + rectangle_width < cell_width {
                    let next_x: i32 = rectangle_x + rectangle_width;
                    let next_index: usize = (relative_y * cell_width + next_x) as usize;
                    if consumed[next_index] || snapshot.is_cell_occupied(
                        origin_x + next_x,
                        origin_y + relative_y,
                    ) != Some(true) { break; }
                    rectangle_width += 1;
                }
                let mut rectangle_height: i32 = 1;
                while relative_y + rectangle_height < cell_height &&
                        (rectangle_x..rectangle_x + rectangle_width).all(|x| {
                            let index: usize =
                                ((relative_y + rectangle_height) * cell_width + x) as usize;
                            !consumed[index] && snapshot.is_cell_occupied(
                                origin_x + x,
                                origin_y + relative_y + rectangle_height,
                            ) == Some(true)
                        }) {
                    rectangle_height += 1;
                }
                for y in relative_y..relative_y + rectangle_height {
                    for x in rectangle_x..rectangle_x + rectangle_width {
                        consumed[(y * cell_width + x) as usize] = true;
                    }
                }
                let world_x: i32 = origin_x + rectangle_x;
                let world_y: i32 = origin_y + relative_y;
                self.cellular_terrain.push(self.rapier.insert_collider(
                    ColliderBuilder::cuboid(
                        rectangle_width as f32 / 16.0,
                        rectangle_height as f32 / 16.0,
                    )
                        .translation(Vector::new(
                            world_x as f32 / 8.0 + rectangle_width as f32 / 16.0,
                            world_y as f32 / 8.0 + rectangle_height as f32 / 16.0,
                        ))
                        .build(),
                    None,
                ));
                relative_x += rectangle_width;
            }
        }
        self.cellular_terrain_snapshot = Some(snapshot);
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
