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
    cellular_terrain_sequence: Option<u64>,
}

impl ScenePhysicsWorld {

    /// Creates an empty scene collision world
    pub fn new() -> Self {
        Self {
            rapier: PhysicsWorld::new(),
            cellular_terrain: Vec::new(),
            cellular_terrain_sequence: None,
        }
    }

    /// Replaces fixed Rapier terrain when a newer cellular occupancy snapshot is available
    pub fn update_cellular_terrain(
        &mut self,
        snapshot: Option<&CollisionOccupancySnapshot>,
    ) {
        let Some(snapshot) = snapshot else { return; };
        if self.cellular_terrain_sequence.is_some_and(|sequence| {
            sequence >= snapshot.sequence
        }) { return; }
        for handle in self.cellular_terrain.drain(..) {
            self.rapier.remove_collider(handle);
        }
        let cell_width: i32 = i32::from(snapshot.width) * 8;
        let cell_height: i32 = i32::from(snapshot.height) * 8;
        let origin_x: i32 = snapshot.origin.x * 8;
        let origin_y: i32 = snapshot.origin.y * 8;
        for relative_y in 0..cell_height {
            let world_y: i32 = origin_y + relative_y;
            let mut relative_x: i32 = 0;
            while relative_x < cell_width {
                let world_x: i32 = origin_x + relative_x;
                if snapshot.is_cell_occupied(world_x, world_y) != Some(true) {
                    relative_x += 1;
                    continue;
                }
                let run_start: i32 = world_x;
                relative_x += 1;
                while relative_x < cell_width && snapshot.is_cell_occupied(
                    origin_x + relative_x,
                    world_y,
                ) == Some(true) {
                    relative_x += 1;
                }
                let run_end: i32 = origin_x + relative_x;
                self.cellular_terrain.push(self.rapier.insert_collider(
                    ColliderBuilder::cuboid((run_end - run_start) as f32 / 16.0, 1.0 / 16.0)
                        .translation(Vector::new(
                            (run_start + run_end) as f32 / 16.0,
                            world_y as f32 / 8.0 + 1.0 / 16.0,
                        ))
                        .build(),
                    None,
                ));
            }
        }
        self.cellular_terrain_sequence = Some(snapshot.sequence);
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
