#!/usr/bin/env bash
#
# Fail when anything that names the release disagrees with it.
#
#   ./scripts/check-release-refs.sh 0.2.0
#
# The release workflow runs it with the tag's version before building or
# publishing anything; run it by hand before tagging. Between releases
# main legitimately disagrees (the docs name the coming release while the
# workspace still carries the last one), so it is not a commit hook.
#
# What it checks, each a place a release has to touch:
#   - workspace.package.version, and the version on the asbuilt-core and
#     asbuilt-rust entries under [workspace.dependencies];
#   - every `rev: vX.Y.Z` a consumer copies to reference the hook;
#   - a `## [X.Y.Z]` section in CHANGELOG.md (the release notes);
#   - the README's Status paragraph, which opens with the version;
#   - the site roadmap's MILESTONE, which must already name the next one.
set -euo pipefail

[ $# -eq 1 ] || { echo "usage: $(basename "$0") <version, e.g. 0.2.0>" >&2; exit 2; }
version="$1"
cd "$(dirname "$0")/.."

failures=0
fail() {
  echo "  $1" >&2
  failures=$((failures + 1))
}

# A version line inside one TOML table, read without a TOML parser: the
# first `version = "..."` after the table header, before the next header.
workspace_version=$(awk '/^\[workspace.package\]/ {t=1; next} /^\[/ {t=0} t && /^version *=/ {gsub(/.*= *"|".*/, ""); print; exit}' Cargo.toml)
[ "$workspace_version" = "$version" ] ||
  fail "Cargo.toml: workspace.package.version is \"$workspace_version\""

for crate in asbuilt-core asbuilt-rust; do
  dep=$(grep -E "^$crate = \{" Cargo.toml | sed -E 's/.*version = "([^"]*)".*/\1/')
  [ "$dep" = "$version" ] ||
    fail "Cargo.toml: [workspace.dependencies] $crate is \"$dep\""
done

for file in README.md skills/asbuilt/references/cli.md crates/site/snippets/pre_commit.yaml; do
  found=0
  while IFS= read -r line; do
    found=1
    rev=$(echo "${line#*:}" | sed -E 's/.*rev: *v?([^ ]*).*/\1/')
    [ "$rev" = "$version" ] || fail "$file:${line%%:*}: rev: v$rev"
  done < <(grep -n -E '^[[:space:]]*rev: *v[0-9]' "$file" || true)
  [ "$found" = 1 ] || fail "$file: no hook rev: line"
done

grep -q -F "## [$version]" CHANGELOG.md || fail "CHANGELOG.md: no ## [$version] section"

status=$(awk '/^## Status/ {t=1; next} t && NF {print; exit}' README.md)
case "$status" in
  "$version"[,.\ ]*) ;;
  *) fail "README.md: the Status paragraph opens \"${status:0:40}\"" ;;
esac

milestone=$(sed -n -E 's/^pub const MILESTONE: &str = "([^"]*)";/\1/p' crates/site/src/roadmap.rs)
[ "$milestone" != "$version" ] ||
  fail "crates/site/src/roadmap.rs: MILESTONE is still \"$milestone\"; move it to the next release"

if [ "$failures" -gt 0 ]; then
  echo "$failures reference(s) disagree with release $version" >&2
  exit 1
fi
echo "every release reference names $version"
