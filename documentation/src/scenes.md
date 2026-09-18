# Game Loop And Scenes

`GameApplication` owns the window, input polling, rendering, and calls into
your `Game`. Your game usually owns an `Option<Scene>` and exposes it through
the `scene` and `scene_mutable` callbacks. `Game::launch` is the normal startup
path.

## Scene Creation

Construct a temporary scene with `Scene::new` or `Scene::new_with_generator`.
Use `SceneData::load` with `Scene::load` or `load_with_generator` for a saved
world. Scene construction allocates the buffered simulation area, compiles
material tables, and initializes actors, collision, fluids, gases, reactions,
thermal state, and persistence resources.

`SceneSimulationConfiguration` controls capacities and simulation settings.
The generator supplies missing chunks and may provide initial actor spawns.
Generated content becomes ordinary scene state after creation; it is not a
second runtime authority.

## Update Lifecycle

The application passes elapsed wall time to `Scene::update`. The scene consumes
that time as fixed simulation ticks (currently 60 Hz with bounded catch-up),
applies queued edits, advances resident simulation, and schedules streaming or
readback work. `Game::actor_contacts` receives contact batches,
`Game::material_extractions` receives completed extraction batches,
`Game::pass_pointer_input` receives camera-transformed pointer state, and
`Game::update` receives the ordinary game callback after scene work.

Rendering and UI continue while gameplay is paused. `Game::is_paused` disables
world simulation; a pawn may opt into movement while paused with
`simulate_when_paused`.

```rust
impl Game for MyGame {
    const TITLE: &'static str = "My Game";

    fn camera(&self) -> Camera {
        Camera {
            width: 48.0,
            height: 27.0,
            zoom: 1.0,
            follow_acceleration: 0.0,
            follow_speed: 0.0,
            follow_distance_maximum: 0.0,
        }
    }

    fn is_paused(&self) -> bool {
        false
    }

    fn scene(&self) -> Option<&Scene> {
        self.scene.as_ref()
    }

    fn scene_mutable(&mut self) -> Option<&mut Scene> {
        self.scene.as_mut()
    }

    fn user_interface_context(&mut self) -> &mut UserInterfaceContext {
        &mut self.ui
    }
}
```

## Active And Buffered Areas

The scene maintains an active simulation area and a larger buffered resident
area. A possessed pawn normally drives the target. `request_area_around` can
request another center for a tool or scripted camera. Check
`is_position_resident` before acting on a streamed coordinate; a non-resident
world position is not queryable through active scene buffers.

## Actors Generated With A Region

`SceneGenerator::generate_actor_spawns_with_seed` may provide initial generic
physical actors for a newly generated region. The scene owns those actors after
generation. Actors leaving the buffered area are snapshotted and removed from
live ECS/Rapier state, then restored with the same stable `Actor` identity.
Generation is not called again for ordinary unload/reload.

See [Saving, Loading, And Streaming](persistence.md) for residency behavior
and [Scene Model](../internals/src/scenes/scene.md) for engine ownership.

## Asynchronous Material Extraction

Gameplay can remove matching authoritative material from a resident world
region without polling GPU state. `SceneRegion` currently supports circles in
world/tile units, and `MaterialFilter` supports any material, one identifier,
tags, and forms. Results are aggregated by material and delivered once through
`Game::material_extractions`; amounts use the stored normalized amount for the
underlying cellular cell, fluid particle, gas concentration, or rigid cell.
Material outside the resident simulation area is left untouched.

```rust
let request = scene.extract_materials(MaterialExtraction {
    region: SceneRegion::Circle { center, radius: 1.0 },
    filter: MaterialFilter::Material(stone),
})?;

fn material_extractions(&mut self, results: &[MaterialExtractionResult]) {
    for result in results {
        if result.request == self.collection_request {
            // result.materials contains only nonzero totals.
        }
    }
}
```
