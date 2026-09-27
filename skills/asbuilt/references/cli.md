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
| `asbuilt render [root] [-o DIR]` | `likec4 gen dot` then `dot -Tsvg`, one SVG per view, into `<model dir>/views` by default | Node, Graphviz |
| `asbuilt docs [root] [-o DIR] [--no-render] [--title TEXT] [--source-url URL]` | a static HTML tree (index, one page per crate with modules and relations, SVGs embedded) into `<model dir>/site` by default; renders first unless `--no-render`; exits 1 on a stale model | Node, Graphviz (neither with `--no-render`) |
| `asbuilt --version` | the version, with the build commit when not a tagged release | |

## Exit codes

- `0`: done, or the model is current.
- `1`: `check` found drift (the unified diff is on stdout, the verdict
  on stderr), `docs` refused because the committed model is stale, or
  `validate` found the model directory invalid (LikeC4's diagnostics on
  stderr).
- `2`: anything else: no model yet, no `Cargo.toml` at the root, a
  config typo, an externals `from` naming nothing, a bin named like a
  module, `npx` or `dot` missing. The message names the file or id.

## The pre-commit hook and CI step

```yaml
# .pre-commit-config.yaml
- id: asbuilt-check
  name: asbuilt check
  entry: asbuilt check
  language: system
  pass_filenames: false
  files: (docs/architecture/|crates/|src/)
```

In CI, `asbuilt check` after the test step on every platform proves the
survey is byte-identical across them; a Linux-only step can add
`asbuilt validate` where Node is available.

## Installing

```bash
cargo install asbuilt                                             # from crates.io
cargo install --git https://github.com/padamson/asbuilt asbuilt   # from main
```
