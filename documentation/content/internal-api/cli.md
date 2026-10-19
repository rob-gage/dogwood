+++
title = "CLI internals"
description = "How project templates are rendered and cached, and how CLI commands find and invoke Cargo."
weight = 80
[extra]
sources = ["cli/src/template", "cli/src/utility", "cli/src/commands", "cli/src/main.rs"]
+++

## Template source and parameters

This page is for contributors who change how `dogwood` generates projects, caches the shared template, or invokes Cargo. It covers template rendering, the template cache, and Cargo discovery.

### Source selection

- `TemplateSource::canonical()` prefers the editable `template` directory beside the CLI crate when it exists. Otherwise it selects the embedded snapshot compiled with `include_dir`.
- Callers can also construct an explicit filesystem or embedded source.
- File enumeration returns relative paths and bytes in sorted path order.

### Rendering rules

- `rendered_files` treats files with `.rs`, `.toml`, `.md`, `.txt`, or `.ron` extensions as UTF-8 text and substitutes `{% raw %}{{field_name}}{% endraw %}` placeholders. Other files are copied byte-for-byte.
- Empty sources, unsafe relative paths, unclosed placeholders, empty or invalid field names, and unknown or missing parameter names return errors.
- An unmatched closing `{% raw %}}}{% endraw %}` is copied literally, because the renderer only searches for `{% raw %}{{{% endraw %}`.

`TemplateSource::generate` requires a destination that does not already exist. If writing generated files fails, it removes the partially created destination.

### Parameters

`TemplateParameters::new(project_name)` creates four values: the project name, a crate name with hyphens replaced by underscores, and engine and editor dependency fragments.

- When `DOGWOOD_ENGINE_VERSION` is set, it must be UTF-8 and parse as SemVer. That version is used for both dependencies, with the editor marked optional.
- Without it, canonical local `engine` and `editor` paths are embedded in the fragments.
- The template source itself does not choose a Git dependency.

See the [CLI reference](@/external-api/cli.md) for command syntax and the [first project guide](@/getting-started/first-project.md) for generated project structure.

## Shared template cache

`TemplateCache::default_root()` selects the operating-system temporary directory joined with `dogwood/template`. `run-template` and `edit-template` both select this cache with project name `dogwood_template`, then invoke the regular run or edit flow.

### Layout

| Entry | Purpose |
| --- | --- |
| `crate` | The stable generated project. |
| `.dogwood-template-state` | The previous generated-file baseline. |
| `.dogwood-template-fingerprint` | Marker holding the generation fingerprint. |
| `.dogwood-template-state-staging` | Temporary directory used while the baseline is rebuilt. |

### Refresh

The fingerprint covers a format identifier, each sorted source path and file contents, and all sorted parameter name/value pairs. If the cached project exists with a matching marker, `select` returns it unchanged. Otherwise it renders the new source and reconciles files against the prior baseline:

- For rendered files, it updates only files that still match their prior baseline, preserves user edits with their prior baseline, and creates missing files.
- For obsolete files, it removes only files that still match their prior baseline; otherwise it retains the file and baseline.

State is built in the staging directory and renamed into place after reconciliation.

### Safety checks

- Before reading or writing a project file below `crate`, the cache checks the relative path components for symlinks.
- It rejects a symlink or non-directory at the cache crate path.
- It rejects a project with no readable marker rather than replacing it.

> **Limitation:** The fingerprint is a custom pair of 64-bit accumulators. The source does not establish cryptographic collision resistance.

## Cargo discovery and subprocesses

### Finding Cargo

`locate_cargo()` first asks `which` for `cargo`. It then checks `bin/cargo` (`cargo.exe` on Windows) under the Cargo home: `$CARGO_HOME` when set, otherwise the default Cargo home from `home::cargo_home()`.

A candidate is accepted only when `cargo --version` succeeds and its output contains a parseable Cargo SemVer version. If neither works, the function prints installation and PATH guidance and returns `Err(())`.

### Build, run, and edit

The build helper always invokes `cargo metadata --no-deps --format-version 1` in the project directory and uses the reported target directory unless an output directory was supplied. Template-specific commands obtain the cached project before entering these flows.

| Command | Builds first? | Cargo invocation |
| --- | --- | --- |
| `build` | It is the build | `cargo build`, with `--release` unless debug mode is on, and the resolved target directory |
| `run` | Yes, with the build helper | `cargo run [--release] --target-dir <build target directory>`, plus `--quiet` unless verbose |
| `edit` | Yes, release build | `cargo run --release --bin editor --features editor --target-dir <build target directory>`, plus `--quiet` unless verbose |

Quiet mode captures build output and prints it on build failure. Verbose mode prints the metadata and build commands; metadata stdout remains captured for its JSON, while the build command inherits the terminal.

For how a launched game is updated and rendered after startup, see [frame lifecycle](@/architecture/frame-lifecycle.md).
