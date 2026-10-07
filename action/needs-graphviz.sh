#!/usr/bin/env bash
#
# Which asbuilt subcommand in the action's `command:` input lays views out
# with Graphviz `dot`: prints `render` or `docs`, or nothing.
#
#   action/needs-graphviz.sh "docs -o site"     # docs
#
# The subcommand is the first word that is neither an option nor the value
# of --config, so an argument spelled `docs` (`-o docs`) is not one;
# `docs --no-render` reuses rendered views and needs no Graphviz. No glob
# expansion: the words are read as the action passes them to asbuilt.
set -euf -o pipefail

subcommand="" no_render="" skip=""
for word in $1; do
  if [ -n "$skip" ]; then
    skip=""
    continue
  fi
  case "$word" in
    --config) skip=1 ;;
    --no-render) no_render=1 ;;
    -*) ;;
    *) [ -n "$subcommand" ] || subcommand="$word" ;;
  esac
done

case "$subcommand" in
  render) echo render ;;
  docs) [ -n "$no_render" ] || echo docs ;;
esac
