# Versioned landing site (padamson.github.io/asbuilt)

The landing page is published per version so a visitor on any release
sees the site as it shipped, with a dropdown to switch versions and a
banner when they are not on the latest stable. Every snapshot carries
this repo's own architecture documentation under `architecture/`, the
output of `asbuilt docs` over the committed model.

## Layout (on the `gh-pages` branch)

The repo is served as project Pages, so every path below sits under
`/asbuilt/`:

```
/asbuilt/                       root redirect → newest /asbuilt/vX.Y.Z/ (or /asbuilt/dev/ before the first release)
/asbuilt/versions.json          { "latest": "X.Y.Z", "versions": ["X.Y.Z", … newest-first] }
/asbuilt/v0.1.0/                immutable release snapshot
/asbuilt/v0.1.0/architecture/   `asbuilt docs` for that release
/asbuilt/dev/                   main HEAD (unreleased preview)
/asbuilt/.nojekyll
```

The SPA reads `/asbuilt/versions.json` at runtime to populate the
dropdown (so an old snapshot still lists versions released after it was
built) and compares its own build-time `SITE_VERSION` to `latest` to
decide whether to show the "newer release available" or "unreleased dev
build" banner. `crates/site/src/version.rs` holds the one runtime copy
of the prefix (`SITE_PREFIX`); every other asset path in the app is
relative, so a snapshot mounts anywhere.

## How a build knows its version

`crates/site/build.rs` reads the `SITE_VERSION` env var and bakes it in
via `env!("SITE_VERSION")`. The deploy sets it: `dev` for the main-HEAD
build, the release version (`0.1.0`) for a snapshot. Each snapshot is
built with `trunk build --public-url /asbuilt/<dest>/` so its assets
resolve under the subpath.

## Deploy ([.github/workflows/pages.yml](../.github/workflows/pages.yml))

One job: lint the site crates, `asbuilt render` this repo's model (the
context view is the landing page's example figure), build a root-served
`SITE_VERSION=dev` site with `asbuilt docs --no-render` written into its
`architecture/`, and run the playwright-rs **dogfood gate** against it.
Then build the target snapshot the same way, serve it under its real
base path for the snapshot tests (assets resolve, release-only rendering,
the architecture section links every crate and embeds a view that
loads), drop it into the `gh-pages` worktree under `/<dest>/`, regenerate
`versions.json` and the root redirect
([deploy/update-manifest.sh](../crates/site/deploy/update-manifest.sh)
with the `/asbuilt` prefix), and commit.

Triggers:
- **push to `main`** touching `crates/**`, `docs/architecture/**`,
  `asbuilt.toml` or the workflow → rebuilds `/asbuilt/dev/`. The paths
  are that wide because the documentation is generated from the code.
- **pull request** → runs both gates and deploys nothing. It is a
  required check, so it has no paths filter.
- **`workflow_dispatch` with `version=X.Y.Z`** → publishes
  `/asbuilt/vX.Y.Z/` from the checked-out source.

## One-time bootstrap

1. The first push to `main` after the workflow lands publishes
   `/asbuilt/dev/`, creating `gh-pages` with the manifest (empty
   `latest`) and a root redirect to `dev/`.
2. **Repo setting (manual):** Settings → Pages → Build and deployment →
   Source: **Deploy from a branch** → Branch: **`gh-pages` / (root)**.
   Or:
   ```
   gh api -X POST repos/padamson/asbuilt/pages -f build_type=legacy -f source[branch]=gh-pages -f source[path]=/
   ```

## Per release

After tagging `vX.Y.Z`, publish its snapshot from the tagged source:

```
gh workflow run pages.yml -f version=X.Y.Z --ref vX.Y.Z
```

This builds `/asbuilt/vX.Y.Z/`, makes it the new `latest` (root redirect
and manifest), and leaves older snapshots untouched. It is step 6 of the
release process in `CLAUDE.md`.

## Running the gates locally

The e2e tests need a Trunk-built site and a Chromium that playwright-rs
installed (`cargo install --locked playwright-rs --version 0.19.0
--features cli && playwright-rs install chromium`). The sequence the
workflow runs, from the repo root:

```
cargo run -p asbuilt -- render && cp docs/architecture/views/context.svg crates/site/public/views/
(cd crates/site && SITE_VERSION=dev trunk build --release)
cargo run -p asbuilt -- docs --no-render -o crates/site/dist/architecture
cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml \
  --run-ignored only -E 'test(/^site_/) and not test(/^site_(deployed_snapshot|architecture)/)'
(cd crates/site && SITE_VERSION=dev trunk build --release --public-url /asbuilt/dev/ --dist dist-snapshot)
cargo run -p asbuilt -- docs --no-render -o crates/site/dist-snapshot/architecture
SNAPSHOT_DIST=$PWD/crates/site/dist-snapshot SNAPSHOT_BASE=/asbuilt/dev/ SNAPSHOT_VERSION=dev \
  cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml \
  --run-ignored only -E 'test(/^site_(deployed_snapshot|architecture)/)'
```

A Claude Code session cannot run them: Chromium does not launch under
the macOS sandbox. Run them from a plain terminal, or let the pull
request's Pages job run them.
