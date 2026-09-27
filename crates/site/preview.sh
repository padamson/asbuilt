#!/usr/bin/env bash
#
# Local preview with the receipts and the rendered context view, then hot
# reload. Run from anywhere.
#
# Prereqs (once): rustup target add wasm32-unknown-unknown; cargo install
# trunk; cargo install --locked playwright-rs --features cli &&
# playwright-rs install chromium; Node (for npx likec4) and Graphviz dot.
set -euo pipefail

cd "$(dirname "$0")"
ROOT="$(cd ../.. && pwd)"

# The landing page embeds this repo's own context view.
(cd "$ROOT" && cargo run -q -p asbuilt -- render && cp docs/architecture/views/context.svg crates/site/public/views/)
trunk build
# The dogfood gate, which also writes the receipts into public/receipts/.
(cd "$ROOT" && cargo nextest run --manifest-path crates/site-e2e/Cargo.toml --config-file .config/nextest.toml \
  --run-ignored only -E 'test(/^site_/) and not test(/^site_(deployed_snapshot|architecture)/)')
exec trunk serve --open
