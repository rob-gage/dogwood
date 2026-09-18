# Actors and gameplay

`physics::actors::ActorRegistry` owns actor identifiers and the built-in actor
components. An ordinary actor has a `ScenePosition`; a pawn additionally has
movement, velocity, control, and optional collision state.

```rust
let actor = scene.actor_registry_mutable().spawn(position);
let pawn = ActorPawn {
    collision_shape: Some(ActorCollisionShape::Circle { radius: 0.375 }),
    walking: Some(ActorPawnWalkingConfiguration {
        speed: 4.0, acceleration: 24.0, mass: 8.0,
        jump_velocity: 7.0, maximum_slope_angle: 50.0_f32.to_radians(),
    }),
    movement: Some(ActorPawnMovement::Walking),
    ..ActorPawn::new()
};
let player = scene.actor_registry_mutable().spawn_possessable_pawn(
    pawn, position, velocity,
);
scene.possess_actor(player);
```

Use `spawn_pawn` for non-player pawns and `spawn_possessable_pawn` for player
candidates. Set controls through `set_control_state`; the default
`Game::pass_input` sends arrow/WASD controls to the possessed actor. Read
`get_position`, `get_velocity`, `get_pawn`, or `get_render_position`; remove
with `despawn`.

Actor simulation runs during the fixed scene tick. Walking uses scene collision,
swimming uses fluid interaction when configured, and noclip integrates directly.
`Actor` is a stable Dogwood identity; the underlying ECS entity is private and
may be reconstructed in a later streaming pass. Generic dynamic actors can be
spawned without Rapier types:

```rust
let square = scene.actor_registry_mutable().spawn_physical_actor(
    ActorPhysicalConfiguration {
        collision_shape: ActorCollisionShape::Rectangle { width: 0.7, height: 0.7 },
        color: [1.0, 0.2, 0.1, 1.0],
        ..Default::default()
    },
    position,
    SceneVelocity { x: 0.0, y: 0.0 },
);
```

The physics world synchronizes generic actor position and velocity back to the
registry. Logical actor contacts are delivered after `Scene::update` through
`Game::actor_contacts(&[ActorContactEvent])`; handle `Started` events there and
despawn through the registry. `Game::update(Duration)` runs afterward for
ordinary gameplay updates.

Generic physical actors outside the retained buffered scene area are kept as
engine-owned snapshots rather than live ECS or Rapier objects. Returning to a
resident region restores the same `Actor` identity, transform, velocity, and
physical configuration. Possessed actors remain resident. A
`SceneGenerator` can provide initial `ActorPhysicalSpawn` values for a region;
that hook is called only when the region is first generated, not on ordinary
unload/reload.
