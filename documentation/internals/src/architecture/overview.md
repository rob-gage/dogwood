# Architecture

Dogwood is a windowed Rust application host around a shared `wgpu` device. The
`engine` crate owns the application contract and render orchestration. The
physics crate owns the scene and most simulation state. Compute resources are
shared by rendering and simulation through one `Accelerator`.

At the highest level:

```text
Game -> GameApplication -> Scene::update
  |          |                 |
  |          +-- input         +-- CPU streaming/persistence
  |                            +-- GPU simulation passes
  +-- UI composition                 |
                                     +-- SceneGraphics -> SceneRenderer
                                     +-- UI output -> UserInterfaceRenderer
```

`Scene` is the ownership boundary for a loaded world. It contains CPU-side
chunks, actors, rigid-body state, and filesystem coordination, plus the
Accelerator buffers for resident cellular, fluid, gas, thermal, and collision
state. Subsystems borrow those buffers while encoding work; they do not own the
world independently.

The normal data direction is CPU authoring/persistence -> resident GPU state ->
derived GPU results -> bounded CPU readback -> authoritative CPU or persistent
state. Some data is deliberately GPU-authoritative while resident, especially
fluid particles and gas fields. The streaming layer makes those choices
explicit when a region leaves the resident ring.

### Relevant implementation

- `engine/src/lib.rs` — public facade and subsystem re-exports.
- `engine/src/games/game.rs` — game contract and launch path.
- `engine/src/games/game_application.rs` — application update and render loop.
- `engine_physics/src/scenes/scene.rs` — aggregate scene ownership boundary.
- `engine_physics/src/scenes/scene_update.rs` — per-update and fixed-tick order.
- `engine_compute/src/accelerator.rs` — shared device, queue, and polling.
