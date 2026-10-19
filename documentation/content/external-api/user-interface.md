+++
title = "User interface"
description = "Compose native egui interfaces and game UI with the engine facade."
weight = 100
[extra]
sources = ["engine_user_interface/src", "engine/src/lib.rs"]
+++

## Using `Game::user_interface`

This page is for game programmers composing egui interfaces and game UI with the engine facade, and for integrators embedding the UI renderer. The `engine::user_interface` module re-exports the UI crate as well as `egui`, `egui_dock`, `egui_extras`, `egui_wgpu`, `wgpu`, and `winit`.

Its `UserInterface` type is an alias for `egui::Ui`. The engine calls [`Game::user_interface`](@/external-api/engine-and-games.md) during each rendered frame; use that surface for ordinary egui widgets and painting. egui may run a composition closure more than once per frame, so keep one-time side effects out of it or make them idempotent.

## Game UI layout

`game_ui` provides `GameUi` and `UiElement` for an immediate-mode element tree with a reusable Taffy layout-node cache. Store a `GameUi` in the game so its cache can be reused. Elements come in three kinds:

| Kind | Elements |
|---|---|
| Containers | `panel`, `row`, `column`, `overlay`, `grid(columns)`, `scroll`, `spacer(flex)` (a flexible empty region) |
| Leaves | `text`, `image`, `rectangle`, `separator`, `progress` |
| Interactive | `button`, `image button`, `toggle`, `slider`, `text input` (single-line) |

Toggles, sliders, and text inputs borrow their values mutably for composition. A game stores its `GameUi` and the values its widgets edit:

```rust
use engine::{
    games::Game,
    user_interface::{
        game_ui::{GameUi, UiElement},
        UserInterface,
    },
};

struct InventoryGame {
    game_ui: GameUi,
    enabled: bool,
}

impl Game for InventoryGame {
    fn user_interface(&mut self, ui: &mut UserInterface) {
        let responses = self.game_ui.show(
            ui,
            UiElement::column()
                .gap(8.0)
                .padding(12.0)
                .child(UiElement::text("Inventory"))
                .child(UiElement::toggle("inventory-open", "Open", &mut self.enabled)),
        ).expect("game UI layout should be valid");
        if responses.get("inventory-open").is_some_and(|response| response.changed()) {
            // `self.enabled` has already been updated by the toggle.
        }
    }
}
```

`show` returns `Result<UiResponses, taffy::TaffyError>`; the `expect` above fails only on an invalid layout, such as a duplicate explicit ID. Look up a widget's response by the ID passed when it was built.

### Showing and configuring

- `show` lays out the root in egui's available rectangle; `show_in` accepts an explicit egui rectangle.
- `GameUi::with_id` creates a `GameUi` with a stable namespace for anonymous elements.
- `GameUi::set_layout_rounding` enables or disables logical-unit rounding in the layout engine. Rounding is off by default.

### ID rules

- Response lookup uses the same ID supplied when constructing the element.
- Explicit IDs are global egui IDs; namespace an ID if you reuse it on multiple surfaces.
- A repeated explicit ID in one tree returns a layout error.

### Layout and appearance types

`UiLayout` exposes Taffy style as well as clipping and helper methods. `UiLength` supports automatic, fixed logical-point, percentage (0-100), equal-weight fill space (`Fill`), and weighted flex lengths (`Flex`). The other `game_ui` exports are:

| Type | Role |
|---|---|
| `UiAppearance` | Visual states of an element (normal, hovered, pressed, and so on). |
| `UiVisual` | The paintable visual of one state. |
| `UiBrush` | A solid, image, or nine-slice fill. |
| `UiImage` | A texture region with fitting and tint settings. |
| `UiInsets` | Four-sided source-border distances in texture pixels for nine-slice images. |
| `UiTextStyle` | Typography of a text element. |
| `UiFonts` | Font definitions and named font families. |
| `UiResponses` | Responses collected from one composition pass, looked up by element ID. |
| `UiLayoutNode`, `UiLayoutEngine`, `UiComputedLayout` | Low-level layout inputs, Taffy-backed computation, and computed geometry, exposed under `game_ui::layout`. |
| `taffy` | Re-export of the Taffy layout crate used by `UiLayout`. |

