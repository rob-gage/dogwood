+++
title = "Input and game UI"
description = "Handle host events in a Game and compose a small in-game interface."
weight = 60
[extra]
sources = [
    "engine_host/src/system_events",
    "engine/src/games/game.rs",
    "engine/src/games/game_instance.rs",
    "engine_user_interface/src/game_ui",
    "engine_user_interface/src/lib.rs",
    "engine_user_interface/src/user_interface_context.rs",
    "engine/src/lib.rs",
]
+++

## Handle host input

This page covers receiving host input and composing a small in-game interface in a `Game`. `Game` has four callbacks for host input and UI; see [host types](@/external-api/host.md) for event variants and units.

- `keyboard_event`: receives a `SystemKeyboardEvent`.
- `mouse_event`: receives a `SystemMouseEvent`.
- `window_event`: receives a `SystemWindowEvent`.
- `user_interface`: composes the interface drawn over the scene (see below).

```rust
use engine::{
    games::Game,
    system_events::{
        KeyCode, PhysicalKey, SystemKeyboardEvent, SystemMouseEvent, SystemWindowEvent,
    },
};

struct CounterGame {
    count: u32,
    size: (u32, u32),
    is_button_down: bool,
}

impl Game for CounterGame {
    fn keyboard_event(&mut self, event: SystemKeyboardEvent) {
        if event.pressed
            && !event.repeat
            && event.physical_key == PhysicalKey::Code(KeyCode::Space)
        {
            self.count += 1;
        }
    }

    fn mouse_event(&mut self, event: SystemMouseEvent) {
        if let SystemMouseEvent::Button { pressed, .. } = event {
            self.is_button_down = pressed;
        }
    }

    fn window_event(&mut self, event: SystemWindowEvent) {
        if let SystemWindowEvent::Resized { width, height } = event {
            self.size = (width, height);
        }
    }
}
```

In the editor, keyboard presses and mouse events reach the game only in Play mode with the game camera selected. Key releases are forwarded in every mode; see [Using the editor](@/examples/using-the-editor.md#input-while-playing).

## Compose game UI

`Game::user_interface` receives egui's `Ui` (exported as `engine::user_interface::UserInterface`). Dogwood also provides `GameUi`, a declarative element tree:

- Build elements with `UiElement`.
- Call `GameUi::show` in each `user_interface` callback.
- Read the returned responses by element id.

Keep one `GameUi` in your game so it can reuse its layout state. `show` returns an error for an invalid layout; the example below treats that as a programming error.

```rust
use engine::{games::Game, user_interface::{UserInterface, game_ui::{GameUi, UiElement}}};

struct MenuGame {
    ui: GameUi,
    count: u32,
    increment_pending: bool,
}

impl Game for MenuGame {
    fn update(&mut self, _elapsed_time: std::time::Duration) {
        if self.increment_pending {
            self.count += 1;
            self.increment_pending = false;
        }
    }

    fn user_interface(&mut self, ui: &mut UserInterface) {
        let responses = self.ui
            .show(ui, UiElement::column()
                .child(UiElement::text(format!("Count: {}", self.count)))
                .child(UiElement::button("increment", "Add one")))
            .expect("valid game UI layout");
        if responses.get("increment").is_some_and(|response| response.clicked()) {
            self.increment_pending = true;
        }
    }
}
```

The game UI API includes layout, appearance, text, image, and control elements. For egui widgets or custom painting, use the `Ui` passed to the callback directly. See [user interface API](@/external-api/user-interface.md) for the wider API surface.

{{ <rustdoc path="engine/games/trait.Game.html" text="Game" /> }}
