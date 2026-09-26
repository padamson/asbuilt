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

[Unreleased]: https://github.com/padamson/asbuilt/commits/main
