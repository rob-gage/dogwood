# Game loop and scenes

`SceneGenerator::generate_actor_spawns_with_seed` may provide initial generic
physical actors for a newly generated region. The scene owns those actors after
generation: runtime movement and despawns are authoritative. Actors that leave
the retained buffered area are snapshotted and removed from ECS/Rapier, then
restored with the same stable `Actor` when their region returns.

`GameApplication` owns the window, input polling, rendering, and fixed-rate
scene updates. `Game::launch` is the normal entry point. The engine calls
`Scene::update(elapsed, is_simulation_active)`; it handles streaming, queued
edits, and fixed simulation ticks (currently 60 Hz with bounded catch-up).

Gameplay normally owns an optional scene:

```rust
impl Game for MyGame {
    const TITLE: &'static str = "My Game";
    fn camera(&self) -> Camera {
        Camera {
            width: 48.0, height: 27.0, zoom: 1.0,
            follow_acceleration: 0.0, follow_speed: 0.0,
            follow_distance_maximum: 0.0,
        }
    }
    fn is_paused(&self) -> bool { false }
    fn scene(&self) -> Option<&Scene> { self.scene.as_ref() }
    fn scene_mutable(&mut self) -> Option<&mut Scene> { self.scene.as_mut() }
    fn user_interface_context(&mut self) -> &mut UserInterfaceContext {
        &mut self.ui
    }
}
```

Construct a temporary scene with `Scene::new` or `Scene::new_with_generator`, or
load persistent `SceneData` with `Scene::load`/`load_with_generator`. A
possessed actor causes the active area to follow it; `request_area_around` can
request another center.

Use `is_position_resident` before acting on a streamed position. A scene has an
active area and a larger buffered area; gameplay should not assume every world
coordinate is resident.
