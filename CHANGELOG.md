# Changelog

All notable changes to this project are documented here.
The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added
- Workspace of three crates created from `rust-project-template`: `asbuilt` (the CLI), `asbuilt-core` (model, LikeC4 emitter, config, drift check) and `asbuilt-rust` (the Rust front-end). One version for all three; a `vX.Y.Z` tag publishes them together
- `asbuilt --version` names the build commit when the build is not a tagged release
- `asbuilt_rust::detect`: a root with a `Cargo.toml` file is a Rust code base

[Unreleased]: https://github.com/padamson/asbuilt/commits/main
