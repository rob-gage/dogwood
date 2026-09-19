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

The supported CLI convention is a Cargo workspace with a binary package named
`game`. A workspace may declare the package explicitly:

```toml
[workspace.metadata.dogwood]
version = 1
game = "game"
```

From the workspace root, use `dogwood debug` for local Cargo development,
`dogwood build --linux --x64` (or another supported target) for a portable
release package, `dogwood run` to build and run that package from a reusable
temporary cache, and `dogwood check` to validate the project and Docker build
tools. Build outputs are written to `dist/<target>/`; Windows packages include
`vulkan-1.dll` beside the executable. Linux packages use a packaged loader when
one is supplied and otherwise fall back to the system Vulkan loader. No Vulkan
SDK is required; the installed GPU driver must provide the Vulkan ICD.

`build` and `run` require exactly one platform flag (`--windows` or `--linux`)
and exactly one architecture flag (`--x86`, `--x64`, or `--arm64`) when any
target flags are supplied. For example: `dogwood build --windows --x64`.
Omitting all target flags selects the current platform and architecture while
still using the Dockerfile.
