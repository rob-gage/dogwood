+++
title = "Actor registry and identity"
description = "How scene-local actor handles map to Bevy entities and how the registry manages components."
weight = 10
[extra]
sources = ["engine_world/src/actors/actor.rs", "engine_world/src/actors/actor_allocator.rs", "engine_world/src/actors/actor_registry.rs", "engine_world/src/actors/components/actor_identity.rs", "engine_world/src/actors/actor_error.rs", "engine/src/lib.rs"]
+++

This page is for engine contributors. It explains how an actor handle becomes a Bevy entity, how identifiers are allocated, and where the identity bookkeeping can go wrong.

## Actor handles

An {{ <rustdoc path="engine/actors/struct.Actor.html" text="Actor" /> }} is a copyable scene-local handle containing a `u64`. The field is private and there is no public getter.

- `Actor::new(identifier)` is public, so handles are forgeable. A constructed handle resolves to whichever actor currently owns that identifier; `Actor::new(0)` can alias the first actor spawned in a registry.
- A handle remains a value after despawn, but it resolves only while its actor is loaded in that scene's registry. See [actor lifetime](@/internals/actors/actor-lifetime.md) for region-based removal.

## Registry and allocator

Each {{ <rustdoc path="engine/actors/struct.ActorRegistry.html" text="ActorRegistry" /> }} owns a Bevy `World` and an {{ <rustdoc path="engine/actors/struct.ActorAllocator.html" text="ActorAllocator" /> }} resource.

- The allocator starts at identifier zero and increments an atomic counter with checked addition. Identifiers are not reused within an allocator.
- `ActorRegistry::spawn` allocates an `ActorIdentityComponent`, inserts it with the supplied Bevy bundle, and returns the handle.

### Identity hooks

The `ActorIdentityComponent` hooks maintain the allocator's public `entities` map of `Actor -> Entity`:

- The insert hook adds the mapping.
- The discard hook removes it when the component is removed, replaced, or its entity is despawned.

The registry uses this map to resolve handles for component access and mutation.

## Registry operations

| Operation | Behavior |
|---|---|
| `spawn`, `despawn`, `is_loaded` | Create an actor, remove it, or test whether a handle resolves. |
| `component`, `component_mutable` | Typed read or mutable access to one component. |
| `insert` | Adds a component, replacing an existing one of the same type. |
| `remove` | Removes a component and returns it. |
| `components<A, B>` | Iterates pairs for entities that have both components. |

## Errors

These variants are {{ <rustdoc path="engine/actors/enum.ActorError.html" text="ActorError" /> }}:

- `ActorIdentifiersExhausted`: allocation failed.
- `ActorNotLoaded`: a result-returning operation received an unknown or unloaded handle. `is_loaded` returns `false` instead.
- `ActorComponentMissing`: the requested component is absent.

## Identity hazards

> **Limitation:** The identity hooks do not check for an existing mapping before inserting one. The normal `spawn` path allocates unique identifiers, but `ActorIdentityComponent::new` is public, and `ActorRegistry::insert` and `remove` can manipulate identity components. Manually reassigning identities can overwrite or remove mappings that lookups depend on.

Removing an identity does not despawn its entity. `remove::<ActorIdentityComponent>` removes only the component and its mapping. The entity's other components remain available to `components<A, B>` iteration; use `despawn` to remove the whole entity.

For component ownership and system consumers, see [Who reads each component](@/internals/actors/component-consumers.md).
