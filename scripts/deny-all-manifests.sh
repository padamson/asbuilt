#!/usr/bin/env bash
#
# `cargo deny check bans licenses sources` over the root workspace and
# every crate excluded from it, with the one deny.toml at the repo root.
# The excluded site crates carry their own dependency graphs, which
# `cargo deny` at the root never sees; this is their license gate.
# Advisories are `scripts/check-advisories.sh`'s, which covers every
# tracked Cargo.lock and tells a finding from a broken tool, so the
# advisory monitor can file an issue from its report. Arguments replace
# the default checks, so `deny-all-manifests.sh licenses` runs that one
# alone.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ $# -eq 0 ]; then
  set -- bans licenses sources
fi

status=0
for manifest in Cargo.toml crates/site/Cargo.toml crates/site-e2e/Cargo.toml; do
  echo "==> cargo deny --manifest-path $manifest check $*"
  # `--config` because cargo deny otherwise prefers a deny.toml next to
  # the manifest.
  cargo deny --config "$PWD/deny.toml" --manifest-path "$manifest" check "$@" || status=1
done
exit "$status"
