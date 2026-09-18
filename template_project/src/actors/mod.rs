use engine::physics::tiles::TileCoordinates;
use engine::physics::{
    actors::{Actor, ActorCollisionShape, ActorPhysicalConfiguration},
    scenes::{Scene, ScenePosition, SceneVelocity},
};

pub fn spawn_demo_squares(scene: &mut Scene) -> Vec<Actor> {
    (-12..=12)
        .step_by(3)
        .map(|x| {
            scene.actor_registry_mutable().spawn_physical_actor(
                ActorPhysicalConfiguration {
                    collision_shape: ActorCollisionShape::Rectangle {
                        width: 0.7,
                        height: 0.7,
                    },
                    color: [0.95, 0.25, 0.1, 1.0],
                    ..Default::default()
                },
                ScenePosition {
                    tile_coordinates: TileCoordinates { x, y: 3 },
                    x_offset: 0.5,
                    y_offset: 0.5,
                },
                SceneVelocity { x: 0.0, y: 0.0 },
            )
        })
        .collect()
}
