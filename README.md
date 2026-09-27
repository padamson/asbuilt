# asbuilt

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

## Status

Pre-release. The Rust front-end, `survey`, `check`, and the LikeC4
wrappers work; the first consumer is being wired up. Until 0.1.0 is on
crates.io, install from `main`.

## Installation

```bash
cargo install --git https://github.com/padamson/asbuilt asbuilt
```

`survey` and `check` need only cargo. `validate`, `export json` and
`render` shell out to `npx likec4@1.59.3` (Node), and `render` also
needs Graphviz `dot`.

## Usage

```bash
asbuilt survey                 # writes docs/architecture/model.c4
asbuilt check                  # exit 1 with a diff when the committed model is stale
asbuilt validate               # likec4 validate over the model directory (and curated views beside it)
asbuilt export json            # docs/architecture/model.json, machine-independent
asbuilt render                 # one SVG per view under docs/architecture/views/
```

Every subcommand takes an optional root (the current directory by
default) and `--config <path>`. Exit codes: 0 done or current, 1 drift or
an invalid model, 2 anything else, with the file or id in the message.

### What the model says

One container per crate, one component per module nested as in the code,
a `tests` and an `examples` component per crate that has them, and one
relation per pair of modules with the referenced item names as the label
and the strongest evidence as the kind (`implements`, `constructs`,
`calls`, `names`, `uses`). Descriptions come from the first paragraph of
each module's `//!` doc. Paths resolve through `pub use` chains and glob
re-exports to the defining module; an edge from a module to its own
ancestor or descendant is never recorded. Macro bodies are not parsed and
method calls on values are not resolved.

### Configuration

Optional `asbuilt.toml` at the root:

```toml
[output]
path = "docs/architecture/model.c4"

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

A `from` that names no generated element fails the survey, so a typo is
an error rather than a missing edge. Curated views go in a sibling `.c4`
file that references generated ids; `asbuilt validate` catches a stale
one.

### Keeping it honest

```yaml
# .pre-commit-config.yaml
- id: asbuilt-check
  name: asbuilt check
  entry: asbuilt check
  language: system
  pass_filenames: false
  files: (docs/architecture/|crates/|src/)
```

and the same command as a CI step on every platform.

## Agent skill

[![skills.sh](https://skills.sh/b/padamson/asbuilt)](https://skills.sh/padamson/asbuilt)

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
