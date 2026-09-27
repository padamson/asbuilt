#!/usr/bin/env bash
#
# The two landing-site crates sit outside the workspace (wasm target, a
# large tree), so each carries its own Cargo.lock:
#
#   crates/site       the Leptos app Trunk builds
#   crates/site-e2e   the playwright-rs test that gates the deploy
#
# Neither depends on a workspace crate by path today, so a root manifest
# change cannot stale them yet; the hook is keyed on the root manifest and
# lockfile anyway, so a future path dependency (site-e2e running asbuilt
# in-process, say) is covered from day one, and on each crate's own
# manifest, which is what does move.
#
# This checks rather than rewrites: refreshing a lockfile is a dependency
# resolution change, and it should be an explicit command you ran, not
# something that happened during a commit. CI runs the same crates with
# `--locked`, so a stale lockfile that slips past the hook fails there.
set -uo pipefail

status=0
for manifest in crates/site crates/site-e2e; do
  if ! cargo metadata --manifest-path "$manifest/Cargo.toml" --locked --format-version 1 \
       >/dev/null 2>&1; then
    echo "stale lockfile: $manifest/Cargo.lock is behind its manifest."
    echo "  refresh with: cargo metadata --manifest-path $manifest/Cargo.toml >/dev/null"
    echo "  then stage $manifest/Cargo.lock"
    status=1
  fi
done
exit $status
