---
name: asbuilt
description: Use when a repo has an `asbuilt.toml` or a `docs/architecture/model.c4`, when `asbuilt check` fails in a pre-commit hook or CI, or when asked to draw, update or explain a code base's architecture with LikeC4. Covers survey, check, externals and theme config, curated views, the documentation tree, and what the model does and does not record.
license: MIT OR Apache-2.0
metadata:
  version: "0.5.1"
---

# asbuilt

`asbuilt` keeps a [LikeC4](https://likec4.dev) architecture model that
describes the code as it is, the way as-built drawings describe a
building as constructed rather than as designed. `asbuilt survey` reads
the code base and writes the `.c4` model; `asbuilt check` surveys again
and exits 1 with a diff when the committed model no longer matches.
Nothing in the model is hand-written and nothing in the code is
annotated, so the model cannot drift from the code without `check`
saying so.

## When to use

- The repo has an `asbuilt.toml` at its root, or a committed
  `docs/architecture/model.c4`.
- A pre-commit hook or CI step running `asbuilt check` is red.
- You are asked to draw, update or explain the architecture of a Rust
  code base.

## The three rules

1. **Never edit `model.c4`.** It is generated. Run `asbuilt survey` and
   commit the result; that is the whole fix for a red `check`.
2. **Curated views go in a sibling file** (`docs/architecture/views.c4`,
   say) that references generated ids. `asbuilt validate` runs LikeC4's
   parser over the directory and rejects a stale id, which is the gate
   for hand-written views.
3. **Externals go in `asbuilt.toml`, not in the code.** Nothing static
   says a module spawns a Node process; `[[externals]]` does, and a
   `from` that names no generated element is an error, not a missing
   edge.

## What the model records

- One **container** per crate (id is the crate name with `_`, title the
  package name, technology `library crate`, `proc-macro crate`,
  `binary` or `test crate`), one **component** per module nested in the
  real hierarchy, plus one `tests` and one `examples` component per
  crate that has them, and one `bin`-tagged component per bin beside a
  lib.
- Every element carries `metadata { path }` and a `link` relative to
  the model file, and the first paragraph of the module's `//!` doc as
  its description.
- One **relation** per (source module, target module) pair, kind the
  strongest evidence found (`implements` > `constructs` > `calls` >
  `names` > `uses`), label the referenced item names, sorted. Paths
  resolve through `pub use` chains and glob re-exports to the defining
  module. An edge between a module and its own ancestor or descendant
  is never recorded: `mod x;` is structure, not coupling.
- Generated views: `index` (containers and externals) and one scoped
  view per element with children, named `view_<id with _ for .>`.

## What it does not record

A macro's body counts only when it parses as Rust (comma-separated
expressions, statements, or items), so `assert_eq!`, both forms of
`vec!` and `fuzz_target!` contribute their references, while a body in
another syntax (Leptos `view!`, a `quote!` with `#var` interpolations)
records only the macro's own path. Method calls on values are not
resolved (`conn.send()` records nothing), and `#[cfg(test)]` items are
skipped. A module that reaches another only through those is missing
its edge; the fix is in asbuilt, not in a hand-written overlay.

## Commands

`survey` and `check` need only cargo. `validate`, `export json`,
`render` and `docs` shell out to `npx likec4@1.59.3`, and `render` and
`docs` also need Graphviz `dot` (`docs --no-render` reuses rendered
SVGs and needs neither). `asbuilt docs` writes a static HTML tree
(index, one page per crate with its modules and relations, the
diagrams embedded) that serves from any directory; mount it under a
docs site or Pages, with `[docs] home_url` for a link back to the host
and `[docs] stylesheet` for its palette. `docs` replaces only the files
an earlier run wrote and refuses, before rendering, to overwrite tree
files it did not write unless `--force`. `[theme]` colors each element
kind in the rendered diagrams, and the docs pages inline the views and
switch them between the light and dark colors with the page, which a
visitor can pin to System, Light or Dark. Rendered SVGs and the docs tree are
build output: commit `model.c4` and curated `.c4` files, and ignore
`views/` and the tree. Details, exit codes and the `asbuilt.toml` keys:

- [`references/cli.md`](references/cli.md): every subcommand and its exit codes.
- [`references/config.md`](references/config.md): `asbuilt.toml`.
- [`references/model-ids.md`](references/model-ids.md): how ids are spelled, for curated views.
