---
name: asbuilt
description: Use when a repo has an `asbuilt.toml` or a `docs/architecture/model.c4`, when `asbuilt check` fails in a pre-commit hook or CI, or when asked to document a code base's architecture with LikeC4. The CLI is still being written; this skill fills in as commands land.
license: MIT OR Apache-2.0
metadata:
  version: "0.1.0"
---

# asbuilt

`asbuilt` keeps a LikeC4 architecture model that describes the code as
it is. `asbuilt survey` reads a code base and writes the `.c4` model;
`asbuilt check` surveys again and fails when the committed model no
longer matches. The model is generated, never edited by hand, and the
code carries no annotations for it.

## When to use

- The repo has an `asbuilt.toml` at its root or a `docs/architecture/model.c4`.
- A pre-commit hook or CI step running `asbuilt check` is red.
- You are asked to draw or update an architecture diagram of a code base.

## How it works

The commands are not written yet. What will be true when they are:

- Never edit `model.c4`. Run `asbuilt survey` and commit the result.
- Curated views go in a sibling `.c4` file that references generated ids.
- `asbuilt check` needs no Node; `asbuilt validate` and `asbuilt render`
  shell out to a pinned `npx likec4`.

- [`references/usage.md`](references/usage.md): the CLI surface, as it lands.
