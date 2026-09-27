# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added
- Workspace of three crates created from `rust-project-template`: `asbuilt` (the CLI), `asbuilt-core` (model, LikeC4 emitter, config, drift check) and `asbuilt-rust` (the Rust front-end). One version for all three; a `vX.Y.Z` tag publishes them together
- `asbuilt --version` names the build commit when the build is not a tagged release
- `asbuilt_rust::detect`: a root with a `Cargo.toml` file is a Rust code base
- `asbuilt-core` model: `Model`, `Element`, `Relation`, `RelationKind` in precedence order (implements, constructs, calls, names, uses), an empty `Deployment`; `normalize` merges relations per pair and fixes the output order; `validate` reports two ids that become the same LikeC4 identifier
- `asbuilt-core` config: `asbuilt.toml` with `[output]`, `[[externals]]` and their relations; a front-end's own table (`[rust]`) is kept raw and parsed by the front-end with its own schema; unknown keys inside `[output]` or an external are rejected and the error names the file
- `asbuilt-core` emitter: a `Model` to LikeC4 text (specification, nested elements with tags first, `metadata { path }` and a `link` relative to the output file, relations labeled with the item names, an `index` view plus one scoped view per element with children); the header carries no version so a release is not drift
- `asbuilt-core` externals: `[[externals]]` become tagged elements and their relations `uses` edges; a `from` naming no element is an error naming the external and the `from`
- `asbuilt::likec4::validate` runs the pinned `npx likec4 validate`; two `likec4_` tests are gated on Node and run by the new `LikeC4 validate` CI job
- `asbuilt-rust` structure: `[rust]` config (`extra_manifests`, `include_tests`, `include_examples`); crate and target discovery with `cargo metadata --no-deps --offline` over the workspace and each extra manifest (build scripts skipped, duplicate crate names an error); the module tree walk from each target root (`x.rs` or `x/mod.rs`, `#[path]`, inline modules, `#[cfg(test)]` skipped, the first `//!` paragraph as the description), read through a `FileSource` so its tests run in memory
- `asbuilt-rust` references and resolution: a `syn` visitor collects every `use` (expanded, renames, globs, inside function bodies too), `impl Trait for`, struct literal, call and path per module, with `Type::new()`-style calls as construction and `#[derive]` lists parsed; a pure resolver over an in-memory tree handles `crate::`, `self::`, `super::`, crate-name and bare first segments, follows `pub use` chains and glob re-exports to a bounded fixed point, and attributes a path to the module it reaches when it stops early; aggregation keeps the strongest kind per (source, target) pair, sorts and dedups the item names, collapses test and example targets into one component each, and never records an edge to an ancestor or descendant
- `asbuilt-core::survey`: the `Frontend` trait and the survey that runs every front-end recognizing a root, merges their models, applies the externals and checks ids for collisions
- `asbuilt_rust::analyze` and `RustFrontend`: the whole Rust survey, crates to containers (title the package name, technology from the targets, path relative to the surveyed root with `/` on every platform and `.` for the root), modules to components with their file paths and doc paragraphs, a bin beside a lib as a `bin`-tagged component, tests and examples as one component each
- Six fixture workspaces under `crates/asbuilt-rust/tests/fixtures/` with byte-exact `expected.c4` snapshots (`UPDATE_EXPECT=1` rewrites one), each with a claim test on the relation it exists to prove; `likec4_validate_accepts_every_fixture_snapshot` runs every snapshot through the real parser
- `asbuilt survey [root] [-o path]` writes the model at the configured path (parents created, `-o` relative to the root), `--config` reads a config from anywhere; `asbuilt check [root]` surveys in memory and exits 0 when the committed model is current, 1 with a unified diff on stdout when it is stale, 2 on any error (no model yet, no supported stack, a config typo, a bad externals `from`), and needs no Node
- `asbuilt-core::check::compare`: the drift check as a unified diff, line endings normalized first
- A consumer fixture under `crates/asbuilt/tests/fixtures/consumer/` (a workspace, an extra manifest, one external) with its committed `docs/architecture/model.c4`; the CLI tests spawn the binary over it and over scratch copies of it

- `asbuilt validate` (likec4 validate over the model directory, exit 1 when it rejects the model or a curated view), `asbuilt export json` (normalized: `links[].relative` stripped, so the file is the same on every machine), `asbuilt render` (`likec4 gen dot` then Graphviz `dot -Tsvg`, one SVG per view); four more `likec4_` tests
- The `asbuilt` skill (`npx skills add padamson/asbuilt`) written for a consumer agent: the three rules, what the model records, and references for the CLI, `asbuilt.toml` and id spelling
- README usage, configuration and hook sections; CLAUDE.md sections on fixtures and the `likec4_` tests
- asbuilt surveys itself: `asbuilt.toml` at the root (externals: cargo, the LikeC4 CLI, Graphviz), `docs/architecture/model.c4`, curated views in `docs/architecture/views.c4`, an `asbuilt-check` pre-commit hook, an `asbuilt check` step on every CI platform and an `asbuilt validate` step on the LikeC4 job

- `asbuilt docs [root] [-o DIR] [--no-render] [--title] [--source-url]`: a static HTML documentation tree generated natively from the survey (index, one page per container with its modules, relations both ways) embedding the SVGs `render` produced; refuses to write when the committed model is stale; `[docs] title` and `source_url` in `asbuilt.toml`; `--no-render` for machines without Node
- `asbuilt_core::docs::generate`, the pure generator behind it, with pulldown-cmark for the description paragraphs; `emit::view_ids` exposes the view names it shares with the emitter

- `crates/site`: the landing page at padamson.github.io/asbuilt, a Leptos CSR app built by Trunk, excluded from the workspace with its own lockfile; embeds this repo's own context view and links the generated architecture docs; a version switcher for the `/asbuilt/dev/` and `/asbuilt/vX.Y.Z/` snapshots. The site crate is surveyed too, through `[rust] extra_manifests`

- `crates/site-e2e`: the deploy gate, excluded from the workspace with its own lockfile; four `site_` tests drive the Trunk-built page with playwright-rs 0.19 (the landing page as advertised, with trace, HAR, ARIA-snapshot and screenshot receipts; the version switcher against a stubbed manifest under the site prefix; the dev build's unreleased state; the deployed snapshot under its base path with every response checked). Gated with `#[ignore]`; a missing `dist/` fails, it does not skip. The e2e crate and the Chromium it drives are in this repo's model too

### Fixed
- A bare name brought in by a glob import (`use crate::a::*; Thing::new()`) now resolves; modules that glob-import a prelude were missing from the graph
- `#[path]` on a `mod` inside an inline module block is relative to the inline module's directory, as rustc requires
- A bin, `tests` or `examples` component whose id would equal a lib module's is an error naming the package and the name, instead of two elements with one id
- `Model::validate` rejects any repeated id, including an `[[externals]]` id equal to a surveyed crate's
- An empty `[[externals]]` id is rejected
- Surveying one member of a larger workspace scopes the model to the packages under that root; a root with no package under it is an error
- A root-level crate's `tests` and `examples` components get the path `tests`, not `./tests`
- `-o` is resolved against the canonical root, so an absolute path, `..`, or a `.` root all give correct `link` lines
- Two element ids that collapse to one view id (`a.b_c`, `a_b.c`) get distinct view names
- `link` values are percent-encoded, so a space or quote in a path cannot break the parse
- The reserved-word list is every keyword token of likec4 1.59.3's grammar, probed one by one, not a hand-picked sample; `icons` (a playwright-rust module) was the one that got through

[Unreleased]: https://github.com/padamson/asbuilt/commits/main
