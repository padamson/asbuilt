# asbuilt command line

Every subcommand takes an optional `[root]` (the repository root, the
current directory by default) and the global `--config <path>` to read a
config from somewhere other than `<root>/asbuilt.toml`.

| Command | Does | Needs |
|---|---|---|
| `asbuilt survey [root] [-o PATH]` | writes the model at the configured path, or at `-o` (relative to the root); creates parent directories; prints nothing on success | cargo |
| `asbuilt check [root]` | surveys in memory and compares with the committed model | cargo |
| `asbuilt validate [root]` | `likec4 validate` over the model directory, which also checks curated `.c4` files beside the model | Node |
| `asbuilt export json [root] [-o PATH]` | `likec4 export json`, normalized (the machine-specific `links[].relative` removed), to `<model dir>/model.json` by default | Node |
| `asbuilt render [root] [-o DIR]` | `likec4 gen dot` then `dot -Tsvg`, one SVG per view with its `.dot` beside it, into `<model dir>/views` by default; see "What `render` replaces" | Node, Graphviz |
| `asbuilt docs [root] [-o DIR] [--no-render] [--force] [--title TEXT] [--source-url URL] [--home-url URL] [--home-title TEXT] [--stylesheet URL] [--color-scheme SCHEME] [--no-scheme-toggle]` | a static HTML tree (index, one page per crate with modules and relations, the views inlined) into `<model dir>/site` by default; renders first unless `--no-render`; exits 1 on a stale model; see "What `docs` writes" | Node, Graphviz (neither with `--no-render`) |
| `asbuilt --version` | the version, with the build commit when not a tagged release | |

## What `render` replaces

`render` writes into a scratch directory inside the output directory and
touches nothing else until every view has rendered. Then it removes the
views an earlier render made that the model no longer has (a `.dot`
LikeC4 wrote, recognizable by its `likec4_viewId` attribute, and the
`.svg` beside it), moves the new files in, and removes the scratch
directory. A failed render (no Node, no `dot`) leaves the previous SVGs
in place, and files no render made, such as a logo in a shared asset
directory, are never touched.

## What `docs` writes

The tree uses these names, and `docs` treats them as its own:
`index.html`, `views.html` (only when there are curated views),
`style.css` (the page), `theme.css` (the diagrams), `theme.js` (the
scheme control, unless `--no-scheme-toggle`), `containers/<id>.html`,
and `views/<view>.svg` copied from the render. The header draws the
asbuilt mark before the tree's title, in the page's `--accent`. Every page carries
`<meta name="generator" content="asbuilt docs">`, and every link is
relative, so the tree serves from any directory. This layout, the
classes and the variables below are stable within 0.x; a change to any
of them is called out in the changelog.

A view LikeC4 drew (its `.dot` carries `likec4_viewId`) is inlined as
`<svg class="c4" data-view="<view>">`, each node and group box classed
`c4-k-<kind>` by element kind, a node's secondary text `c4-muted`, an
edge label's backing `c4-label-bg`. Any other SVG is an `<img>`.
`theme.css` colors the inlined views from `[theme]`: per kind,
`--c4-<kind>-fill`, `-stroke`, `-text` and `-muted`, for light and for
dark; group boxes are a tint of their kind's color over `--bg`, and
edges and labels follow `--fg`, `--bg` and `--muted` (`--c4-edge`,
`--c4-edge-text`, `--c4-label-bg`). A host stylesheet can restate any of
these.

The pages follow the system's light or dark setting. With the scheme
control (the default), a visitor can pick System, Light or Dark in the
header; the choice is kept in `localStorage` under
`asbuilt-docs-scheme` for the whole site, and applied before the first
paint. The control ships hidden and the script reveals it, so without
JavaScript a page follows the system setting and shows no dead control.
`--color-scheme light` or `dark` (or `[docs] color_scheme`) sets the
scheme before a visitor chooses, written on the root as `data-theme`,
so it holds without the script too; `--no-scheme-toggle` (or `[docs]
scheme_toggle = false`) leaves the control and the script out.

Before writing, `docs` removes the files under those names that an
earlier run wrote, so a renamed crate's page or a dropped view cannot
ship; anything else in the directory (a `.gitkeep`, a host's own pages)
stays. When one of those names exists and no earlier run wrote it (the
`index.html` is missing or lacks the generator meta), `docs` exits 2
before rendering, and `--force` replaces them. The files are removed
only once the render and the pages are ready, so a failed render keeps
the previous tree. Writing into the model directory itself is allowed:
its `views/` is the render's own and is left alone.

`--home-url` and `--stylesheet` (or `[docs] home_url` and `stylesheet`)
resolve from each page's depth when relative, so `../` names the
directory above the tree from every page.

## Exit codes

- `0`: done, or the model is current.
- `1`: `check` found drift (the unified diff is on stdout, the verdict
  on stderr), `docs` refused because the committed model is stale, or
  `validate` found the model directory invalid (LikeC4's diagnostics on
  stderr).
- `2`: anything else: no model yet, no `Cargo.toml` at the root, a
  config typo, an externals `from` or a `[theme]` key naming nothing, a
  bin named like a module, `npx` or `dot` missing, `docs` refusing tree
  files it did not write. The message names the file, id or directory.

## The pre-commit hook and CI step

```yaml
# .pre-commit-config.yaml (pre-commit or prek)
repos:
  - repo: https://github.com/padamson/asbuilt
    rev: v0.2.0
    hooks:
      - id: asbuilt-check
```

The hook is defined in asbuilt's `.pre-commit-hooks.yaml` and runs the
`asbuilt` on the PATH (`language: system`), so install the release `rev`
names; a stale model reported right after an upgrade is the binary and
the committed model disagreeing about the new release's output, fixed by
`asbuilt survey`. It runs when a `.rs` file, a `Cargo.toml`,
`asbuilt.toml` or anything under `docs/architecture/` changes; a model
kept elsewhere (`[output] path`) overrides `files:` on the hook:

```yaml
      - id: asbuilt-check
        files: (\.rs$|Cargo\.toml$|asbuilt\.toml$|^arch/)
```

In CI, `asbuilt check` after the test step on every platform proves the
survey is byte-identical across them; a Linux-only step can add
`asbuilt validate` where Node is available.

## Installing

```bash
cargo install asbuilt                                             # from crates.io
cargo install --git https://github.com/padamson/asbuilt asbuilt   # from main
```
