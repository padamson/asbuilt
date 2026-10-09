# asbuilt-site

The landing page for `asbuilt`, served at
[padamson.github.io/asbuilt](https://padamson.github.io/asbuilt/). A Leptos
CSR app (client-side, compiled to WebAssembly) built with
[Trunk](https://trunkrs.dev), styled with Tailwind CSS v4, deployed to
GitHub Pages as versioned snapshots (`/asbuilt/dev/` from main,
`/asbuilt/vX.Y.Z/` per release).

It is end-to-end tested by `playwright-rs` before every deploy. That test
lives in the sibling crate [`../site-e2e`](../site-e2e) and runs as a gate in
[`.github/workflows/pages.yml`](../../.github/workflows/pages.yml): the site
deploys only if the test can drive the built page and confirm it works as
advertised. The page also embeds this repo's own architecture: the
`context` view `asbuilt render` produces, and the documentation tree
`asbuilt docs` writes under `architecture/`.

This crate and `site-e2e` are **excluded from the root workspace** (this one
targets `wasm32`; both pull a large dependency tree), so workspace-wide
cargo commands and the publishable crates' supply chain are unaffected. Run
cargo commands here with `--manifest-path`. Each carries its own
`Cargo.lock`.

## Prerequisites

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk
```

Tailwind is handled by Trunk, which downloads the pinned v4 standalone
binary (see `Trunk.toml`), so there is no Node dependency for the build.
Rendering the context view needs Node (`npx likec4`) and Graphviz.

## Preview locally

```bash
cargo install --locked playwright-rs --features cli && playwright-rs install chromium   # once
crates/site/preview.sh
```

`preview.sh` renders the context view into `public/views/`, builds the
site, runs the dogfood test once to write the receipts into
`public/receipts/`, then `trunk serve`s with hot reload. Trunk re-copies
both directories into `dist/` on every rebuild. Plain `trunk serve --open`
from this directory is enough when only components changed.

## Build

```bash
cd crates/site
SITE_VERSION=dev trunk build --release        # dist/, public_url = "/"
```

## Run the dogfood test

From the repo root, after `trunk build`:

```bash
cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml \
  --run-ignored only -E 'test(/^site_/) and not test(/^site_(deployed_snapshot|architecture)/)'
```

The tests are `#[ignore]`d because they need a built site and Chromium; a
missing `dist/` fails them, it does not skip them.

## Format and lint

```bash
cargo fmt --manifest-path crates/site/Cargo.toml -- --check
cargo clippy --manifest-path crates/site/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings
cargo nextest run --manifest-path crates/site/Cargo.toml --config-file .config/nextest.toml
```

## Layout

```
index.html              Trunk entry: HTML shell, meta/OG tags, copy-dir of receipts and views
Trunk.toml              build config; pins Tailwind v4; public_url = "/"
input.css               Tailwind v4 entry + the rust/ink theme tokens
build.rs                highlights snippets/ with syntect at build time; SITE_VERSION;
                        LIKEC4_VERSION read from the CLI's pin; the context figure
                        lifted from public/architecture/views.html, links re-based
snippets/               code samples, one file per language (.rs/.sh/.toml/.c4/.diff/.yaml)
public/receipts/        written by the dogfood test (ignored, .gitkeep tracked)
public/views/           context.svg from `asbuilt render` (ignored, .gitkeep tracked)
deploy/update-manifest.sh   versions.json and the root redirect for the gh-pages tree
src/version.rs          SITE_VERSION, is_dev(), SITE_PREFIX and the two URL helpers
src/app.rs              composes the page sections
src/components/         Hero, Install, HowItWorks, Example, Features, DogfoodBanner,
                        Roadmap, VersionSwitcher, UnreleasedBadge, CodeBlock, CodeTabs,
                        FeatureCard, Footer, icons
src/roadmap.rs          the roadmap's types; the data is public/roadmap.json, fetched from /asbuilt/dev/ at runtime
src/snippets.rs         includes the build-time-generated highlighted HTML
```

## Syntax highlighting

Snippets are highlighted at build time by `build.rs` using syntect, a build
dependency only. To add one, drop a file in `snippets/` named by extension
(`foo.rs`) and render `crate::snippets::FOO_RS` with `CodeBlock` or
`CodeTabs`.

## Notes

- CSR because GitHub Pages cannot run a server. All view code lives in
  `#[component]`s, so a move to SSR would touch only the entry and build
  config.
- `SITE_PREFIX` in `src/version.rs` is the one runtime copy of the
  `/asbuilt` path; every other asset path is relative, which is what lets
  the same build serve at the root locally and under a snapshot path when
  deployed.
