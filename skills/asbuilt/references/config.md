# asbuilt.toml

Optional, at the repository root. Every key has a default; an unknown key
inside `[output]`, `[docs]`, `[rust]`, an `[[externals]]` entry or a
`[theme]` color table is an error naming the file.

```toml
[output]
# Where `survey` writes and `check` reads, relative to the root.
path = "docs/architecture/model.c4"

[docs]
# The title of the documentation tree `asbuilt docs` writes; the root
# directory's name when absent.
title = "playwright-rust"
# Turns every element's path into a link. Absent: paths are plain text.
source_url = "https://github.com/padamson/playwright-rust/blob/main/"
# A link back to the site that hosts the tree, first in every page's
# crumbs and header. Relative values resolve from each page's depth, so
# "../" is the directory above the tree under any mount point.
home_url = "../"
# The text of that link; the URL itself when absent.
home_title = "playwright-rust"
# A stylesheet linked last on every page, resolved like home_url. It can
# restate the page tokens (--bg, --fg, --muted, --accent, --rule, --code,
# --figure-bg) in the host's palette.
stylesheet = "../architecture.css"

[theme]
# A color per element kind as the diagrams draw it: `container`,
# `component`, or an external's `kind`. A kind with no entry keeps
# LikeC4's default. The table form is accepted; the diagrams use `light`.
container = "#f0a884"
process = { light = "#d4d4d4", dark = "#5a5a5a" }

[rust]
# Crates inside the repo but outside the workspace, each with its own
# lockfile. Relative to the root.
extra_manifests = ["crates/site/Cargo.toml", "crates/site-e2e/Cargo.toml"]
# One `tests` component per crate for its test and bench targets.
include_tests = true
# One `examples` component per crate for its example targets.
include_examples = true

[[externals]]
id = "node_driver"            # a LikeC4 identifier; must not collide with a crate
kind = "process"              # the LikeC4 element kind
title = "Playwright driver"
technology = "Node.js process"
description = "The official Playwright server, assembled by build.rs."

[[externals.relations]]
from = "playwright_rs.server.playwright_server"   # a generated element id, or another external's id
title = "spawns"
technology = "stdio"
```

`from` may be written the way the model spells the id or the way LikeC4
will (`-` as `_`). A `from` that names nothing fails the survey with the
external and the `from` in the message; a wrong module name is a
generator error, not a diagram that quietly omits an edge.

`[theme]` colors are declared in the generated specification, so a
theme change changes `model.c4`: re-survey and commit, and `check`
reports drift until you do. A key naming a kind the model does not have
fails the survey with the kinds it does have; a color that is not
`#rrggbb`, a table without `light`, or an unknown key in the table fails
the config with the key or value named.
