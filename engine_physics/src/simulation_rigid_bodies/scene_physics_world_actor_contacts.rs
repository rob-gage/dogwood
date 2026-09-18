// Copyright Rob Gage 2026

use std::collections::HashMap;
use std::collections::HashSet;

use rapier2d::prelude::ColliderHandle;
use rapier2d::prelude::Vector;

use super::ScenePhysicsWorld;
use crate::actors::Actor;
use crate::actors::ActorContactEvent;
use crate::actors::ActorContactState;

impl ScenePhysicsWorld {
    pub(crate) fn physical_proxy_states(&self) -> Vec<(Actor, [f32; 2], [f32; 2])> {
        self.physical_proxies
            .iter()
            .filter_map(|(actor, proxy)| {
                let physical_rigid_body: &rapier2d::dynamics::RigidBody =
                    self.rapier.bodies.get(proxy.body)?;
                let physical_rigid_body_position: rapier2d::math::Pose =
                    physical_rigid_body.position().clone();
                let physical_rigid_body_velocity: Vector = physical_rigid_body.linvel();
                Some((
                    *actor,
                    [
                        physical_rigid_body_position.translation.x,
                        physical_rigid_body_position.translation.y,
                    ],
                    [
                        physical_rigid_body_velocity.x,
                        physical_rigid_body_velocity.y,
                    ],
                ))
            })
            .collect()
    }

    pub(crate) fn actor_contact_events(&mut self) -> Vec<ActorContactEvent> {
        let mut actors_by_collider: HashMap<ColliderHandle, Actor> = HashMap::new();
        for (actor, proxy) in self.pawn_proxies.iter().chain(self.physical_proxies.iter()) {
            actors_by_collider.insert(proxy.collider, *actor);
        }
        let current_actor_contact_pairs: HashSet<(u64, u64)> = self
            .rapier
            .contact_pairs()
            .filter(|pair| pair.has_any_active_contact())
            .filter_map(|pair| {
                let first_actor_identifier: u64 =
                    actors_by_collider.get(&pair.collider1)?.stable_identifier();
                let second_actor_identifier: u64 =
                    actors_by_collider.get(&pair.collider2)?.stable_identifier();
                (first_actor_identifier != second_actor_identifier).then_some(
                    if first_actor_identifier < second_actor_identifier {
                        (first_actor_identifier, second_actor_identifier)
                    } else {
                        (second_actor_identifier, first_actor_identifier)
                    },
                )
            })
            .collect();
        let mut actor_contact_events: Vec<ActorContactEvent> = Vec::new();
        for &(first_actor_identifier, second_actor_identifier) in
            current_actor_contact_pairs.difference(&self.actor_contacts)
        {
            actor_contact_events.push(ActorContactEvent {
                first: Actor::new(first_actor_identifier),
                second: Actor::new(second_actor_identifier),
                state: ActorContactState::Started,
            });
        }
        for &(first_actor_identifier, second_actor_identifier) in
            self.actor_contacts.difference(&current_actor_contact_pairs)
        {
            actor_contact_events.push(ActorContactEvent {
                first: Actor::new(first_actor_identifier),
                second: Actor::new(second_actor_identifier),
                state: ActorContactState::Ended,
            });
        }
        self.actor_contacts = current_actor_contact_pairs;
        actor_contact_events
    }
}
