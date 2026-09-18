# Dogwood Game Programming

Dogwood games normally import one crate:

```rust
use dogwood_engine::{
    compute,
    graphics,
    physics,
    user_interface,
    Game,
    GameApplication,
};
```

`dogwood_engine` is the supported game-facing facade. Its module layout is the
contract; the lower-level `dogwood_engine_*` crates are implementation details
unless a page explicitly names them.

## If You're Implementing Gameplay, Start Here

- [Actors](actors.md) — spawn, possess, move, and remove actors.
- [Scene/world access](scenes.md) — construct a scene and run fixed updates.
- [Physics and collisions](physics.md) — choose pawn movement and collision
  shapes.
- [Materials](materials.md) — register the matter your game uses.
- [Input](input.md) — use the default controls or translate your own.
- [UI](ui.md) — add counters and other egui-backed widgets.
- [Saving and streaming](persistence.md) — load scene data and understand
  residency.
- [Recipes](recipes.md) — copy the shortest working patterns.

The executable [template
project](https://github.com/rob-gage/dogwood/tree/main/template_project) is the
canonical example in this repository. Start with
`template_project/src/project.rs`.
