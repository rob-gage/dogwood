# Agent instructions

## Keep documentation accurate

`documentation/content/` (Markdown, built with Zola 0.23) documents the current implementation. A change that
makes a page wrong must fix that page in the same change set.

- **Find affected pages:** each page lists its files in front matter `[extra] sources = [...]`;
directories cover subtrees. Grep for changed paths (and parent directories) and for renamed or removed
identifiers. Keep `sources` lists current when files are added, moved, or removed.
- **Cross-system pages that are easy to miss:**
  - Frame and scene step order: `architecture/` (`frame-lifecycle`, `scene-pipeline`, `concurrency-and-gpu-sync`, `resources-and-lifetimes`).
  - Constants: `architecture/resources-and-lifetimes.md` and `architecture/glossary.md`.
  - WGSL or buffer packing: `internals/shaders/`.
  - Actor components: `external-api/actor-components.md` and `internals/actors/component-consumers.md`.
  - Errors and panics: `architecture/errors.md`.
  - Re-exports: `external-api/_index.md` and example imports.
  - `template/` and CLI: `getting-started/` and both `cli.md` pages.
  - Implementation status: the `Not implemented` and `Limitation` notes.
- **Verify:** run `./documentation/site.sh check`. It builds the site and checks `@/` links, `sources` paths,
source coverage, and rustdoc links. With `RUSTDOC=0`, rustdoc links are skipped. The check can't validate
prose, so re-read affected pages against the source, make sure the Rust examples still compile, and say
what you could not verify.
- **Never** run `site.sh publish` unless asked; it pushes to `gh-pages`.

## Writing

- Keep pages short and factual. Each fact has one canonical page; elsewhere, summarize it in a
sentence and link to it.
- Mark gaps with `> **Not implemented:**`, `> **Partial:**`, or `> **Limitation:**`. Don't invent rationale.
- Use `architecture/glossary.md` terms. Put identifiers in backticks and cite file paths without line numbers.
- Link pages as `[Text](@/path.md)`. Link Rust items with the component `{{ <rustdoc path="engine/...html" text="..." /> }}`,
adding `private={true}` for private-only items. Content is templated, so wrap literal `{{`/`{%` in `{% raw %}`.
- Diagrams are fenced `text` blocks. No JavaScript, Node tooling, GitHub workflows, auto-deploy, or
committed build output (it goes outside the checkout and in `target/documentation`).
