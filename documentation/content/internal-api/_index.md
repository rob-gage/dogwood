+++
title = "Internal API"
description = "Crate boundaries and the internal types that connect Dogwood's subsystems."
weight = 4
sort_by = "weight"
template = "section.html"
+++

Contributor-level notes on how each crate is wired internally; public contracts live in the [external API](@/external-api/_index.md) section.

| Page | Read it when you need... |
|---|---|
| [Crates and modules](@/internal-api/crates.md) | The workspace map, crate dependencies, and re-exports. |
| [Runtime internals](@/internal-api/runtime.md) | How `Engine` drives `GameInstance`. |
| [Host internals](@/internal-api/host.md) | Windows, the GPU context, and worker pools. |
| [World internals](@/internal-api/world.md) | `Scene` ownership and the stream and simulate flow. |
| [Graphics internals](@/internal-api/graphics.md) | The scene renderer passes. |
| [UI internals](@/internal-api/user-interface.md) | egui state, the overlay renderer, and game UI layout. |
| [Editor internals](@/internal-api/editor.md) | The editor viewport, tools, and input routing. |
| [CLI internals](@/internal-api/cli.md) | Templates, the cache, and Cargo invocation. |

## Private rustdoc

Private rustdoc is generated separately with `--document-private-items`. Private Rust API link paths start with the implementation crate name, such as {{ <rustdoc path="engine_world/scenes/struct.Scene.html" text="Scene" private={true} /> }}. Use the private build when a symbol is absent from the public rustdoc. Public contracts are in [Engine and Game](@/external-api/engine-and-games.md) and [Scenes](@/external-api/scenes.md).