See the {{ <rustdoc path="engine_user_interface/game_ui/index.html" text="game_ui" /> }} rustdoc for their fields and methods, including {{ <rustdoc path="engine_user_interface/game_ui/layout/struct.UiLayoutEngine.html" text="UiLayoutEngine" /> }}, {{ <rustdoc path="engine_user_interface/game_ui/layout/struct.UiLayoutNode.html" text="UiLayoutNode" /> }}, and {{ <rustdoc path="engine_user_interface/game_ui/layout/struct.UiComputedLayout.html" text="UiComputedLayout" /> }}.

For images, `UiTexture` stores a texture ID and pixel size; retain the originating `TextureHandle` and choose `TextureOptions` when registering it. `UiImageFit` supports `Native`, `Stretch`, `Contain`, and `Cover`. `UiNineSlice` can stretch or tile its center and edges independently (`UiNineSliceMode`).

## Components and themes

The `engine::user_interface::components` module provides reusable egui helpers:

- `Hud` anchors foreground content.
- `InspectorSection` shows a collapsible titled section.
- `Notifications` displays expiring messages.
- `SettingsRow` pairs a label with controls.
- `StatusBar` and `Toolbar` provide compact horizontal strips.
- `StyledPanel` frames a content region.

For example, inside `Game::user_interface`, `Hud::new("status").show(ui.ctx(), |hud_ui| { hud_ui.label("Ready"); });` creates and displays a foreground HUD. See the {{ <rustdoc path="engine_user_interface/components/index.html" text="components rustdoc" /> }} and {{ <rustdoc path="engine_user_interface/components/struct.Hud.html" text="Hud" /> }} API docs.

`UserInterfaceTheme::default()` selects the configured dark theme. `dark()` and `light()` create the built-in variants; `with_accent` changes link and selection colors. Edit `theme.style` for broader egui customization before applying it; `apply` installs the style into an egui context. See {{ <rustdoc path="engine_user_interface/struct.UserInterfaceTheme.html" text="UserInterfaceTheme" /> }}.

### Workspace helpers

The `engine::user_interface::advanced` module provides editor-oriented workspace helpers:

- `DockWorkspaceState` owns egui_dock state and can select an active tab.
- `show_themed_dock_area` displays a dock with the active egui theme.
- `WorkspaceNavigationItem` describes stable-ID navigation entries with nested children.
- `show_workspace_navigation` displays that tree and reports selection through a callback.

The selection callback runs during composition and may run on multiple layout passes. Record an idempotent pending action and perform external effects after composition.

> **Limitation:** `GameUi` and the workspace navigation tree helpers have no call sites outside the UI crate. The editor uses `DockWorkspaceState` and reusable components while composing its layout with egui.

## Embedding the renderer (advanced)

Most games never use these types; the engine drives them. They matter to integrators who host egui themselves.

- `UserInterfaceContext` owns the egui context, runs a composition closure, and applies `UserInterfaceTheme`. `set_theme` replaces the active theme while preserving egui interaction memory.
- `UserInterfaceWindowState` adapts native window events and platform output through egui-winit. `process_window_event` returns advisory consumption information and does not change gameplay event routing.
- `UserInterfaceRenderer` paints `FullOutput` over an existing single-sample wgpu target with no depth or stencil attachment.

### Render order

- Before calling `render`, pass the output's `platform_output` to `UserInterfaceWindowState::handle_platform_output`; `render` consumes the remaining `FullOutput`.
- The renderer returns callback command buffers for the caller to submit before the encoder's command buffer.
- The target format must match the renderer's creation format. If the format changes, create a new `UserInterfaceRenderer` and a new egui context together, because the old context will not replay consumed texture deltas into the new renderer.

See the rustdoc for {{ <rustdoc path="engine_user_interface/struct.UserInterfaceContext.html" text="UserInterfaceContext" /> }}, {{ <rustdoc path="engine_user_interface/struct.UserInterfaceWindowState.html" text="UserInterfaceWindowState" /> }}, and {{ <rustdoc path="engine_user_interface/struct.UserInterfaceRenderer.html" text="UserInterfaceRenderer" /> }}.

For the lower-level engine callback contract, see [Engine and Game](@/external-api/engine-and-games.md); for game-facing examples, see [Input and game UI](@/examples/input-and-ui.md).
