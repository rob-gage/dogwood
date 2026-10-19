+++
title = "Your first project"
description = "Generate a Dogwood project and use the CLI to build, run, or open it in the editor."
weight = 20
[extra]
sources = ["cli/src/main.rs", "cli/src/commands", "cli/src/template/template_parameters.rs", "cli/src/template/template_source.rs", "template/Cargo.toml", "template/src", "template/assets"]
+++

This page is for game programmers who have installed the `dogwood` CLI. It creates a project from the template, then builds, runs, and edits it.

## Create a project

From the directory where you want the project folder, run:

```sh
dogwood new falling-stones
cd falling-stones
```

`dogwood new NAME` creates `./NAME`; pass `--path PATH` to choose another destination, which must not already exist.

Choose a valid project name; see the [CLI reference](@/external-api/cli.md#project-names) for the naming rules.

### Generated layout

The project contains a library for the sample game, a default game executable, an optional editor executable, and the template assets:

```text
Cargo.toml
src/
  lib.rs
  main.rs
  template_game.rs
  template_scene_generator.rs
  bin/editor.rs
assets/sprites/
```

The default `main.rs` creates an `Engine`, builds `TemplateGame`, and launches it. Cargo's default run target is the project-named binary. The `editor` binary is enabled by the `editor` Cargo feature.

## Build and run

Run the CLI from the project directory. These commands use release mode by default.

```sh
dogwood build
dogwood run
dogwood edit
```

| Command | Default behavior | Options |
|---|---|---|
| `dogwood build` | Builds in release mode. | `--debug` selects debug mode; `--output PATH` or `-o PATH` sets Cargo's target directory. |
| `dogwood run` | Builds and runs in release mode. | `--debug` selects debug mode. |
| `dogwood edit` | Builds and runs the `editor` binary in release mode with the `editor` feature enabled. | No debug or custom output-directory option. |

All three commands also accept:

- `--path PATH` (default `.`): the project directory.
- `--verbose`: prints the Cargo commands and streams their output.

### Build output location

For `--output` path handling and relative-path caveats, see the [CLI reference](@/external-api/cli.md).

## Engine dependency

The generated `Cargo.toml` uses path dependencies to the `engine` and `editor` crates in the Dogwood checkout where the CLI was built. These paths are fixed when `dogwood new` runs, so keep the checkout at that location for generation and builds.

To use registry dependencies instead, set `DOGWOOD_ENGINE_VERSION` to a valid semantic version before generating the project. That version applies to both dependencies, and `editor` stays optional.

## Template commands

To try the canonical sample without creating a project in the current directory, use `dogwood run-template` or `dogwood edit-template`. All flags are in the [CLI reference](@/external-api/cli.md).

Next, read [core concepts](@/getting-started/core-concepts.md) for the mental model, then the [template walkthrough](@/getting-started/template-walkthrough.md).
