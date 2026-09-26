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

[Unreleased]: https://github.com/padamson/asbuilt/commits/main
