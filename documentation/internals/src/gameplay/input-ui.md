# Input And User Interface

Input begins as Winit keyboard events. `KeyboardInputState` retains pressed
keys; `InputTranslator` turns that state into a normalized `ControlState`.
`GameApplication` stores the selected translator and calls `Game::pass_input`
before each update. The default translator maps arrows and WASD to locomotion,
and the default game implementation writes that control state to the possessed
actor.

UI is separate from simulation. `UserInterfaceContext` owns egui context,
window integration state, and the latest `FullOutput`. Game code composes UI
after scene/game update through `add_contents`; reusable Dogwood widgets
implement `Widget::display` against the wrapper `UserInterface`. Built-in
stacks, buttons, and spacers are ordinary widget composition.

At render time the context output is taken, tessellated, texture deltas are
uploaded, and egui is drawn over the scene render pass. UI input is offered to
egui from window events and may be consumed before gameplay-specific handling.
UI state belongs to the game/editor owner, not the scene.

### Relevant Implementation

- `engine_input/src/keyboard/` — Winit key state and key constants.
- `engine_input/src/controls/` — control state and translators.
- `engine/src/games/game.rs` — default translator and control dispatch.
- `engine_user_interface/src/user_interface_context.rs` — egui lifecycle,
  event handling, and output ownership.
- `engine_user_interface/src/widget.rs` and `widgets/` — widget contract and
  built-in composition.
- `engine/src/renders/user_interface_renderer.rs` — GPU UI handoff.
