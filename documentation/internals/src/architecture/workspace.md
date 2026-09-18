# Workspace and crate boundaries

The workspace is intentionally split by responsibility. `dogwood_engine` is
the supported facade; lower-level crates are linked into its public modules.

| Crate | Responsibility |
|---|---|
| `engine` | game host, window/event loop, render orchestration |
| `engine_compute` | WGPU device/queue, buffers, shader composition, timing |
| `engine_graphics` | camera, colors, material appearance, scene render views |
| `engine_input` | keyboard state and control translation |
| `engine_physics` | scenes, actors, materials, simulation, persistence |
| `engine_user_interface` | egui context and reusable widgets |
| `engine_audio` | current audio/material-acoustics extension surface |
| `editor` | editor host, tools, brushes, viewport and editor UI |
| `cli` | project initialization command |
| `template_materials` | canonical declarative material/reaction set |
| `template_project` | executable game and editor example |

The facade aliases implementation crates privately, then re-exports their
public items under `compute`, `graphics`, `input`, `physics`, and
`user_interface`. This keeps game imports stable while allowing internals to
move between modules. The editor deliberately consumes the facade plus a few
implementation crates for its host-specific integration.

### Relevant implementation

- `Cargo.toml` — workspace members and shared dependency versions.
- `engine/Cargo.toml` — facade dependencies and platform features.
- `engine/src/lib.rs` — aggregation and public module layout.
- `engine_physics/src/lib.rs` — physics public/private module boundary.
- `editor/src/lib.rs` — editor crate boundary and `EditorGame` export.
- `cli/src/main.rs` — project creation entry point.
