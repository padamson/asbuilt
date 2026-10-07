<h1 align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="brand/lockup-dark.svg">
    <img src="brand/lockup-light.svg" width="360" alt="asbuilt">
  </picture>
</h1>

<p align="center">
  <a href="https://crates.io/crates/asbuilt"><img src="https://img.shields.io/crates/v/asbuilt.svg" alt="crates.io"></a>
  <a href="https://docs.rs/asbuilt-core"><img src="https://docs.rs/asbuilt-core/badge.svg" alt="docs.rs"></a>
  <a href="https://github.com/padamson/asbuilt/actions?query=branch%3Amain"><img src="https://img.shields.io/github/check-runs/padamson/asbuilt/main?label=CI&amp;logo=github" alt="CI"></a>
  <a href="#license"><img src="https://img.shields.io/crates/l/asbuilt" alt="License: MIT OR Apache-2.0"></a>
  <a href="https://likec4.dev"><img src="https://img.shields.io/badge/dynamic/regex?url=https%3A%2F%2Fraw.githubusercontent.com%2Fpadamson%2Fasbuilt%2Fmain%2Fcrates%2Fasbuilt%2Fsrc%2Flib.rs&amp;search=LIKEC4_VERSION%3A%20%26str%20%3D%20%22(%5B%5E%22%5D%2B)%22&amp;replace=%241&amp;label=LikeC4&amp;color=45ba4b" alt="LikeC4 version pinned by the CLI"></a>
  <img src="https://img.shields.io/badge/MSRV-1.88-555" alt="MSRV 1.88">
  <a href="https://skills.sh/padamson/asbuilt"><img src="https://skills.sh/b/padamson/asbuilt" alt="skills.sh"></a>
</p>

