// Copyright Rob Gage 2026

use super::*;
use crate::scenes::{ScenePosition, SceneVelocity};
use crate::tiles::TileCoordinates;

#[test]
fn test_swimming_sample_uses_hysteresis_without_changing_other_modes() {
    let mut registry: ActorRegistry = ActorRegistry::new();
    let mut pawn: ActorPawn = ActorPawn::new();
    pawn.collision_shape = Some(ActorCollisionShape::Rectangle {
        width: 1.0,
        height: 1.0,
    });
    pawn.walking = Some(ActorPawnWalkingConfiguration {
        speed: 0.0,
        acceleration: 0.0,
        mass: 1.0,
        jump_velocity: 0.0,
        maximum_slope_angle: 0.0,
    });
    pawn.swimming = Some(ActorPawnSwimmingConfiguration {
        maximum_speed: 1.0,
        acceleration: 1.0,
        density: 1.0,
        drag: 1.0,
        enter_immersion: 0.6,
        exit_immersion: 0.4,
    });
    pawn.movement = Some(ActorPawnMovement::Walking);
    let actor: Actor = registry.spawn_pawn(
        pawn,
        ScenePosition {
            tile_coordinates: TileCoordinates { x: 0, y: 0 },
            x_offset: 0.0,
            y_offset: 0.0,
        },
        SceneVelocity { x: 0.0, y: 0.0 },
    );
    registry.apply_swimming_sample(actor, [0.6, 0.0, 0.0, 1.0, 1.0]);
    assert!(matches!(
        registry.get_pawn(actor).unwrap().movement,
        Some(ActorPawnMovement::Swimming)
    ));
    registry.apply_swimming_sample(actor, [0.5, 0.0, 0.0, 1.0, 1.0]);
    assert!(matches!(
        registry.get_pawn(actor).unwrap().movement,
        Some(ActorPawnMovement::Swimming)
    ));
    registry.apply_swimming_sample(actor, [0.39, 0.0, 0.0, 1.0, 1.0]);
    assert!(matches!(
        registry.get_pawn(actor).unwrap().movement,
        Some(ActorPawnMovement::Walking)
    ));
    assert!(registry.set_movement_for_test(actor, ActorPawnMovement::Flying));
    registry.apply_swimming_sample(actor, [1.0, 0.0, 0.0, 1.0, 1.0]);
    assert!(matches!(
        registry.get_pawn(actor).unwrap().movement,
        Some(ActorPawnMovement::Flying)
    ));
}
