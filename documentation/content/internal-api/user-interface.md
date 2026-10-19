+++
title = "UI internals"
description = "How native egui state, overlay rendering, themes, and game UI layout are managed."
weight = 60
[extra]
sources = ["engine_user_interface/src/user_interface_context.rs", "engine_user_interface/src/user_interface_renderer.rs", "engine_user_interface/src/user_interface_window_state.rs", "engine_user_interface/src/user_interface_theme.rs", "engine_user_interface/src/game_ui"]
+++

## Native egui state

This page is for engine contributors working on `engine_user_interface`. It covers the native egui context, window state, overlay renderer, and the game UI storage and layout engine.

`UserInterfaceContext` wraps a cloneable `egui::Context`. Construction applies `UserInterfaceTheme::default()` (the dark theme). `run` passes the supplied `RawInput` to egui and invokes the composition closure for each layout pass, then returns an owned `egui::FullOutput` without retaining it.

`UserInterfaceTheme::dark()` and `light()` build the corresponding styles; `with_accent` sets link, selection, and widget accent colors. `apply` installs the style and matching egui theme while preserving interaction memory, and `UserInterfaceContext::set_theme` delegates to it.

- egui can request a discarded pass and call the closure again, so composition must be safe to repeat.
- Repaint requests use the host-installed callback when available.
- The root viewport's `repaint_delay` remains in `FullOutput` for callers that inspect it. Dogwood's native host schedules redraws from its render deadline or continuous polling.

### Window state

The host owns a separate `UserInterfaceWindowState` per native window. It binds egui-winit state to the root viewport, initial window scale factor, native theme, and optional texture-size limit.

`process_window_event` forwards events to egui-winit and returns its `EventResponse`, but the response is advisory. Dogwood's host independently routes its translated input callbacks. The wrapper forces lifecycle events (including focus, resize, close, and redraw) to unconsumed, and forces redraw's repaint flag to false.

On redraw, the host takes accumulated input and later applies `PlatformOutput` to the window. See [host internals](@/internal-api/host.md) for host callback routing and [UI API](@/external-api/user-interface.md) for the game-facing surface.

## Overlay renderer

`UserInterfaceRenderer` wraps egui-wgpu's renderer. `renderer()` and `renderer_mut()` expose the inner renderer for texture and callback-resource access.

Construction fixes the target format. The target must meet these requirements:

- the same format as construction (on a format change, create a new `UserInterfaceRenderer` and egui context; the old context's consumed texture deltas are not replayed);
- a single sample, with no depth or stencil;
- preferably a gamma-space `Rgba8Unorm` or `Bgra8Unorm` format.

`render` consumes `FullOutput`, so the caller must take `platform_output` from it first. It applies texture updates, tessellates shapes, and updates GPU buffers. When paint jobs are nonempty, it paints an overlay pass using `LoadOp::Load`, preserving earlier target contents. For a zero-width or zero-height target it skips tessellation and painting, frees pending texture deletions, and returns no callback command buffers.

`render` returns egui-wgpu callback command buffers rather than submitting them. The app submits those buffers before the command buffer finished from the encoder passed to `render`. These responsibilities let the game instance and editor compose their own scene and UI work; see [frame lifecycle](@/architecture/frame-lifecycle.md).

## Game UI storage and layout

Taffy is the tree-based layout engine used here to compute element sizes and positions.

### Storage

`GameUi` is a reusable runtime holding a `UiLayoutEngine` and optional ID namespace. Each `show` or `show_in` call consumes a `UiElement` tree and flattens it in preorder into element storage and `UiLayoutNode` records. `show` uses the current available egui rectangle; `show_in` uses its supplied rectangle. Both use egui logical-point coordinates.

Anonymous IDs derive from the configured namespace (by default, the egui UI ID and optional root ID) plus the child-index path. Explicit IDs are inserted as given, and repeated explicit IDs in one tree return `TaffyError::InvalidInputNode`.

### Layout engine

The layout engine maintains a Taffy tree and a `HashMap<egui::Id, taffy::NodeId>`. For nonempty input, it validates the root, unique IDs, child indexes, cycles or multiple parents, and disconnected nodes; empty input returns no layouts. It reuses cached Taffy nodes by ID, updates styles and children, removes cache entries absent from the current tree, and marks leaves dirty.

The public {{ <rustdoc path="engine_user_interface/game_ui/layout/struct.UiLayoutEngine.html" text="UiLayoutEngine" /> }} computes layouts from {{ <rustdoc path="engine_user_interface/game_ui/layout/struct.UiLayoutNode.html" text="UiLayoutNode" /> }} inputs and returns {{ <rustdoc path="engine_user_interface/game_ui/layout/struct.UiComputedLayout.html" text="UiComputedLayout" /> }} results.

### Measurement

Layout computation measures leaves as follows; containers generally measure as zero and receive size from Taffy.

- Text, buttons, and toggles use egui text layout.
- Sliders and text inputs use egui interaction size.
- Image buttons use their image's source size.
- Image elements measure to the source texture's pixel size multiplied by the absolute UV span on each axis.
- Separators measure 1 × 1 point; progress indicators measure 64 points wide by `interact_size.y` high.

Logical-unit rounding starts disabled and can be enabled on the runtime.

### Results and rendering

The result for each node stores an absolute border rectangle, allocated content rectangle, and content size that includes Taffy's scroll overflow. Rendering walks the flattened tree, paints elements, clips descendants when requested, and uses egui `ScrollArea` for scroll nodes.

Controls borrow mutable state from the caller: toggle changes its `bool` when clicked, while slider and text-input widgets operate on borrowed values during rendering. Responses are collected in `UiResponses` by element ID.

### Default layout style

`UiLayout::default()` uses column flex, with clipping disabled and no width or height intent; `UiLength::default()` is `Auto`.

### Fill and flex normalization

`UiLayout` keeps a Taffy style plus optional width and height intent, which is used to normalize `Fill` and `Flex(w)` against the parent's layout mode:

| Parent | Fill | Flex(w) |
| --- | --- | --- |
| Flex, main axis | Grows with weight 1 from a zero basis | Grows with weight `w` from a zero basis |
| Flex, cross axis | 100% size | 100% size |
| Grid | 100% size | 100% size |
| Absolute | 100% size on each axis unless both opposite insets are set | Same as Fill |
| Root | Resolves to the supplied bounds | Resolves to the supplied bounds |

Raw Taffy style values override the intent:

- A nondefault `flex_grow` or `flex_basis` suppresses main-axis and grid normalization.
- A non-auto raw dimension on an axis overrides the intent for that axis in every mode.

### Inherited behaviour

- Flattening a scroll node disables flex shrinking for its direct children.
- Effective enabled state is inherited: a child is enabled only when it and its parent are enabled.
- `UiElement::overlay` is normalized into a one-cell grid; grid children share that cell unless positioned absolutely.

See [game UI API](@/external-api/user-interface.md) for element construction.

{{ <rustdoc path="engine_user_interface/user_interface_context/struct.UserInterfaceContext.html" text="UserInterfaceContext" private={true} /> }} · {{ <rustdoc path="engine_user_interface/user_interface_window_state/struct.UserInterfaceWindowState.html" text="UserInterfaceWindowState" private={true} /> }} · {{ <rustdoc path="engine_user_interface/user_interface_renderer/struct.UserInterfaceRenderer.html" text="UserInterfaceRenderer" private={true} /> }}
