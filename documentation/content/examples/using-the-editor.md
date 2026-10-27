+++
title = "Using the editor"
description = "Launch a game in Dogwood's editor and use its viewport, tools, camera, and save controls."
weight = 80
[extra]
sources = [
    "editor/src",
    "template/src/bin/editor.rs",
    "template/src/template_game.rs",
    "cli/src/commands/edit.rs",
    "cli/src/main.rs",
    "cli/src/commands/build.rs",
    "template/Cargo.toml.template",
    "engine/src/games/game_instance.rs",
    "engine_world/src/scenes/scene.rs",
    "engine/src/lib.rs",
    "engine_graphics/src/world/materials/material_radiance.rs",
    "engine_world/src/materials/material_temperature_color.rs",
    "engine_graphics/src/renders/scene_renderer_lighting_pass.rs",
    "engine_graphics/src/renders/scene_renderer_lighting.wgsl",
    "engine_graphics/src/renders/scene_renderer_lighting_cascades.wgsl",
    "engine_graphics/src/renders/scene_renderer_lighting_mips.wgsl",
    "engine_graphics/src/renders/scene_renderer_lighting_optical.wgsl",
    "engine_graphics/src/renders/scene_renderer_lighting_spread.wgsl",
    "engine_graphics/src/renders/scene_renderer_lighting_sprites.wgsl",
    "template/assets/sprites/glowstone_red.png",
    "template/assets/sprites/glowstone_green.png",
    "template/assets/sprites/glowstone_blue.png",
    "template/assets/sprites/glowstone_cyan.png",
    "template/assets/sprites/glowstone_magenta.png",
    "template/assets/sprites/glowstone_yellow.png",
    "template/assets/sprites/glowstone_gravel_red.png",
    "template/assets/sprites/glowstone_gravel_green.png",
    "template/assets/sprites/glowstone_gravel_blue.png",
    "template/assets/sprites/glowstone_gravel_cyan.png",
    "template/assets/sprites/glowstone_gravel_magenta.png",
    "template/assets/sprites/glowstone_gravel_yellow.png",
]
+++

Dogwood's editor runs your game in a window with a viewport, a tools panel, camera controls, and saving.

## Launch the editor

- From the CLI: run `dogwood edit` ([CLI reference](@/external-api/cli.md)) from a Dogwood project. It builds and runs the project's `editor` binary in release mode with the `editor` feature enabled.
- From code: construct an `Engine` and a game, then call `Editor::launch(engine, game)`. It consumes both and opens the editor window, with the game shown in its Viewport. The template's editor binary shows this wiring.

See [Editor API](@/external-api/editor.md) for the signature and template wiring.

## View and edit a scene

The editor opens with docked **Viewport** and **Tools** tabs. Tools lists the scene's registered cellular materials and an **Eraser**, plus brush and view settings. Select a brush and set its size from 1 to 64 cells:

| Brush | Effect |
| --- | --- |
| Circle | Fills cell centers within half the brush size of the stroke. |
| Square | Stamps a square of the selected size at each sampled cell along the stroke. |

Material strokes use the selected color and **Insert** or **Replace** mode; erasing always uses Replace with the empty material. Brush strokes are clipped to the active region, so cells outside the resident tiles are not edited. See [editing and saving the world](@/examples/editing-cells.md) for edit validation and persistence details.

Click and drag with the primary mouse button in the Viewport to apply a stroke.

The toolbar selects Graphics or Pressure view. Pressure view colors cells by pressure in atmospheres and displays a legend. Overlay toggles show chunks in pink, tiles in dark pink, and collision outlines in green. The cursor is drawn as a cell-grid overlay. The status bar shows:

- The cell under the cursor.
- The latest edit rejection, cleared by the next stroke that succeeds or is clipped to no cells.
- Frames per second and updates per second.
- Save state.

The editor starts paused, with Circle size 8, Insert mode, white color, and Graphics view and overlays off. It selects the first material in its list, or Eraser if the list is empty.

## Pressure tools

The Pressure tool adds or removes pressure along a stroke at a rate in atmospheres per second. The Explosion tool sets radius, pressure magnitude in atmospheres, and delay from 0 to 10 seconds. Clicking places an actor with a red overlay; when its delay ends during Play, the editor adds pressure across its radius. See [pressure edit behavior](@/external-api/collisions-and-edits.md#building-an-edit).

## Play, pause, and move the camera

The toolbar's **Play** button starts game updates and simulation. **Pause** stops those updates while the editor continues streaming the scene.

**Free camera** switches from the game's camera to an editor camera starting at the same position and using the same camera dimensions.

- Move it with **W/A/S/D** or the arrow keys while the viewport is hovered or focused.
- Movement speed on each axis is the camera's visible width per second.
- Scroll over the viewport to zoom; scroll changes clamp the free camera zoom to 0.25–2.0.

## Input while playing

Game input is generally routed only in Play mode with the game camera selected; key releases are always forwarded.

- Keyboard presses reach `Game::keyboard_event` only when the viewport is focused and egui is not requesting keyboard input. Key releases are always forwarded.
- Mouse events reach `Game::mouse_event` only when the pointer hovers over the viewport. Cursor positions are translated to viewport pixels.
- egui pointer-button releases are also forwarded to the game UI outside the viewport, so no button stays held.

See [Input and game UI](@/examples/input-and-ui.md) for how a game handles these events.

## Save changes

Choose **Save** or use the platform command-S shortcut. The editor requests `Scene::save`, marks the save as pending, and shows **Saving…** until `Scene::is_saved()` reports completion. A failed chunk write can leave the status at **Saving…**. It then shows a brief "Scene saved" notification.

The status bar otherwise reports **Unsaved changes** after accepted editor strokes and **Saved** after the requested save completes. This indicator tracks successful editor stroke requests, not arbitrary changes made by gameplay while Play is active.

> **Limitation:** The editor exposes material and eraser brushes, tile/chunk grid toggles, and play/free-camera controls. The current toolbar has no undo or redo control, and editor workspace/tool choices are initialized at launch.

{{ <rustdoc path="editor/struct.Editor.html" text="Editor" /> }}

## Paint water

The template registers `water` and six colored glowstone materials in the material picker. Their optical properties contribute emitted light and occlusion; see the [template lighting overview](@/getting-started/template-walkthrough.md#materials). Select water and paint into empty space, then enable Play: particle spawns and erasures are applied by simulation, so they do not appear while the editor is paused. Water uses its material color; the brush color does not tint individual particles. The player's jump/up and crouch/down controls swim vertically once sufficiently immersed.

Large requests can report `CapacityReached`; see [fluid capacity](@/external-api/scenes.md#fluid-capacity). Enable Play to apply particle edits before requesting a save, and keep the lifecycle running while imports finish; see [fluid saving](@/internals/world-streaming/fluid-streaming.md#resident-saves). The template deletes its world on launch, as described in [the walkthrough](@/getting-started/template-walkthrough.md#world).
