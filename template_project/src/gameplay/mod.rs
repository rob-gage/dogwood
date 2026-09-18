use engine::physics::actors::{Actor, ActorContactEvent, ActorContactState};
use engine::physics::scenes::Scene;

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
