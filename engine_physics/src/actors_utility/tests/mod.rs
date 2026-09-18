// Copyright Rob Gage 2026

use crate::actors::Actor;
use crate::actors::ActorPawn;
use crate::actors::ActorPawnMovement;
use crate::actors::ActorPawnSwimmingConfiguration;
use crate::actors::ActorPawnWalkingConfiguration;
use crate::actors::ActorPhysicalConfiguration;
use crate::actors_utility::ActorCollisionShape;
use crate::actors_utility::ActorRegistry;
use crate::scenes::ScenePosition;
use crate::scenes::SceneVelocity;
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

#[test]
fn test_actor_identity_is_stable_and_not_an_ecs_entity() {
    let mut registry = ActorRegistry::new();
    let first = registry.spawn(ScenePosition {
        tile_coordinates: TileCoordinates { x: 0, y: 0 },
        x_offset: 0.0,
        y_offset: 0.0,
    });
    let second = registry.spawn(ScenePosition {
        tile_coordinates: TileCoordinates { x: 1, y: 0 },
        x_offset: 0.0,
        y_offset: 0.0,
    });
    assert_ne!(first, second);
    assert!(registry.despawn(first));
    let replacement = registry.spawn(ScenePosition {
        tile_coordinates: TileCoordinates { x: 2, y: 0 },
        x_offset: 0.0,
        y_offset: 0.0,
    });
    assert_ne!(first, replacement);
    assert!(registry.contains(second));
}

#[test]
fn test_physical_actor_can_be_spawned_and_despawned() {
    let mut registry = ActorRegistry::new();
    let actor = registry.spawn_physical_actor(
        ActorPhysicalConfiguration::default(),
        ScenePosition {
            tile_coordinates: TileCoordinates { x: 0, y: 2 },
            x_offset: 0.5,
            y_offset: 0.5,
        },
        SceneVelocity { x: 0.0, y: 0.0 },
    );
    assert!(registry.contains(actor));
    assert!(registry.despawn(actor));
    assert!(!registry.contains(actor));
}

#[test]
fn test_unloaded_actor_queries_are_safe_and_restore_keeps_identity() {
    let mut registry = ActorRegistry::new();
    let actor = registry.spawn_physical_actor(
        ActorPhysicalConfiguration::default(),
        ScenePosition {
            tile_coordinates: TileCoordinates { x: 0, y: 0 },
            x_offset: 0.5,
            y_offset: 0.5,
        },
        SceneVelocity { x: 1.0, y: 0.0 },
    );
    let snapshot = registry.physical_snapshot(actor).unwrap();
    assert!(registry.despawn(actor));
    assert!(!registry.contains(actor));
    assert!(registry.get_position(actor).is_none());
    assert!(registry.get_velocity(actor).is_none());
    assert!(!registry.set_position(actor, snapshot.position));
    assert!(!registry.set_velocity(actor, snapshot.velocity));
    assert!(registry.restore_physical_snapshot(snapshot));
    assert!(registry.contains(actor));
    assert_eq!(registry.get_velocity(actor).unwrap().x, 1.0);
    let restored = registry.get_render_position(actor, 0.5).unwrap();
    assert_eq!(
        restored.tile_coordinates.x,
        snapshot.position.tile_coordinates.x
    );
    assert_eq!(
        restored.tile_coordinates.y,
        snapshot.position.tile_coordinates.y
    );
    assert_eq!(restored.x_offset, snapshot.position.x_offset);
    assert_eq!(restored.y_offset, snapshot.position.y_offset);
    let replacement = registry.spawn(ScenePosition {
        tile_coordinates: TileCoordinates { x: 1, y: 0 },
        x_offset: 0.0,
        y_offset: 0.0,
    });
    assert_ne!(replacement, actor);
}

#[test]
fn test_physical_actor_interpolation_tracks_consecutive_fixed_states() {
    let mut registry = ActorRegistry::new();
    let actor = registry.spawn_physical_actor(
        ActorPhysicalConfiguration::default(),
        ScenePosition {
            tile_coordinates: TileCoordinates { x: 0, y: 3 },
            x_offset: 0.5,
            y_offset: 0.5,
        },
        SceneVelocity { x: 0.0, y: 0.0 },
    );
    registry.apply_physical_proxy_states(&[(actor, [0.5, 2.5], [0.0, -1.0])]);
    registry.apply_physical_proxy_states(&[(actor, [0.5, 2.0], [0.0, 0.0])]);
    let rendered = registry.get_render_position(actor, 0.5).unwrap();
    assert!((rendered.tile_coordinates.y as f32 + rendered.y_offset - 2.25).abs() < f32::EPSILON);
}
