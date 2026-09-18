# CLI And Templates

The CLI owns project initialization and build orchestration.
`dogwood new` validates a Cargo-compatible project name, creates the requested
directory and `src`, and writes a manifest plus a `main.rs` that launches the
template project with a shared `Accelerator`.

The build commands discover a Cargo workspace through Cargo metadata. A game
project must contain a binary package named `game`, or declare its package in
`[workspace.metadata.dogwood]` with `version = 1` and `game = "..."`.
`dogwood debug` runs that package locally. `dogwood build` uses Docker BuildKit
with cargo-xwin for Windows targets and cargo-zigbuild for Linux targets, then
exports `dist/<target>/game` (or `game.exe`). `dogwood check` reports project,
host, Docker, and cross-build readiness without building the game.

The template is executable documentation. `template_project` implements
`Game`, creates temporary scene data, configures a generator, spawns and
possesses a pawn, spawns physical square actors, collects actor contact events,
and draws a counter. Its modules are split into project, actors, gameplay,
scene, and UI areas, with each showing the intended place for game behavior.

`template_materials` is the canonical material authoring example. It registers
all four material forms, assigns appearance and thermal metadata, creates tags
and reactions, and compiles one registry shared by the template scene and
tests. The template generator fills missing chunks with a deterministic stone
ground using world cell coordinates and appearance seeds.

### Relevant Implementation

- `cli/src/main.rs` — project creation, manifest and source templates.
- `cli/src/command.rs` and `cli/src/subcommand.rs` — CLI argument model.
- `template_project/src/project.rs` — canonical `Game` and scene setup.
- `template_project/src/scene/generator.rs` — generated chunk terrain.
- `template_project/template-project.rs` — runnable game entry point.
- `template_project/template-project-editor.rs` — editor entry point.
- `template_materials/src/materials.rs` — material registry authoring.
- `template_materials/src/reactions.rs` — tag/exact reaction declarations.
