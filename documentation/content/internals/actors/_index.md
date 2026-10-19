+++
title = "Actors and systems"
description = "Actor identity, component consumers, pawn movement, and actor lifetime in the active region."
weight = 30
sort_by = "weight"
template = "section.html"
[extra]
sources = [
    "engine_world/src/actors/actor_registry.rs",
    "engine_world/src/actors/components/actor_persistence.rs",
    "engine_world/src/scenes/collisions/collision.rs",
]
+++

These pages are for engine contributors. They describe how scene actors, stored as Bevy ECS entities behind an [`ActorRegistry`](@/internals/actors/registry.md), are identified, consumed by engine systems, moved, and removed when they leave the [active region](@/architecture/glossary.md) unless marked persistent.

- [Actor registry and identity](@/internals/actors/registry.md): handles, id allocation, and lookups.
- [Who reads each component](@/internals/actors/component-consumers.md): which system uses which component.
- [Pawn movement](@/internals/actors/pawn-movement.md): the character controller.
- [Actor lifetime in the active region](@/internals/actors/actor-lifetime.md): region exit and persistence.

For the simulations these systems feed, see the [Rapier collision world](@/internals/simulation/collision-world.md) and [cell and body contact exchange](@/internals/simulation/contact-exchange.md).
