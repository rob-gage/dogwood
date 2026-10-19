+++
title = "Actors"
description = "Actor handles and the scene-owned registry used to spawn and access actor components."
weight = 50
[extra]
sources = ["engine_world/src/actors/actor.rs", "engine_world/src/actors/actor_allocator.rs", "engine_world/src/actors/actor_registry.rs", "engine_world/src/actors/actor_error.rs", "engine_world/src/actors/mod.rs", "engine/src/lib.rs"]
+++

Actors are scene-local handles to ECS (entity-component-system) entities. Game code uses `ActorRegistry` to spawn actors and to read or write their components. The types are exported under `engine::actors`, and the component types under `engine::actors::components`.

See [actor components](@/external-api/actor-components.md) for the component reference and [actors, cameras, and pawns](@/examples/actors-and-pawns.md) for a complete spawn example.

## Actor handles

An `Actor` is a copyable, ordered, hashable `u64` handle with a private field. It is scene-local: a registry maps the handles of currently loaded actors to ECS entities. An `Actor` remains a scene-local identity when it has no live entity, but registry lookups succeed only while it is loaded.

`Actor::new(identifier)` constructs a handle from any `u64`. It does not check uniqueness or register the handle, so prefer handles returned by `ActorRegistry::spawn`.

- The registry's allocator starts at zero and increments monotonically.
- Identifiers are not reused during that allocator's lifetime.
- Two independently constructed registries can allocate equal handles, so an `Actor` is not a globally unique identifier.

## ActorRegistry

`ActorRegistry` owns its ECS world and exposes actor operations rather than direct world access. A `Scene` owns a registry and exposes it mutably through `Scene::actor_registry_mutable`.

| Method | Behavior | Errors |
|---|---|---|
| `new()` | Creates an empty registry. | None |
| `spawn(components)` | Takes a Bevy `Bundle` (a group of components to insert together), allocates an identity, spawns it, and returns `Result<Actor, ActorError>`. | `ActorIdentifiersExhausted` |
| `is_loaded(actor)` | Returns whether the handle maps to a live entity. | None (returns `false`) |
| `component<T>(actor)` | Borrows one component immutably. | `ActorNotLoaded`, `ActorComponentMissing` |
| `component_mutable<T>(actor)` | Borrows a component mutably, for components whose Bevy `Component::Mutability` is `Mutable`. | `ActorNotLoaded`, `ActorComponentMissing` |
| `components<A, B>()` | Iterates over pairs of components on live entities, skipping entities missing either one. | None (filtered iterator) |
| `insert(actor, components)` | Inserts a bundle into a live actor, replacing existing components of those types. | `ActorNotLoaded` |
| `remove<T>(actor)` | Removes and returns one component. | `ActorNotLoaded`, `ActorComponentMissing` |
| `despawn(actor)` | Despawns the live actor. | `ActorNotLoaded` |

`spawn` inserts the identity component automatically. Identifier exhaustion is reported as an error rather than wrapping the counter.

## ActorError

`ActorError` has three variants:

- `ActorIdentifiersExhausted`
- `ActorNotLoaded { actor }`
- `ActorComponentMissing { actor }`

## ActorAllocator

`ActorAllocator` is public (`new()`, `allocate_actor() -> Result<ActorIdentityComponent, ActorError>`, `entity(actor) -> Result<Entity, ActorError>`, and a public `entities: HashMap<Actor, Entity>`), but `ActorRegistry` uses it internally and games do not need it. See [actor registry and identity](@/internals/actors/registry.md) for how it works.

## Rustdoc

{{ <rustdoc path="engine/actors/struct.Actor.html" text="Actor" /> }} ·
{{ <rustdoc path="engine/actors/struct.ActorRegistry.html" text="ActorRegistry" /> }} ·
{{ <rustdoc path="engine/actors/struct.ActorAllocator.html" text="ActorAllocator" /> }} ·
{{ <rustdoc path="engine/actors/enum.ActorError.html" text="ActorError" /> }}
