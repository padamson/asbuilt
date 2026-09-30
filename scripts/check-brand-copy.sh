#!/usr/bin/env bash
#
# brand/mark.path is the mark's source. asbuilt-core embeds a copy
# (crates/asbuilt-core/src/mark.path) because a published crate cannot
# include a file outside its own directory. This fails when the two
# differ, naming the fix.
set -euo pipefail
cd "$(dirname "$0")/.."
if ! cmp -s brand/mark.path crates/asbuilt-core/src/mark.path; then
  echo "crates/asbuilt-core/src/mark.path differs from brand/mark.path."
  echo "  refresh with: cp brand/mark.path crates/asbuilt-core/src/mark.path"
  exit 1
fi
