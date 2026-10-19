+++
title = "Game UI layout and appearance"
description = "How game UI elements map to Taffy layout, egui painting, and interaction responses."
weight = 20
[extra]
sources = ["engine_user_interface/src/game_ui", "engine_user_interface/src/user_interface_renderer.rs", "engine/src/games/game_instance.rs"]
+++

## Composition and layout

This page is for engine contributors. It explains how `GameUi` turns a `UiElement` tree into Taffy layout, egui painting, and interaction responses. For the facade path and builder reference, see the [public UI API](@/external-api/user-interface.md).

### Composition

`GameUi` takes a `UiElement` tree by value during an egui frame; its text and control values may be borrowed. `show` lays the root out in the current available rectangle; `show_in` takes explicit egui bounds. Both return `Result<UiResponses, taffy::TaffyError>`.

Each call runs these steps:

1. Flatten the tree.
2. Measure text and controls with egui.
3. Compute a Taffy layout.
4. Paint the elements and gather their responses.

Reuse a `GameUi` to retain its Taffy node cache between calls. `GameUi::with_id` sets the namespace used for anonymous element IDs.

### Elements and IDs

`UiElement` constructors cover panels, rows, columns, overlays, grids, scroll containers, spacers, text, images, rectangles, separators, progress indicators, buttons, image buttons, toggles, sliders, and single-line text inputs. Containers own child elements. Controls that edit application state borrow their value for the composition pass.

Elements have layout, appearance, typography, and enabled and selected state. ID rules:

- Interactive controls (`button`, `image_button`, `toggle`, `slider`, `text_input`) take a required ID. Other elements may set one with `id`.
- Anonymous IDs derive from the element's tree path and root namespace.
- Explicit IDs are global and are not scoped by `GameUi`. Namespace them at the call site when reusing IDs across surfaces.
- Duplicate explicit IDs in one tree return a Taffy layout error.

### Layout styles

The default `UiLayout` is a column flex container. Its public `taffy::Style` can be set directly or adjusted with helpers for dimensions, padding, margins, gaps, aspect ratio, clipping, and flex direction.

| Rule | Behavior |
| --- | --- |
| Length | `Fixed` uses logical egui points. `Percent` is expressed from 0 to 100, converted to Taffy's 0-1 fraction, and resolves against each Taffy parent's content box. `Auto` delegates sizing; `Fill` is flex weight 1; `Flex(weight)` supplies a weight. |
| Flex main axis | `Fill`/`Flex` sets grow intent and a zero flex basis. |
| Flex cross axis | `Fill`/`Flex` fills the available dimension with 100% sizing. |
| Grid | For `Fill`/`Flex` dimensions, grid children use percentage fill unless the child has a nondefault `flex_grow` or `flex_basis`. |
| Absolute | Fill/flex dimensions use percentage fill unless opposing insets anchor that dimension. Absolute children have separate normalization. |

For the root element, `Fill`/`Flex` dimensions resolve to the corresponding bounds dimension.

Direct Taffy style fields are passed through. On a flex child's main axis, non-default raw `flex_grow` or `flex_basis` suppresses flex normalization; cross-axis fill only yields to an explicit raw dimension. Grid children with either non-default raw flex field skip fill normalization. Absolute-fill normalization checks opposing insets and explicit dimensions. Length-based helpers sanitize non-finite and negative values. `show_in` bounds are normalized to finite coordinates and nonnegative dimensions.

### Element-specific layout

- An overlay is a single-cell Taffy grid. Its children are placed at line 1 of both axes unless they are absolutely positioned.
- A spacer with auto width becomes a flex width when its parent direction is row or row-reverse; otherwise, an auto height becomes a flex height.
- Children of a scroll element get `flex_shrink = 0`.

### Computed layout

- `UiComputedLayout.rect` is the absolute border-box rectangle; `content_rect` is the allocated content box.
- `content_size` includes Taffy's reported overflow and is used by scroll elements.
- Coordinates and sizes are logical points, not physical pixels or world tiles.
- Optional logical-unit rounding is off by default.
- The layout engine validates the node graph and can return Taffy errors for malformed input or failed tree operations.

