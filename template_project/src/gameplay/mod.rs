use engine::physics::{
    actors::{Actor, ActorContactEvent, ActorContactState},
    scenes::{
        MaterialExtraction, MaterialExtractionRequest, MaterialFilter, Scene, ScenePosition,
        SceneRegion,
    },
};
use engine::{
    GamePointerInput,
    graphics::{Color, SceneOverlay},
};

pub const COLLECTION_REACH: f32 = 1.0;
pub const COLLECTION_RADIUS: f32 = 0.7;

pub fn calculate_aim_direction(
    player: [f32; 2],
    pointer: Option<[f32; 2]>,
    current: [f32; 2],
) -> [f32; 2] {
    let Some(pointer) = pointer else {
        return current;
    };
    let difference = [pointer[0] - player[0], pointer[1] - player[1]];
    let length_squared = difference[0] * difference[0] + difference[1] * difference[1];
    if length_squared <= 0.0001 {
        return current;
    }
    let length = length_squared.sqrt();
    [difference[0] / length, difference[1] / length]
}

pub fn update_pointer_aim(
    scene: Option<&Scene>,
    pawn: Actor,
    input: &GamePointerInput,
    aim_direction: &mut [f32; 2],
) {
    let Some(pointer) = input.world_position else {
        return;
    };
    let Some(player) = scene
        .and_then(|scene| scene.actor_registry().get_position(pawn))
        .copied()
    else {
        return;
    };
    let player = player.world();
    *aim_direction = calculate_aim_direction(player, Some(pointer), *aim_direction);
}

pub fn collection_center(
    scene: &Scene,
    pawn: Actor,
    aim_direction: [f32; 2],
) -> Option<ScenePosition> {
    let player = scene.actor_registry().get_position(pawn)?.world();
    Some(ScenePosition::from_world([
        player[0] + aim_direction[0] * COLLECTION_REACH,
        player[1] + aim_direction[1] * COLLECTION_REACH,
    ]))
}

pub fn collection_overlay(
    scene: &Scene,
    pawn: Actor,
    aim_direction: [f32; 2],
) -> Option<SceneOverlay> {
    let center = collection_center(scene, pawn, aim_direction)?.world();
    Some(SceneOverlay::CircleOutline {
        center,
        radius: COLLECTION_RADIUS,
        color: Color::new_rgb(255, 235, 80),
    })
}

pub fn request_collection(
    scene: &mut Scene,
    pawn: Actor,
    aim_direction: [f32; 2],
) -> Option<MaterialExtractionRequest> {
    let center = collection_center(scene, pawn, aim_direction)?;
    scene
        .extract_materials(MaterialExtraction {
            region: SceneRegion::Circle {
                center,
                radius: COLLECTION_RADIUS,
            },
            filter: MaterialFilter::Any,
        })
        .ok()
}

pub fn collect_contacts(scene: &mut Scene, pawn: Actor, contacts: &[ActorContactEvent]) -> u32 {
    let mut collected = 0;
    for event in contacts {
        if event.state != ActorContactState::Started
            || !event.first.eq(&pawn) && !event.second.eq(&pawn)
        {
            continue;
        }
        let square = if event.first == pawn {
            event.second
        } else {
            event.first
        };
        if scene
            .actor_registry()
            .physical_configuration(square)
            .is_none_or(|configuration| configuration.color != crate::actors::DEMO_SQUARE_COLOR)
        {
            continue;
        }
        if scene.actor_registry().contains(square) {
            scene.actor_registry_mutable().despawn(square);
            collected += 1;
        }
    }
    collected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aim_is_normalized_and_zero_distance_keeps_previous_direction() {
        assert_eq!(
            calculate_aim_direction([0.0, 0.0], Some([2.0, 0.0]), [0.0, 1.0]),
            [1.0, 0.0]
        );
        assert_eq!(
            calculate_aim_direction([0.0, 0.0], Some([-2.0, 0.0]), [0.0, 1.0]),
            [-1.0, 0.0]
        );
        assert_eq!(
            calculate_aim_direction([0.0, 0.0], Some([0.0, 0.0]), [1.0, 0.0]),
            [1.0, 0.0]
        );
    }

    #[test]
    fn collection_center_has_fixed_reach() {
        let player = [2.0, -1.0];
        let center = [player[0] + COLLECTION_REACH, player[1]];
        assert_eq!(center, [3.0, -1.0]);
    }
}
