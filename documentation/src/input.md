# Input

The default `Game` implementation translates arrows and WASD into normalized
locomotion and sends it to the possessed actor. Override `input_translator` to
use `SimpleInputTranslator::ARROWS`, `WASD`, or a custom `InputTranslator`.

```rust
fn input_translator(&self) -> Box<dyn InputTranslator> {
    Box::new(SimpleInputTranslator::WASD)
}
```

`KeyboardInputState::is_pressed(Key::SPACE)` reads a key. For controls beyond
locomotion, override `pass_input` and inspect the state before calling or
replacing the default behavior. Keep input mapping in the game layer; actors
consume `ActorControlState`.

`MouseInputState` similarly retains held mouse buttons. Winit supplies mouse
input as a `WindowEvent::MouseInput` with separate `state` and `button` fields;
pass both fields to `process_event`, then query a button with

## Gameplay Pointer Input

`Game::pass_pointer_input` receives `GamePointerInput` before each ordinary
`Game::update`. Its `world_position` is already converted through the active
camera and scene viewport; it is `None` outside that viewport. Primary press
and release are one-frame edges, while `primary_down` remains held. UI-owned
presses do not become gameplay presses, and a host can disable gameplay
pointer delivery while using the editor for scene editing.

For a fixed-reach collection tool, normalize the direction from the pawn to
the pointer and use that direction at a fixed distance:

```rust
let player = scene.actor_registry().get_position(pawn).unwrap().world();
let center = ScenePosition::from_world([
    player[0] + aim[0] * 1.0,
    player[1] + aim[1] * 1.0,
]);
scene.extract_materials(MaterialExtraction {
    region: SceneRegion::Circle { center, radius: 0.7 },
    filter: MaterialFilter::Any,
})?;
```

Extraction completes asynchronously through `Game::material_extractions`; use
the request ID to associate aggregate material amounts with the tool. The
editor routes pointer input to gameplay while attached to the game pawn. After
Detach, pointer input is reserved for editor scene tools and no gameplay
collection request is generated.
