# Documentation scaffold

Requires Zola and, for publishing, Git and authenticated access to the publishing remote.
No Node.js, theme downloads, or additional build tools are used.

```sh
./documentation/site.sh build
./documentation/site.sh serve
# Manual deployment only:
./documentation/site.sh publish
```

`build` prints a retained `/tmp/dogwood-docs.*/site` directory. Remove that temporary
parent directory when finished. Preview and publishing temporary files are cleaned
up automatically. Direct Zola builds into `documentation/public/` are also ignored.

The default deployment URL is `https://rob-gage.github.io/dogwood/`. Set `BASE_URL`
for a different build/deployment URL and `PAGES_REMOTE` for a remote other than
`origin`. Configure GitHub Pages to deploy from the root of `gh-pages` in the
repository's Pages settings. No workflow or automatic publishing is included.

Publishing builds first, then uses a temporary worktree based on the remote
`gh-pages` tip, the local branch if only that exists, or a new orphan branch.
Only generated site output and `.nojekyll` are published. Existing Pages content
is replaced. The script commits in the temporary worktree and pushes without
force; the source checkout, branch, index, and uncommitted files are preserved.
A failed push can be retried by rerunning the command. A newly created local
`gh-pages` branch is retained. Builds use the current files, including uncommitted
Markdown changes.

## Source layout

- `content/`: authored Markdown sections, with titles and concise topic outlines
  in `_index.md` front matter; section bodies are intentionally empty.
- `templates/`: shared layout, section navigation, and a page template for later content.
- `static/style.css`: local responsive styling.
- `config.toml`: site settings and deployment URL.
- `site.sh`: local build, preview, and explicitly invoked publishing.

The hierarchy covers getting started, usage examples, external API, internal API,
architecture, and deep internals. Internal sections track runtime/host, world
streaming/persistence, actors, simulation, rendering/UI, and shaders. The outline
is derived from the current workspace and template; it does not imply implemented
audio features or provide example code.

Generated Rust API references will be integrated separately under
`static/rustdoc/` (ignored, reserved URL `/rustdoc/`). No Rust API generation or
link is enabled yet. Authored API guides remain in `content/external-api/` and
`content/internal-api/`. Do not copy generated references into Markdown sources.
Markdown and small TOML front-matter records are the content source; templates,
CSS, deployment, and the future Rust API integration can be replaced independently.
