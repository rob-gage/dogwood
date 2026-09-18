# Game-Project Layout

Keep game code separate from engine code. The template uses this small shape:

```text
my_game/
├── Cargo.toml
├── src/
│   ├── lib.rs          # exports the game type
│   ├── project.rs      # Game implementation and scene ownership
│   ├── actors/         # actor/gameplay helpers
│   ├── gameplay/       # rules and per-game systems
│   ├── scene/          # SceneGenerator and world setup
│   └── ui/             # game widgets/state
└── my-game.rs          # executable entry point
```

`Cargo.toml` should depend on `dogwood_engine` and, when useful, a separate
material crate. Import engine APIs through `dogwood_engine`, not directly from
`dogwood_engine_physics` or other implementation crates.

`project.rs` is the composition root: it owns the game state, accelerator
launch setup, scene, and UI context. `actors/` should contain
spawn/configuration helpers, `gameplay/` should contain rules such as pickups
and scoring,
`scene/` should contain the generator and world setup, and `ui/` should contain
widgets plus HUD state. The template's executable is deliberately thin; a
real game can retain that separation while replacing demo content.
