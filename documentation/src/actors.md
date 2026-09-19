# Actors And Gameplay

`physics::actors::ActorRegistry` owns actor identifiers and the built-in actor
components. An ordinary actor has a `ScenePosition`; a pawn additionally has
movement, velocity, control, and optional collision state.

```rust
use dogwood_engine::physics::{
    actors::{
        Actor,
        ActorCollisionShape,
        ActorPawn,
        ActorPawnMovement,
        ActorPawnWalkingConfiguration,
        ActorPhysicalConfiguration,
    },
    scenes::{Scene, SceneVelocity},
};

let actor: Actor = scene.actor_registry_mutable().spawn(position);
let pawn: ActorPawn = ActorPawn {
    collision_shape: Some(ActorCollisionShape::Circle { radius: 0.375 }),
    walking: Some(ActorPawnWalkingConfiguration {
        speed: 4.0, acceleration: 24.0, mass: 8.0,
        jump_velocity: 7.0, maximum_slope_angle: 50.0_f32.to_radians(),
    }),
    movement: Some(ActorPawnMovement::Walking),
    ..ActorPawn::new()
};
let player: Actor = scene.actor_registry_mutable().spawn_possessable_pawn(
    pawn, position, velocity,
);
scene.possess_actor(player);
```

Actors can optionally carry `ActorSprites`. Sprite sheets are shared CPU-side
RGBA sources used by the world renderer. Add named horizontal animations, then
select them from gameplay code:

```rust
use dogwood_engine::physics::actors::{
    ActorSpriteAnimation, ActorSpriteSheet, ActorSprites,
};

let sprite_sheet: ActorSpriteSheet =
    ActorSpriteSheet::new(64, 16, vec![0; 64 * 16 * 4]).expect("valid RGBA sheet");
let animation: ActorSpriteAnimation = ActorSpriteAnimation::new(
    sprite_sheet, None, 16, 16, 4, 8.0, true,
).expect("valid animation");
let mut sprites: ActorSprites = ActorSprites::new();
let idle_animation = sprites.add_animation("idle", animation).unwrap();
scene.actor_registry_mutable().set_sprites(actor, sprites);
scene.actor_registry_mutable().sprites_mutable(actor)
    .unwrap().play(idle_animation);
```

Set `world_size` and `world_offset` through `set_world_size` and
`set_world_offset`; these control world placement independently of source
pixels and collision dimensions.

`play` does not restart an already selected animation; use `restart` to do so.
`pause`, `resume`, and `set_animation_speed` control playback without changing
the authored rate. A missing radiance sheet means an all-black RGBA radiance
sheet, with no per-actor black image allocation. Radiance RGB emits into the
scene; radiance alpha is preserved but currently ignored. Sprite sheets and frames may
have arbitrary positive dimensions; even dimensions are generally preferred
for authored assets but are not a runtime requirement.

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
let square: Actor = scene.actor_registry_mutable().spawn_physical_actor(
    ActorPhysicalConfiguration {
        collision_shape: ActorCollisionShape::Rectangle {
            width: 0.7,
            height: 0.7,
        },
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
