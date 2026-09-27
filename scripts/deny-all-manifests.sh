#!/usr/bin/env bash
#
# `cargo deny check` over the root workspace and every crate excluded from
# it, with the one deny.toml at the repo root (`--config` defaults to
# `<cwd>/deny.toml`, so run from here). The excluded site crates carry
# their own dependency graphs, which `cargo deny` at the root never sees;
# this is their advisory and license gate. Arguments pass through, so
# `deny-all-manifests.sh advisories` is what the advisory monitor runs.
set -euo pipefail
cd "$(dirname "$0")/.."

status=0
for manifest in Cargo.toml crates/site/Cargo.toml crates/site-e2e/Cargo.toml; do
  echo "==> cargo deny --manifest-path $manifest check $*"
  cargo deny --manifest-path "$manifest" check "$@" || status=1
done
exit "$status"