## Appearance, images, and text

### Visual selection

`UiAppearance` holds optional `UiVisual` values for the `normal`, `disabled`, `pressed`, `hovered`, `selected`, and `focused` states. `normal` is the final fallback. `UiAppearance::visual` picks the first matching case:

1. Disabled element: `disabled`, else `normal`.
2. Pressed: `pressed`, `hovered`, then `selected` (only if the element is selected), `focused`, `normal`.
3. Selected and not pressed: `selected`, then `hovered` if hovered, then `focused` if focused, then `normal`.
4. Hovered: `hovered`, `focused`, `normal`.
5. Focused: `focused`, else `normal`.
6. Otherwise: `normal`.

### Visuals and brushes

`UiVisual` combines a `UiBrush` background, border stroke, corner radius, tint, opacity, and point offset. A brush paints nothing, a solid color, a fitted image, or a nine-slice image. Visual opacity is clamped for custom painting; text and native control painting are handled separately.

### Textures and images

`UiTexture` stores an egui texture ID and the full texture's source pixel dimensions. The application retains the actual texture handle and chooses its sampling options when uploading it.

`UiImage` adds normalized UV coordinates, a source-texel inset (default 0.5), a fit policy, and a tint. The fit policy decides how the image maps to its destination:

| Fit | Behavior |
| --- | --- |
| `Native` | Centers at the selected source region's size. |
| `Stretch` | Fills the destination. |
| `Contain` | Preserves aspect ratio without cropping. |
| `Cover` | Preserves aspect ratio and crops the UVs. |

### Nine-slice images

`UiInsets` and nine-slice border distances are source texture pixels; `UiNineSlice.scale` converts them to destination points.

- Source borders are clamped and proportionally reduced when opposing borders exceed the source.
- Destination borders are proportionally scaled to fit.
- Center and edges stretch by default, with optional tiling. Corners always stretch.
- Tiling is capped at 64 pieces per axis. If the requested count exceeds 64, pieces are enlarged to fill the destination; otherwise, the final piece may be cropped.

### Fonts and text style

`UiFonts` owns egui font definitions. Registering a font adds a named family with proportional-family fallbacks. Call `install` on the egui context after changing definitions.

`UiTextStyle` defaults to a 14-point proportional font, white color, left alignment, wrapping, and no explicit line height. It builds egui text layout for the available width.

## Painting and responses

### Painting

`GameUi` paints with egui painters for custom visuals, text, images, and toggles. Toggles use egui interaction with a custom-painted checkbox and label. Sliders and text inputs use native egui widgets.

- When enabled, an element clips its descendants to its content box; scroll containers clip their contents to the viewport.
- Children of buttons and image buttons are painted decoration-only: they do not receive click or control interaction, and their responses use hover-only senses.
- `GameUi` does not own GPU texture resources.

The surrounding [user-interface renderer](@/internals/rendering/user-interface.md) handles egui tessellation and records overlay drawing. The caller submits its returned callback command buffers before the finished command encoder buffer.

> **Limitation:** The engine and editor do not currently compose `GameUi` themselves.

### Responses

Each rendered element contributes a `Response` keyed by its component ID. `UiResponses` offers three lookups:

- `get` hashes the same ID source used with `egui::Id::new`.
- `get_by_id` accepts a complete egui ID.
- `iter` visits the collected pairs.

Buttons request focus when clicked. Toggles change their borrowed `bool` on click, and slider and text-input controls edit their borrowed values through egui.

## API reference

Rustdoc for the main types on this page:

- {{ <rustdoc path="engine_user_interface/game_ui/components/struct.GameUi.html" text="GameUi" /> }}
- {{ <rustdoc path="engine_user_interface/game_ui/layout/struct.UiLayout.html" text="UiLayout" /> }}
- {{ <rustdoc path="engine_user_interface/game_ui/appearance/struct.UiImage.html" text="UiImage" /> }}
- {{ <rustdoc path="engine_user_interface/game_ui/struct.UiTextStyle.html" text="UiTextStyle" /> }}
