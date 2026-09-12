// Copyright Rob Gage 2026

use super::CollisionOccupancySnapshot;
use crate::tiles::TileCoordinates;
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
use std::collections::HashMap;

/// Owns the Rapier collision world and its derived cellular terrain
pub struct ScenePhysicsWorld {
    rapier: PhysicsWorld,
    cellular_terrain: HashMap<TileCoordinates, Vec<ColliderHandle>>,
    cellular_terrain_masks: HashMap<TileCoordinates, [u32; 2]>,
}

impl ScenePhysicsWorld {

    /// Creates an empty scene collision world
    pub fn new() -> Self {
        Self {
            rapier: PhysicsWorld::new(),
            cellular_terrain: HashMap::new(),
            cellular_terrain_masks: HashMap::new(),
        }
    }

    /// Updates only fixed Rapier terrain tiles whose occupancy or residency changed
    pub fn update_cellular_terrain(
        &mut self,
        snapshot: CollisionOccupancySnapshot,
    ) {
        let maximum_x: i32 = snapshot.origin.x + i32::from(snapshot.width);
        let maximum_y: i32 = snapshot.origin.y + i32::from(snapshot.height);
        let removed: Vec<TileCoordinates> = self.cellular_terrain.keys()
            .filter(|coordinates| {
                coordinates.x < snapshot.origin.x || coordinates.x >= maximum_x ||
                    coordinates.y < snapshot.origin.y || coordinates.y >= maximum_y
            }).copied().collect();
        for coordinates in removed { self.remove_cellular_terrain_tile(coordinates); }
        self.cellular_terrain_masks.retain(|coordinates, _| {
            coordinates.x >= snapshot.origin.x && coordinates.x < maximum_x &&
                coordinates.y >= snapshot.origin.y && coordinates.y < maximum_y
        });
        let width: usize = usize::from(snapshot.width);
        for (index, masks) in snapshot.masks.into_vec().into_iter().enumerate() {
            let coordinates: TileCoordinates = TileCoordinates {
                x: snapshot.origin.x + (index % width) as i32,
                y: snapshot.origin.y + (index / width) as i32,
            };
            if self.cellular_terrain_masks.get(&coordinates) == Some(&masks) { continue; }
            self.remove_cellular_terrain_tile(coordinates);
            self.cellular_terrain_masks.insert(coordinates, masks);
            self.insert_cellular_terrain_tile(coordinates, masks);
        }
    }

    /// Removes every collider derived from one world tile
    fn remove_cellular_terrain_tile(&mut self, coordinates: TileCoordinates) {
        if let Some(handles) = self.cellular_terrain.remove(&coordinates) {
            for handle in handles { self.rapier.remove_collider(handle); }
        }
    }

    /// Builds fixed colliders for one occupied 8x8 world tile
    fn insert_cellular_terrain_tile(
        &mut self,
        coordinates: TileCoordinates,
        masks: [u32; 2],
    ) {
        if masks == [0; 2] { return; }
        let mut handles: Vec<ColliderHandle> = Vec::new();
        let mut consumed: [bool; 64] = [false; 64];
        for relative_y in 0..8 {
            let mut relative_x: usize = 0;
            while relative_x < 8 {
                let index: usize = relative_y * 8 + relative_x;
                if consumed[index] || masks[index / 32] & (1 << (index % 32)) == 0 {
                    relative_x += 1;
                    continue;
                }
                let rectangle_x: usize = relative_x;
                let mut rectangle_width: usize = 1;
                while rectangle_x + rectangle_width < 8 {
                    let index: usize = relative_y * 8 + rectangle_x + rectangle_width;
                    if consumed[index] || masks[index / 32] & (1 << (index % 32)) == 0 { break; }
                    rectangle_width += 1;
                }
                let mut rectangle_height: usize = 1;
                while relative_y + rectangle_height < 8 &&
                        (rectangle_x..rectangle_x + rectangle_width).all(|x| {
                            let index: usize = (relative_y + rectangle_height) * 8 + x;
                            !consumed[index] && masks[index / 32] & (1 << (index % 32)) != 0
                        }) {
                    rectangle_height += 1;
                }
                for y in relative_y..relative_y + rectangle_height {
                    for x in rectangle_x..rectangle_x + rectangle_width {
                        consumed[y * 8 + x] = true;
                    }
                }
                let world_x: i32 = coordinates.x * 8 + rectangle_x as i32;
                let world_y: i32 = coordinates.y * 8 + relative_y as i32;
                handles.push(self.rapier.insert_collider(
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
        self.cellular_terrain.insert(coordinates, handles);
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