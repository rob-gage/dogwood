# Public API

The game-facing crate is `dogwood_engine`. Its supported top-level API is:

| Module | Use |
|---|---|
| `Game`, `GameApplication` | application lifecycle |
| `compute` | shared `Accelerator` |
| `graphics` | `Camera`, `Color`, material appearance |
| `input` | keyboard state and control translation |
| `physics` | actors, scenes, materials, tiles, simulation |
| `user_interface` | `UserInterfaceContext`, widgets |

For example:

```rust
use dogwood_engine::{Game, graphics::Camera, physics::scenes::Scene};
```

Rustdoc is the detailed symbol reference. Manual pages describe workflows. Cross-links worth opening include [`Game`](https://docs.rs/dogwood_engine/latest/dogwood_engine/trait.Game.html), [`Scene`](https://docs.rs/dogwood_engine/latest/dogwood_engine/physics/scenes/struct.Scene.html), [`ActorRegistry`](https://docs.rs/dogwood_engine/latest/dogwood_engine/physics/actors/struct.ActorRegistry.html), and [`Material`](https://docs.rs/dogwood_engine/latest/dogwood_engine/physics/materials/enum.Material.html).

Lower-level crates may change as the engine evolves. If a type is not reachable from this facade, do not treat it as a game API.