Keep a [LikeC4](https://likec4.dev) architecture model that describes the
code as it is, the way as-built drawings describe a building as
constructed rather than as designed.

`asbuilt survey` reads a code base and writes the `.c4` model: crates as
containers, modules as components, module-to-module references as
relations labeled with the item names they reference, and the metadata
that ties every element to a path in the tree. Nothing in the model is
hand-written and nothing in the code is annotated. `asbuilt check` surveys
again and fails when the committed model no longer matches, so it runs as
a pre-commit hook and as a CI step. Validation, export and rendering are
LikeC4's; `asbuilt` shells out to a pinned `npx likec4` for those and
reimplements none of it.

The core is language-agnostic, with one front-end per language. The Rust
front-end (`asbuilt-rust`) ships first. It is implemented in Rust so it
installs as one static binary that a pre-commit hook in any repo can
call, and so later front-ends can use Rust-native parsers without a Node
or Python runtime present.

The landing site is at https://padamson.github.io/asbuilt/, versioned
per release, and every snapshot carries this repo's own architecture
documentation under `architecture/`: `asbuilt docs` over the model that
`asbuilt survey` wrote for this very code base, with the views LikeC4
rendered. The site is built, driven with playwright-rs, and deployed by
`.github/workflows/pages.yml` (`docs/versioned-site.md`).

## Status

0.2.0. The model, the Rust front-end, `survey`, `check`, the LikeC4
wrappers and `docs`, now with a viewer (a readable scale, zoom and pan,
nodes that link to their pages, edges that open the relations behind
them), are in use on this repo and on
[playwright-rust](https://github.com/padamson/playwright-rust). Before
1.0 a minor version may change the model's shape or the CLI, and
`CHANGELOG.md` names every such change; 0.2.0 changes every model with
a bin, tests or examples.

## Installation

```bash
cargo install asbuilt
cargo binstall asbuilt     # the release's prebuilt binary, no compile
```

Each GitHub release also carries the archives directly: Linux
(static, for x86_64 and aarch64), macOS and Windows, each with a
build-provenance attestation (`gh attestation verify <archive> --repo
padamson/asbuilt`). To try what is on `main` before it is released, `cargo install --git
https://github.com/padamson/asbuilt asbuilt`.

`survey` and `check` need only cargo. `validate`, `export json`,
`render` and `docs` shell out to `npx likec4` (Node) at the release
the badge above names, and
`render` and `docs` also need Graphviz `dot`; `docs --no-render`
reuses SVGs already rendered and needs neither.

## Usage

```bash
asbuilt survey                 # writes docs/architecture/model.c4
asbuilt check                  # exit 1 with a diff when the committed model is stale
asbuilt validate               # likec4 validate over the model directory (and curated views beside it)
asbuilt export json            # docs/architecture/model.json, machine-independent
asbuilt render                 # one SVG per view under docs/architecture/views/
asbuilt docs                   # a static HTML tree under docs/architecture/site/, embedding those SVGs
```

Every subcommand takes an optional root (the current directory by
default) and `--config <path>`. Exit codes: 0 done or current, 1 drift or
an invalid model, 2 anything else, with the file or id in the message.

### What the model says

One container per crate, one component per module nested as in the code,
a `tests`, an `examples` and a `bin` element per crate that has them
(each a kind of its own, so `[theme]` colors them apart), and one
relation per pair of modules with the referenced item names as the label
and the strongest evidence as the kind (`implements`, `constructs`,
`calls`, `names`, `uses`). Descriptions come from the first paragraph of
each module's `//!` doc. Paths resolve through `pub use` chains and glob
re-exports to the defining module; an edge from a module to its own
ancestor or descendant is never recorded. A macro's body counts when it
parses as Rust (`assert_eq!`, `vec!`, `fuzz_target!`); method calls on
values are not resolved.

### Configuration

Optional `asbuilt.toml` at the root:

```toml
asbuilt = "0.3.0"                                   # the release the model is surveyed by

[output]
path = "docs/architecture/model.c4"

[docs]
title = "playwright-rust"
source_url = "https://github.com/padamson/playwright-rust/blob/main/"
home_url = "../"                                    # a link back to the hosting site on every page

[theme]
container = "#f0a884"                               # the diagrams in your palette, per element kind

[rust]
extra_manifests = ["crates/site-e2e/Cargo.toml"]   # crates in the repo but outside the workspace
include_tests = true
include_examples = true

[[externals]]                                       # what the code cannot state
id = "node_driver"
kind = "process"
title = "Playwright driver"
technology = "Node.js process"

[[externals.relations]]
from = "playwright_rs.server.playwright_server"
title = "spawns"
technology = "stdio"
```

`asbuilt` pins the release: every command run by another one stops
before reading anything else and names the `cargo install` line for the
pinned one, so a contributor with an older or newer binary is not shown
its output as drift. It is a top-level key, so it goes above the first
table. A `from` that names no generated element fails the survey, so a
typo is an error rather than a missing edge. `[docs] title` and `source_url`
name the documentation tree and turn its paths into links;
`home_url`, `home_title` and `stylesheet` fit it into a host site.
`docs` replaces only the files an earlier run wrote, and refuses to
overwrite tree files it did not write unless `--force`. `[theme]`
colors each element kind in the rendered diagrams, with a light and a
dark color the docs pages switch between; a visitor can pick System,
Light or Dark in the header, and `[docs] color_scheme` and
`scheme_toggle` set the default or leave the control out. Every
diagram sits in a frame at a readable scale with Fit, 1:1, Wide and
Fullscreen controls, zooms with the wheel and Ctrl or ⌘ (or a pinch)
and pans by drag; `[docs] viewer = false` (or `--no-viewer`) leaves
the viewer and its script out. Curated views go in
a sibling `.c4` file that references generated ids; `asbuilt validate`
catches a stale one. Rendered SVGs and the docs tree are build output:
commit `model.c4` and the curated views, and ignore the rest.

### Keeping it honest

```yaml
# .pre-commit-config.yaml (pre-commit or prek)
repos:
  - repo: https://github.com/padamson/asbuilt
    rev: v0.2.0
    hooks:
      - id: asbuilt-check
```

The hook runs the `asbuilt` on the PATH, so install the release `rev`
names; with the release pinned in `asbuilt.toml`, any other one stops
with the install line instead of reporting drift. It runs when a `.rs`
file, a `Cargo.toml`, `asbuilt.toml` or anything under
`docs/architecture/` changes; a model kept elsewhere (`[output] path`)
sets `files:` on the hook to include its directory.

In CI, one step:

```yaml
- uses: padamson/asbuilt@v0.3.0
```

The action reads the release from the pin in `asbuilt.toml` (or its
`version` input), installs that release's archive only once `gh
attestation verify` says asbuilt's release workflow built it from that
tag (a release with no archive for the runner is built with `cargo
install` instead, and the run says so), and runs `asbuilt check`.
`command:` runs any other subcommand instead (`docs -o site` writes the
documentation tree), installing Graphviz for `render` and `docs` when
the runner lacks it; `command: ''` installs only. A newer release is a notice in the run, never a failure. Run it
on every platform the code builds on: the survey is byte-identical
across them.

## Brand

The asbuilt mark (a telescope sighting over a graduated arc) and the
AS/BUILT wordmark live in [`brand/`](brand/), with the fonts they need
and a `VERSION` a copy records. A site that links to asbuilt can copy
the directory whole; its README says how each piece is used.

## Agent skill

```bash
npx skills add padamson/asbuilt
```

Works with [Claude Code](https://claude.ai/code),
[Codex](https://openai.com/codex/), [Cursor](https://cursor.com), and any
other [compatible agent](https://agentskills.io/clients). The install copies
the skill into your repo and records its source in `skills-lock.json`.

## Development

See [CLAUDE.md](CLAUDE.md) for development commands.

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (MSRV: 1.88)
- [prek](https://github.com/j178/prek) for pre-commit hooks: `cargo install prek && prek install`
- Node (for `npx likec4`) and Graphviz `dot`, only for the tests and
  commands that validate or render a model

### Build and test

```bash
cargo build --workspace
cargo nextest run --workspace
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.
