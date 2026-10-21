+++
title = "Installation"
description = "Prepare the Rust and graphics prerequisites, then build Dogwood and install its command-line tool."
weight = 10
[extra]
sources = ["Cargo.toml", "cli/Cargo.toml", "cli/src/main.rs", "cli/src/utility", "cli/src/template", "engine_host/Cargo.toml", "engine_user_interface/Cargo.toml"]
+++

## Prerequisites

Install a Rust toolchain with Cargo that supports Rust edition 2024, plus the system graphics and windowing libraries your platform needs. The `dogwood` CLI finds Cargo on `PATH`, falling back to Cargo's home directory (`~/.cargo/bin`).

Dogwood uses wgpu and winit; the workspace enables these backends and window systems:

| Platform | GPU backend | Window system |
|---|---|---|
| Linux | Vulkan | X11 and Wayland |
| macOS | Metal | Native macOS windowing |
| Windows | DirectX 12 and Vulkan | Native Windows windowing |

## Install the CLI

1. Clone the Dogwood repository and change into it.
2. Optional: confirm the workspace compiles.

   ```sh
   cargo build --workspace
   ```

3. Install the CLI. The workspace crate is named `cli` and its binary is `dogwood`; `cargo install` installs it to Cargo's bin directory. Ensure that directory is on your `PATH`:

   ```sh
   cargo install --path cli
   ```

If the checkout's `template/` directory is absent, the CLI reads its embedded template snapshot. By default, project generation still requires the checkout's `engine` and `editor` directories; set `DOGWOOD_ENGINE_VERSION` to use registry dependencies instead. See [Engine dependency](@/getting-started/first-project.md#engine-dependency).

## Check the command

Print the help listing to confirm the install:

```sh
dogwood --help
```

The commands are introduced in [your first project](@/getting-started/first-project.md); the [CLI reference](@/external-api/cli.md) has the complete flag list.

## macOS testing

On Apple Silicon, use a native ARM64 Rust toolchain and install Apple's Command Line Tools (`xcode-select --install`). Build from a local checkout with a recent stable Rust toolchain (at least Rust 1.95 for the current dependencies). The macOS build uses native Metal through wgpu; no Vulkan SDK, MoltenVK bundle, or separate shader conversion command is required. WGSL is translated to MSL at runtime.

From the repository root:

```sh
cargo build --workspace --locked
cargo run -p cli -- run-template --verbose
cargo run -p cli -- edit-template --verbose
```

The template runs exercise the actual pass layouts, rendering, simulation, UI, and presentation. Check pawn movement, terrain rendering, resizing, minimizing/restoring, and closing both windows. In the editor, paint the registered water material to exercise fluid simulation and rendering; the initial template terrain contains no water. Compare startup/pipeline compilation time separately from steady frame performance. Set `RUST_BACKTRACE=1` when collecting a failure report.

> **Limitation:** Successful Apple MSL compilation, presentation, simulation correctness, and M1 performance require testing on macOS.
