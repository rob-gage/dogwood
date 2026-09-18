# Actors and gameplay

`physics::actors::ActorRegistry` owns actor identifiers and the built-in actor components. An ordinary actor has a `ScenePosition`; a pawn additionally has movement, velocity, control, and optional collision state.

```rust
let actor = scene.actor_registry_mutable().spawn(position);
let pawn = ActorPawn { collision_shape: Some(ActorCollisionShape::Circle { radius: 0.375 }),
    walking: Some(ActorPawnWalkingConfiguration { speed: 4.0, acceleration: 24.0,
        mass: 8.0, jump_velocity: 7.0, maximum_slope_angle: 50.0_f32.to_radians() }),
    movement: Some(ActorPawnMovement::Walking), ..ActorPawn::new() };
let player = scene.actor_registry_mutable().spawn_possessable_pawn(pawn, position, velocity);
scene.possess_actor(player);
```

Use `spawn_pawn` for non-player pawns and `spawn_possessable_pawn` for player candidates. Set controls through `set_control_state`; the default `Game::pass_input` sends arrow/WASD controls to the possessed actor. Read `get_position`, `get_velocity`, `get_pawn`, or `get_render_position`; remove with `despawn`.

Actor simulation runs during the fixed scene tick. Walking uses scene collision, swimming uses fluid interaction when configured, and noclip integrates directly. There is no public generic gameplay ECS query API; keep game rules in your own structures and use the registry as the actor boundary.
