+++
title = "Engine and Game"
description = "Construct the runtime and implement the callbacks that connect a game to Dogwood."
weight = 10
[extra]
sources = ["engine/src/runtime", "engine/src/games", "engine/src/lib.rs"]
+++

This page covers the `Engine` runtime and the `Game` trait that connects a game to it. It is for game programmers; `GameInstance`, at the end, is for hosts such as the editor.

## Launching the engine

`Engine` owns an [`Accelerator`](@/external-api/host.md) (the GPU context) and a [`Processor`](@/external-api/host.md) (named CPU thread pools). Construct one with `Engine::new(accelerator, processor)` or `Engine::default()`, and inspect its resources with `accelerator()` and `processor()`.

`launch(game)` consumes the engine and returns `Result<(), EngineError>`. It obtains the game's window configuration, constructs a `GameInstance`, connects the callbacks, and opens the host window.

```rust
use engine::{
    games::Game,
    runtime::{Engine, EngineError},
};

struct MyGame;

impl Game for MyGame {}

fn main() -> Result<(), EngineError> {
    Engine::default().launch(MyGame)
}
```

See [Your first project](@/getting-started/first-project.md) for a complete game.

`Engine::default()` initializes default GPU and CPU resources and panics if either initialization fails. Its default processor pools are described in [Host types](@/external-api/host.md).

## Implementing `Game`

All trait methods have defaults, so implementations override only the callbacks they need.

| Method | Default | Called |
| --- | --- | --- |
| `window_configuration(&self) -> WindowConfiguration` | `WindowConfiguration::default()` | Once during `Engine::launch`, before opening the window |
| `update(&mut self, elapsed_time: Duration)` | No operation | Once per fixed update, at 60 updates per second when the window loop catches up; elapsed time is one update interval |
| `scene(&mut self) -> Option<(&mut Scene, Actor)>` | `None` | During update, streaming, and rendering to obtain the current scene and its camera actor |
| `user_interface(&mut self, user_interface: &mut UserInterface)` | No operation | Every rendered frame to compose UI |
| `window_event(&mut self, event: SystemWindowEvent)` | No operation | When the host emits a window event |
| `keyboard_event(&mut self, event: SystemKeyboardEvent)` | No operation | When the host emits a keyboard event |
| `mouse_event(&mut self, event: SystemMouseEvent)` | No operation | When the host emits a mouse event |

The runtime caps accumulated time at six fixed update intervals after a delayed event-loop iteration. See [Frame and update lifecycle](@/architecture/frame-lifecycle.md) for the fixed-update loop.

### Camera requirement

The selected camera actor must have `ActorCameraComponent` and `ActorKinematicsPositionComponent` for streaming and drawing. It is the actor returned by `scene()` unless overridden with `GameInstance::set_camera_actor`. See [Scenes](@/external-api/scenes.md) and [Actor components](@/external-api/actor-components.md).

## Per-update behavior

Each fixed update runs these steps in order:

1. Query `scene()`. If it returns `Some`, apply held-key control to possessed pawns (actors with `ActorPawnPossessionComponent`; see [Actor components](@/external-api/actor-components.md)).
2. Call `Game::update`.
3. Query `scene()` again. If it returns `Some`, stream and simulate the scene.

`GameInstance` tracks `PhysicalKey::Code` press and release events and clears held keys on `FocusReleased`. It maps keys to pawn control as follows:

| Keys | Control |
| --- | --- |
| A/← and D/→ | Move left and right |
| W/↑ | Jump |
| S/↓ | Crouch |

## `GameInstance`

Most games never touch `GameInstance`; it exists for hosts such as the editor that embed a game's rendering and callbacks. It is public as `engine::games::GameInstance<G>`.

| Method | Purpose |
| --- | --- |
| `new(&Accelerator, game)` | Wrap a game |
| `game_mutable() -> &mut G` | Access the wrapped game |
| `user_interface_context() -> &UserInterfaceContext` | Access the UI context |
| `set_camera_actor(Option<Actor>)` | Override the camera actor |
| `window_event`, `keyboard_event`, `mouse_event` | Forward host events to the game |
| `update(&Accelerator, &Processor, Duration)` | Run one fixed update |
| `stream(&Accelerator, &Processor)` | Stream the scene around the camera |
| `render(&Accelerator, &wgpu::TextureView, egui::RawInput) -> egui::PlatformOutput` | Draw the scene and UI into a texture view |

- A camera set with `set_camera_actor` overrides the camera returned by `scene()` for streaming and drawing; `None` restores the game's camera.
- If `scene()` returns `None` during rendering, the frame is cleared to black and the game UI is still composed and drawn.
- The first `render` call fixes the UI renderer's texture format; later render targets must use the same format.
- The `egui` types are re-exported by `engine::user_interface`.

## Errors and panics

`EngineError` is reachable as `engine::runtime::EngineError`. Its variants carry `Accelerator(AcceleratorError)`, `Processor(ProcessorError)`, `Window(WindowError)`, `SpriteSheet(SpriteSheetError)`, `Material(MaterialError)`, and `Actor(ActorError)`, each with a `From` conversion.

`Engine::launch` returns only `EngineError::Window`; the other variants exist for `From` conversions when you build engine resources yourself.

These calls panic instead of returning errors:

- `update` panics if streaming or simulation fails; `stream` panics if streaming fails.
- `render` panics if scene graphics lookup fails.
- Inserting the pawn-control component on possessed actors panics if `ActorRegistry::insert` fails.

The `Game` callback methods themselves do not return errors.

{{ <rustdoc path="engine/runtime/struct.Engine.html" text="Engine" /> }} · {{ <rustdoc path="engine/runtime/enum.EngineError.html" text="EngineError" /> }} · {{ <rustdoc path="engine/games/trait.Game.html" text="Game" /> }} · {{ <rustdoc path="engine/games/struct.GameInstance.html" text="GameInstance" /> }}
