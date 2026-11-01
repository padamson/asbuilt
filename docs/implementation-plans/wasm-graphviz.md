# Rendering without a system Graphviz - Implementation Plan

**Feature:** `render` and `docs` lay views out with the WebAssembly Graphviz
that LikeC4 already ships, not a system `dot`.

**User Story:** As someone running `asbuilt docs` on my machine or in CI, I
want the diagrams drawn without installing Graphviz, by the same Graphviz
release everywhere, so that a model change cannot fail the build because the
runner's Graphviz is old, and the diagrams I see locally are the ones CI
draws.

**Roadmap:** `roadmap-now-wasm-graphviz` in `crates/site/public/roadmap.json`
(0.4.0). Its acceptance criteria are the three unit tests named in slice 1.

## Why

`render` is the one step that needs a system Graphviz: `likec4 gen dot`
writes one `.dot` file per view, and asbuilt runs `dot -Tsvg` on each. 0.3.1
had to teach the action a per-OS install (`action/install-graphviz.sh`)
because Ubuntu's apt ships Graphviz 2.43, whose spline router fails
("triangulation failed") on some layouts. That install covers only Ubuntu
24.04 on amd64; arm runners, Ubuntu 26.04 (where `ubuntu-latest` is moving),
and every machine outside the action still get whatever `dot` is installed,
and layout differs between Graphviz releases.

## Research (2026-10-31)

- LikeC4 1.59.4, the release asbuilt pins, depends on
  `@hpcc-js/wasm-graphviz` at exactly 1.22.2 (Apache-2.0), a WebAssembly
  build of Graphviz 15.0.0. LikeC4 lays out with it by default; its
  `--use-dot` flag switches to a system `dot`. So the package is already in
  the tree every asbuilt user installs.
- LikeC4 1.60.0, pinned since issue #9, depends on `@hpcc-js/wasm-graphviz`
  at exactly 1.29.2. The measurements below were taken against 1.22.2, so
  they are rerun against 1.29.2 before slice 1.
- LikeC4's CLI has no SVG export (`export png`/`jpg` drive Chromium through
  Playwright), so asbuilt has to call the WebAssembly Graphviz itself.
- A Node script importing the cached package laid out all 10 of this repo's
  views in 81 ms (32 ms of it loading the module), where the system `dot`
  15.1.0 took about a second. It also laid out all 18 of playwright-rust's,
  including `view_playwright_rs_protocol`, the view apt's 2.43 fails on.
- Against the system `dot` 15.1.0, every view had the same nodes and edges,
  and with the numbers stripped the SVGs matched element for element
  (classes, colors, and arrowheads, which the docs legend reads) except that
  a few edge paths had more control points. Canvases differed by at most
  0.6%: the WebAssembly build has no Pango and estimates text widths.
- `@hpcc-js/wasm-graphviz-cli` (1.11.4) is the wrong tool: its `main()`
  catches every error and exits 0, it takes one file per process and writes
  only to stdout, and a path that does not exist is parsed as DOT text.

## Design

- asbuilt carries a short ES module (beside `likec4.rs`, in through
  `include_str!`) that loads `@hpcc-js/wasm-graphviz` once, lays out every
  `.dot` file it is given into an SVG beside it, and exits non-zero after
  printing the file that failed and Graphviz's message.
- `render` writes the module into its scratch directory and runs it through
  npx with the package pinned: `npx --yes -p @hpcc-js/wasm-graphviz@<pin>`
  then `node <module> <files...>`. The pin is a constant beside
  `LIKEC4_VERSION` in `lib.rs`, kept equal to the version LikeC4's own
  `package.json` names; the upstream LikeC4 check names a mismatch when it
  proposes a LikeC4 bump.
- No system-`dot` option. Two render paths would mean two sets of tests and
  output that depends on the machine again, for a gain (Pango's text
  measurement) that the research puts under 1%.
- What goes away: the action's Graphviz step, `action/install-graphviz.sh`
  and `action/needs-graphviz.sh`, the install steps in `test.yml`,
  `pages.yml`, and `upstream-likec4-check.yml`, the action job's
  pinned-version check, `dot_banner` and the old-Graphviz advice with their
  tests, and the `dot_banner` exclude in `.cargo/mutants.toml`.

## Open questions for slice 1

1. **How the module finds the package.** A bare `import` resolves from the
   module's own location, not npx's install. The candidate is to locate the
   `node_modules/.bin` entry npx puts first on `PATH` and import
   `../@hpcc-js/wasm-graphviz/dist/index.js` from there, or to use
   `createRequire` on that directory. Verify on Linux, macOS, and Windows
   (the Test matrix), since `npx -c` quoting differs under `cmd.exe`.
2. **The classifier.** A session running `npx -p @hpcc-js/wasm-graphviz@1.22.2`
   may be blocked as code from outside; an allow rule in
   `.claude/settings.json` for that exact command clears it.

## Slices

### Slice 1: `render` through the WebAssembly Graphviz

Unit tests in `crates/asbuilt/src/likec4.rs`, which are the roadmap's
acceptance criteria:

- `render_lays_out_with_the_pinned_wasm_graphviz`: the command `render`
  builds is npx with the pinned package, and no command is `dot`.
- `every_view_is_laid_out_in_one_run`: one invocation carries every `.dot`
  file.
- `a_view_that_fails_to_lay_out_fails_render_and_is_named`: the module's
  failure output becomes a `LikeC4Error::Failed` that names the view.

Plus a `likec4_` integration test that renders the consumer fixture with an
empty `PATH` apart from Node, which is the proof no `dot` is needed (ignored,
so it backs the criteria rather than being one).

### Slice 2: the action and the workflows

Remove the Graphviz install from the action and from this repo's workflows,
and the action job's version check. The action jobs on all four runners
(`ubuntu-latest`, `ubuntu-24.04-arm`, `macos-latest`, `windows-latest`) must
render with no Graphviz installed.

### Slice 3: docs

`CHANGELOG.md` (a Changed entry: Graphviz is no longer needed, and the
output is laid out by Graphviz 15.0.0 everywhere), the README's
requirements (its Installation note, the action section, and the
development list), `skills/asbuilt/SKILL.md` and `references/cli.md`, and
the skill version.

## Definition of Done

- The three acceptance tests pass and the item's `status` is `done`.
- The `likec4_` lane renders this repo's views and the consumer fixture
  with no `dot` on `PATH`.
- `crates/site/preview.sh` passes (the docs tree is rendered by the new path).
