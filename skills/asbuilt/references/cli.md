# asbuilt command line

Every subcommand takes an optional `[root]` (the repository root, the
current directory by default) and the global `--config <path>` to read a
config from somewhere other than `<root>/asbuilt.toml`.

| Command | Does | Needs |
|---|---|---|
| `asbuilt survey [root] [-o PATH]` | writes the model at the configured path, or at `-o` (relative to the root); creates parent directories; prints nothing on success | cargo |
| `asbuilt check [root] [--format text\|json]` | surveys in memory and compares with the committed model; when current, prints `<model> is current`; on drift, prints the unified diff on stdout and on stderr what changed (`+ module app.store`, `- app.client -[calls]-> app.server`, `~ crate app: description`) then the verdict; `--format json` (the default is `text`) puts all of it in one object on stdout (see "check's JSON") | cargo |
| `asbuilt validate [root]` | `likec4 validate` over the model directory, which also checks curated `.c4` files beside the model | Node |
| `asbuilt export json [root] [-o PATH]` | `likec4 export json`, normalized (the machine-specific `links[].relative` removed), to `<model dir>/model.json` by default | Node |
| `asbuilt render [root] [-o DIR]` | `likec4 gen dot` then `dot -Tsvg`, one SVG per view with its `.dot` beside it, into `<model dir>/views` by default; see "What `render` replaces" | Node, Graphviz |
| `asbuilt docs [root] [-o DIR] [--no-render] [--force] [--title TEXT] [--source-url URL] [--home-url URL] [--home-title TEXT] [--stylesheet URL] [--color-scheme SCHEME] [--no-scheme-toggle] [--no-viewer]` | a static HTML tree (index, one page per crate with modules and relations, the views inlined) into `<model dir>/site` by default; renders first unless `--no-render`; exits 1 on a stale model; see "What `docs` writes" | Node, Graphviz (neither with `--no-render`) |
| `asbuilt --version` | the version, with the build commit when not a tagged release | |

## check's JSON

`asbuilt check --format json` writes one object on stdout and nothing on
stderr unless it fails (exit 2, the error as text). The exit code is the
same as without the flag.

```json
{
  "model": "docs/architecture/model.c4",
  "current": false,
  "readable": true,
  "changes": [
    {"change": "added", "element": {"id": "app.store", "kind": "component", "noun": "module"}},
    {"change": "removed", "relation": {"from": "app.client", "to": "app.server", "kind": "calls"}},
    {"change": "changed", "element": {"id": "app", "kind": "container", "noun": "crate"},
     "fields": ["description"], "kind_was": null},
    {"change": "changed", "relation": {"from": "a", "to": "b", "kind": "constructs"},
     "fields": ["kind", "label"], "kind_was": "calls",
     "names": {"gained": ["New"], "lost": ["old"]}}
  ],
  "diff": "--- a/docs/architecture/model.c4\n+++ b/docs/architecture/model.c4\n…"
}
```

Read the verdict from `current` (or the exit code), never from
`changes`: drift no element or relation explains (a blank line, a
reordered label) is `current: false` with `changes: []`. `readable`
false means the committed model is not one this release wrote: `changes`
is `null` and the diff is all there is; `diff` is `null` when current.

Every change has `change` (`added`, `removed`, `changed`) and either an
`element` (`id`, `kind`, and `noun`, the surveyed language's word for
the kind) or a `relation` (`from`, `to`, `kind`: the fresh kind, or the
committed one when removed). A changed one lists the `fields` that
changed (an element's `kind`, `title`, `description`, `technology`,
`path`, `tags`; a relation's `kind`, `label`, `technology`) and
`kind_was`, the committed kind when `kind` is among them, else `null`. A
changed relation's `names` are those its label `gained` and `lost`;
both are empty when it was only reordered or a name repeated. Ids are as
the `.c4` file writes them.

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
scheme control, unless `--no-scheme-toggle`), `viewer.css` and
`viewer.js` (the diagram viewer, unless `--no-viewer`),
`containers/<id>.html`,
and `views/<view>.svg` copied from the render. The header draws the
asbuilt mark before the tree's title, in the page's `--accent`. Every page carries
`<meta name="generator" content="asbuilt docs X.Y.Z">` and a `<footer>`
naming the same release ("Built with asbuilt vX.Y.Z", with the build
commit when it is not a tagged release); `docs` recognizes a tree as its
own by the meta's `asbuilt docs` prefix, so a tree any release wrote is
replaced without `--force`. Every link but the footer's is relative, so
the tree serves from any directory. This layout, the
classes, and the variables below are stable within 0.x; a change to any
of them is called out in the changelog.

A view LikeC4 drew (its `.dot` carries `likec4_viewId`) is inlined as
`<svg class="c4" data-view="<view>">`, each node and group box classed
`c4-k-<kind>` by element kind, a node's secondary text `c4-muted`, an
edge label's backing `c4-label-bg`. A node whose element the page knows
carries a `<title>` with its id (the hover tooltip) and is wrapped in
an `<a>` to where the element is documented: a crate's page, a module's
section on it, an external's row on the index; the page's own crate is
named but not linked. An edge whose endpoints the page knows carries
`data-from` and `data-to` (their ids), a `<title>` naming them, and an
invisible `c4-hit` twin of its line to click; with the viewer, a figure
carries `<script type="application/json" class="viewer-edges">` mapping
each `from->to` to the relations the edge stands for (endpoint ids and
links, kind, items, technology). Any other SVG is an `<img>`.
`theme.css` colors the inlined views from `[theme]`: per kind,
`--c4-<kind>-fill`, `-stroke`, `-text`, and `-muted`, for light and for
dark; group boxes are a tint of their kind's color over `--bg`, and
edges and labels follow `--fg`, `--bg`, and `--muted` (`--c4-edge`,
`--c4-edge-text`, `--c4-label-bg`), and how strongly a group box takes
its kind's color is `--c4-group-fill-mix` and `--c4-group-stroke-mix`.
A host stylesheet can restate any of these.

