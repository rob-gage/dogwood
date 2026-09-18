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

Rustdoc is the detailed symbol reference. Manual pages describe workflows.
Start with the [dogwood_engine Rustdoc](https://docs.rs/dogwood_engine) and
navigate to `Game`, `Scene`, `ActorRegistry`, or `Material`.

Lower-level crates may change as the engine evolves. If a type is not reachable
from this facade, do not treat it as a game API.
