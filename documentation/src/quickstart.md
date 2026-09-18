# Five-Minute Quickstart

The template already has the minimum runnable shape. A game implements `Game`,
creates a shared `compute::Accelerator`, then launches.

Startup is intentionally staged: create the accelerator once, build the game
and its material registry/scene, then call `Game::launch`. `GameApplication`
owns the window and frame lifecycle. Put game rules in `Game::update`, actor
contact handling in `Game::actor_contacts`, and HUD composition in
`Game::compose_user_interface`; keep scene generation in the scene module.

The canonical launch path is the consuming `Game::launch` method:

```rust
use dogwood_engine::{
    compute::Accelerator,
    Game,
};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    MyGame::new()?.launch(Arc::new(Accelerator::new()?))
}
```

Implement `TITLE`, `camera`, `is_paused`, `scene`, `scene_mutable`, and
`user_interface_context`. The template's `TemplateProject::new` shows
accelerator use, material registration, scene construction, pawn setup, and
possession together. Copy that shape, then move game-specific rules into
separate modules as the project grows.

Run the repository template with:

```text
cargo run -p dogwood_template_project --bin template-project
```