Every inlined view sits in a frame the viewer sizes: at the scale that
fits the text column, held between 0.7 and 1, so a wide view scrolls
inside its frame (at most three quarters of the window tall) instead
of shrinking its text, and a small one is not blown up. The wheel with
Ctrl or Cmd held (what a trackpad pinch sends) zooms about the pointer,
the + and − buttons step, and + − 0 work from the keyboard with the
frame focused; a plain wheel scrolls the page as usual, and dragging
pans a view larger than its frame (the grab cursor says so; a view
that fits offers neither; a drag that starts on a node pans without
following its link). A click on an edge opens the relations it stands
for, one line each with both endpoints linked: the whole list behind
an edge LikeC4 labels `[...]`, or every item of one it truncates.
Escape, the close button, or a click elsewhere dismisses it. The bar's hint names the modifier for the platform (⌘ on a Mac,
Ctrl elsewhere) and is left out on a touch-only device. The other
controls: Fit (the whole view at any scale), 1:1 (as
LikeC4 laid it out), Wide (every figure on the site takes the window's
width, kept in `localStorage` under `asbuilt-docs-wide`), Legend, and
Fullscreen. Every diagram carries a legend of what it draws: each
element kind, by the surveyed language's word with a swatch in its
color, and each relation kind its edges stand for, with a sample of
its line and head (a filled triangle is an edge merging several kinds:
"several strong kinds" on a solid line, "several weak kinds" on a
dotted one, "several kinds" dashed), read off the drawing itself, so a filtered
view lists only what it shows. The viewer makes it the frame's last
strip, inside its border, and Legend hides every figure's, kept under
`asbuilt-docs-legend`; without the viewer it is a row under the
diagram. Its styles are in `theme.css`, so an embedded figure has them. The controls
ship hidden and the script reveals them, so without JavaScript a view
is scaled to the column as before. `--no-viewer` (or `[docs] viewer =
false`) leaves the viewer, its stylesheet, and its script out. A host
page can embed one of the tree's figures (the `<figure data-viewer>`
from a page, its links re-based) with the page tokens defined and
`theme.css`, `viewer.css`, and `viewer.js` loaded; the script also sets up a
figure added after the page loads.

The pages follow the system's light or dark setting. With the scheme
control (the default), a visitor can pick System, Light, or Dark in the
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
- `1`: `check` found drift (the unified diff alone on stdout, so it
  pipes as a patch; on stderr a heading, `<model>: N changes` with a
  line per change, or `no element or relation changed; see the diff`,
  or for a committed model this release did not write `not a model this
  release wrote; see the diff`, and then the verdict), `docs` refused
  because the committed model is stale, or
  `validate` found the model directory invalid (LikeC4's diagnostics on
  stderr).
- `2`: anything else: `asbuilt.toml` pinning another release (checked
  before anything runs), no model yet, no `Cargo.toml` at the root, a
  config typo, an externals `from` or a `[theme]` key naming nothing, a
  bin named like a module, `npx` or `dot` missing, `docs` refusing tree
  files it did not write. The message names the file, id, or directory.

## The pre-commit hook and the CI action

```yaml
# .pre-commit-config.yaml (pre-commit or prek)
repos:
  - repo: https://github.com/padamson/asbuilt
    rev: v0.3.0
    hooks:
      - id: asbuilt-check
```

The hook is defined in asbuilt's `.pre-commit-hooks.yaml` and runs the
`asbuilt` on the PATH (`language: system`), so install the release `rev`
names. Pin that release in `asbuilt.toml` (`asbuilt = "X.Y.Z"`) and a
contributor on any other one gets the install line instead of a diff;
the CI action reads the same line, so one edit moves both. Without a pin, a stale model reported right after an upgrade is
the binary and the committed model disagreeing about the new release's
output, fixed by `asbuilt survey`. It runs when a `.rs` file, a `Cargo.toml`,
`asbuilt.toml`, or anything under `docs/architecture/` changes; a model
kept elsewhere (`[output] path`) overrides `files:` on the hook:

```yaml
      - id: asbuilt-check
        files: (\.rs$|Cargo\.toml$|asbuilt\.toml$|^arch/)
```

In CI, the action installs the pinned release and runs `asbuilt check`:

```yaml
- uses: padamson/asbuilt@v0.3.0
  # with:
  #   version: "X.Y.Z"            # instead of the pin in asbuilt.toml
  #   working-directory: path     # where asbuilt.toml is and the command runs
  #   command: validate           # another subcommand; "" installs only
```

It installs the release archive for the runner only after `gh
attestation verify` confirms asbuilt's release workflow built it from
that release's tag (on Linux, the static musl build where the release
has one); with no archive for the platform it runs `cargo install`,
saying so, and checks no attestation. A newer release on
crates.io is a notice, never a failure. Run it on every platform the
code builds on, which proves the survey byte-identical across them. It
runs one command per step, so `validate` (needs Node) is a second step
with `command: validate`, not a replacement for `check`. For `render`,
and `docs` without `--no-render`, it installs Graphviz when `dot` is
missing: apt on Linux (where the runner has it), brew on macOS, choco on
Windows.

## Installing

```bash
cargo install asbuilt                                             # from crates.io
cargo binstall asbuilt                                            # the release's prebuilt binary
cargo install --git https://github.com/padamson/asbuilt asbuilt   # from main
```
