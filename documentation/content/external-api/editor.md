+++
title = "Editor crate"
description = "Launch a game in the Dogwood editor using its public Editor entry point."
weight = 110
[extra]
sources = ["editor/src/lib.rs", "editor/src/editor.rs", "engine/src/runtime/engine.rs", "template/Cargo.toml", "template/src/bin/editor.rs", "cli/src/commands/edit.rs"]
+++

## Public entry point

The `editor` crate re-exports `Editor<G>` at its crate root. Use `Editor::launch` instead of `Engine::launch` to run the same `Game` inside the editor.

Its only inherent public method is `Editor::launch`; helper modules (brushes, tools, viewport) are private and not re-exported. `Editor<G>` also implements `egui_dock::TabViewer` with `EditorTab` as its `Tab` associated type.

```rust
use editor::Editor;
use engine::{
    games::Game,
    runtime::{Engine, EngineError},
};

struct MyGame;

impl Game for MyGame {}

fn main() -> Result<(), EngineError> {
    Editor::launch(Engine::default(), MyGame)
}
```

The signature is `Editor::launch(engine: Engine, game: G) -> Result<(), EngineError>`, where `G: engine::games::Game`. It consumes both values, uses the game's `window_configuration()`, wraps the game in `GameInstance`, and opens the host window and event loop. A window failure is converted into `EngineError::Window`.

`Engine::default()` creates the default accelerator and processor resources and panics if either initialization fails. Applications that need explicit setup can construct an `Engine` through its runtime API. See {{ <rustdoc path="editor/struct.Editor.html" text="Editor" /> }} and [Engine and Game](@/external-api/engine-and-games.md).

### Initial state

- **Layout:** the editor opens with viewport and tools tabs.
- **Simulation:** it starts paused. Streaming of the game continues while paused, and the Play control enables game updates and simulation.
- **Initial tool:** the first cellular static material by identifier, else the first cellular dynamic material, else the eraser (also used when the game has no scene).

## Template project wiring

The generated template declares an `editor` feature that enables its optional `editor` dependency. Its `editor` binary requires that feature and constructs the engine and game before calling `Editor::launch`:

```toml
[features]
editor = ["dep:editor"]

[[bin]]
name = "editor"
path = "src/bin/editor.rs"
required-features = ["editor"]
```

The template's editor entry point builds `Engine::default()`, creates its `TemplateGame` with the engine accelerator, then launches it. The [`dogwood edit`](@/external-api/cli.md) command builds and runs the `editor` binary with `--features editor`. For interactive usage, see [Using the editor](@/examples/using-the-editor.md).

> **Limitation:** The crate exposes no public editor configuration, tool, brush, or viewport types. Its UI and editing behavior are internal to `Editor`.
