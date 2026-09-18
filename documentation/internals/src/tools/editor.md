# Editor

The editor is a `GameApplication<G>` host with a second layer of editor state.
`EditorApplication` disables ordinary hosted simulation by default, owns a
free-fly pawn and possession handoff, maps pointer coordinates through the
runtime camera, and drives brushes/tools against the same `SceneEditBatch`
boundary used by gameplay.

The editor’s material, eraser, thermal, impulse, and rigid-placement tools
produce scene edits. Brush strokes are converted from cell anchors to batches;
thermal and impulse tools rate-limit continuous edits. Static material placement
can instead queue rigid-body placement. The scene remains responsible for
applying the edits and maintaining GPU/CPU ordering.

Editor UI is an ordinary `Widget` backed by egui. It reports selected material,
brush, view, play, free-fly, and visualization requests through small shared
request cells. The application consumes those requests during its update path.
Scene view modes and tile/chunk borders are renderer settings and do not mutate
simulation state.

Free-fly temporarily creates/possesses an editor pawn and asks the scene to
stream around it. Returning restores the original pawn and waits for streaming
to catch up before normal gameplay possession resumes.

### Relevant implementation

- `editor/src/editor_application.rs` — editor host state, update policy, and
  scene-edit submission.
- `editor/src/editor_application_pointer.rs` — pointer-to-world/cell mapping.
- `editor/src/editor_application_display.rs` — editor overlays and display
  integration.
- `editor/src/editor_brush.rs` — brush footprints and stroke interpolation.
- `editor/src/editor_tool.rs` — material/erase/thermal/impulse tool selection.
- `editor/src/editor_interface.rs` — docked egui controls and request state.
- `editor/src/editor_game.rs` — `Game::launch_in_editor` adapter.
