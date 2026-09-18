# Five-minute quickstart

The template already has the minimum runnable shape. A game implements `Game`,
creates a shared `compute::Accelerator`, then launches.

The canonical launch path is the consuming `Game::launch` method:

```rust
use std::sync::Arc;
use dogwood_engine::{Game, compute::Accelerator};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    MyGame::new()?.launch(Arc::new(Accelerator::new()?))
}
```

Implement `TITLE`, `camera`, `is_paused`, `scene`, `scene_mutable`, and
`user_interface_context`. The template’s `TemplateProject::new` shows scene
construction, material registration, pawn setup, and possession together.

Run the repository template with:

```text
cargo run -p dogwood_template_project --bin template-project
```
