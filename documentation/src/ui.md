# UI and rendering

`Game` owns a `UserInterfaceContext`. The built-in widgets include `Button`,
`Spacer`, `StackHorizontal`, and `StackVertical`. Add a widget through
`GameApplication::add_widget` when you have application access, or use the
context’s `run` method for direct egui composition.

```rust
self.ui.run(input, |ui| {
    ui.egui().label(format!("Score: {}", self.score));
});
```

For per-frame game UI, implement `Game::compose_user_interface` and call
`UserInterfaceContext::add_contents` with Dogwood's `UserInterface`; template
game code does not need a direct egui dependency. For reusable widgets, implement `user_interface::Widget::display`. The
template’s UI module is the place to keep game-specific state and widgets.
`graphics::Camera` controls the scene view; material appearance controls cell
rendering. Game code does not need to touch WGPU renderers.
