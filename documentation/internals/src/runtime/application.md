# Runtime And Application Lifecycle

`Game` is the application-level contract. It supplies a title, camera,
optional scene, pause state, input translation, input dispatch, UI context, and
the consuming `launch` method. `GameApplication<G>` owns the game and the
window-facing state needed to run it.

The Winit lifecycle is:

1. `Game::launch` builds the event loop and `GameApplication`.
2. `resumed` creates the window, surface, UI window state, and render setup.
3. Window events update keyboard/UI input and may request redraw or exit.
4. `about_to_wait` starts timing, translates input, calls `update`, and requests
   a redraw.
5. `update` calls `Scene::update`, then `Game::actor_contacts`, `Game::update`,
   UI composition, performance accounting, and camera following.
6. The redraw handler encodes scene rendering, UI rendering, timing resolve,
   submits the command buffer, and presents the surface frame.

`Scene::update` receives elapsed wall time but internally advances fixed 60 Hz
ticks, capped at four catch-up ticks. The application can disable simulation;
`Game::is_paused` also makes simulation inactive. A pawn may opt into movement
while paused, but GPU world simulation is skipped when inactive. Rendering and
UI composition continue.

The camera follows `Game::camera_target`, normally the possessed actor. Camera
position is interpolated at render time; simulation state remains fixed-step.

### Relevant Implementation

- `engine/src/games/game.rs` — game callbacks, input default, and launch.
- `engine/src/games/game_application.rs` — lifecycle, update, camera, and
  presentation.
- `engine/src/games/game_application_window.rs` — Winit surface and event
  handling.
- `engine/src/renders/scene_renderer.rs` — scene render command encoding.
- `engine/src/renders/user_interface_renderer.rs` — egui render handoff.
