+++
title = "CLI reference"
description = "Commands, options, defaults, and process behavior for the dogwood command-line tool."
weight = 120
[extra]
sources = ["cli/src/main.rs", "cli/src/commands", "cli/Cargo.toml", "cli/src/template", "cli/src/utility/locate_cargo.rs"]
+++

## Usage

The `cli` package installs the `dogwood` binary for creating, building, running, and editing Dogwood projects. Clap supplies `-h, --help`, `-V, --version`, and the `dogwood help [COMMAND]` subcommand.

The canonical template is the starter project that `dogwood new` copies; see [Template walkthrough](@/getting-started/template-walkthrough.md). `run-template` and `edit-template` run that template from a temporary cache without creating a project in the current directory.

| Command | Options | Behavior |
|---|---|---|
| `dogwood new NAME` | `--path PATH` (default `./NAME`) | Creates a project from the canonical template at `./NAME` or `--path`; the destination must not exist. See [Your first project](@/getting-started/first-project.md). |
| `dogwood build` | `--debug`; `--verbose`; `--path PATH` (default `.`); `-o, --output PATH` (default: Cargo metadata's `target_directory`) | Builds the project in release mode by default. `--output` chooses Cargo's target directory. |
| `dogwood run` | `--debug`; `--verbose`; `--path PATH` (default `.`) | Builds, then runs the default Cargo target. Uses release mode by default. |
| `dogwood edit` | `--verbose`; `--path PATH` (default `.`) | Builds in release mode, then runs binary `editor` with feature `editor`. |
| `dogwood run-template` | `--debug`; `--verbose` | Runs the canonical template in the CLI's stable temporary cache. Release mode is the default. |
| `dogwood edit-template` | `--verbose` | Builds the cached canonical template in release mode, then runs its `editor` binary with the `editor` feature. |

### Project names

- Names start with an ASCII letter or digit. Prefer a letter: the validator permits a leading digit, but generated projects with one currently fail to compile.
- Remaining characters may be ASCII letters, digits, `-`, or `_`.
- `CON`, `PRN`, `AUX`, `NUL`, `COM1` to `COM9`, and `LPT1` to `LPT9` are rejected, case-insensitively.

### Output and verbosity

- `--verbose` prints Cargo commands and streams build and run output to the terminal. `cargo metadata` stdout remains captured as JSON.
- Without `--verbose`, build output is captured and emitted if the build fails. `run` and `edit` also pass Cargo `--quiet` for their run step.
- `edit` always uses release mode and has no debug or custom output-directory option.

> **Limitation:** A relative `--output` is joined to `--path`, then Cargo interprets it from the project working directory while Dogwood canonicalizes it from the CLI working directory. When both paths are relative and `--path` is not `.`, this can make the build fail; use an absolute `--path` or `--output` to avoid the ambiguity.

## Examples

```sh
dogwood new cave_game
cd cave_game
dogwood run --debug
dogwood edit --verbose
dogwood build --path . --output /tmp/dogwood-target
dogwood run-template
dogwood edit-template --verbose
```

## Exit behavior

`new` does not invoke Cargo. The other commands require a working Cargo installation.

| Situation | Result |
|---|---|
| Invalid arguments, help, or version requests | Handled by Clap before command dispatch; invalid arguments exit with status 2, while help and version requests exit with status 0. |
| Cargo not found on `PATH` or under Cargo home | Prints installation guidance and exits with status 1. |
| `build` fails (metadata, JSON parsing, missing target directory, process start, build, or canonicalization) | Prints `Build failed.`; exits with Cargo's status where one exists, otherwise 1. |
| `build` succeeds | Prints `Build complete.` |
| `run` or `edit` | Exits with Cargo's exit code when available, otherwise 1. |
| `new` validation, template, or filesystem error; template-cache or command setup error | Returned from `main`; nonzero status. |

### Template cache

`run-template` and `edit-template` share a stable temporary project directory. Cache refreshes preserve files you changed since the previous generated baseline, so edits to the template project can persist across these commands. See [CLI internals](@/internal-api/cli.md) for cache behavior. For editor use, see [Using the editor](@/examples/using-the-editor.md).
