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

The workspace is in place: `asbuilt` (the CLI), `asbuilt-core` (model,
emitter, config, check) and `asbuilt-rust` (the Rust front-end). The
survey itself is being written; `asbuilt --version` is the only command
that does anything yet.

## Installation

```bash
cargo install asbuilt
```

## Usage

```bash
asbuilt --help
```

`asbuilt survey` and `asbuilt check` are documented here as they land.

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
