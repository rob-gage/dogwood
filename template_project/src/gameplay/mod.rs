use engine::physics::{
    actors::{Actor, ActorContactState},
    scenes::Scene,
};

pub fn collect_contacts(scene: &mut Scene, pawn: Actor, squares: &mut Vec<Actor>) -> u32 {
    let mut collected = 0;
    for event in scene.drain_actor_contact_events() {
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
        if let Some(index) = squares.iter().position(|actor| *actor == square) {
            squares.swap_remove(index);
            scene.actor_registry_mutable().despawn(square);
            collected += 1;
        }
    }
    collected
}
