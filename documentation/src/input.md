# Input

The default `Game` implementation translates arrows and WASD into normalized locomotion and sends it to the possessed actor. Override `input_translator` to use `SimpleInputTranslator::ARROWS`, `WASD`, or a custom `InputTranslator`.

```rust
fn input_translator(&self) -> Box<dyn InputTranslator> {
    Box::new(SimpleInputTranslator::WASD)
}
```

`KeyboardInputState::is_pressed(Key::SPACE)` reads a key. For controls beyond locomotion, override `pass_input` and inspect the state before calling or replacing the default behavior. Keep input mapping in the game layer; actors consume `ActorControlState`.
