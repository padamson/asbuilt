#!/usr/bin/env bash
#
# Install Graphviz `dot` on a GitHub runner: the action's install, and the
# one this repo's own workflows use. Reads the runner's RUNNER_OS and
# RUNNER_TEMP, so it runs only there.
#
#   action/install-graphviz.sh ["asbuilt docs"]   # who needs it, for the error
#
# Ubuntu's apt ships Graphviz 2.43 (2020), whose spline router fails
# ("triangulation failed") on some layouts a current release draws. So on
# Ubuntu 24.04 (amd64) this installs Graphviz's own package, pinned by
# checksum. Graphviz builds that package per Ubuntu release and for amd64
# only, so any other Linux gets apt's, with a warning, and so does a
# 24.04 runner that cannot download it. brew on macOS, choco on Windows.
set -euo pipefail

GRAPHVIZ_VERSION=15.1.1
GRAPHVIZ_SHA256=02c70b9d42149f5e2ba0e3882643b22904720de50a0a3da84a30b8f32e8a1497
GRAPHVIZ_URL="https://gitlab.com/api/v4/projects/4207231/packages/generic/graphviz-releases/${GRAPHVIZ_VERSION}/ubuntu_24.04_graphviz-${GRAPHVIZ_VERSION}-cmake.deb"

who="${1:-asbuilt}"

case "$RUNNER_OS" in
  Linux)
    if ! command -v apt-get > /dev/null; then
      echo "::error title=asbuilt::$who needs Graphviz dot, which this action installs with apt-get, and this runner has none: install Graphviz before this step."
      exit 1
    fi
    sudo=""
    [ "$(id -u)" = 0 ] || sudo=sudo
    # shellcheck source=/dev/null
    . /etc/os-release
    arch=$(dpkg --print-architecture)
    deb="$RUNNER_TEMP/graphviz-$GRAPHVIZ_VERSION.deb"
    $sudo apt-get update -qq
    if [ "${ID:-}" = ubuntu ] && [ "${VERSION_ID:-}" = 24.04 ] && [ "$arch" = amd64 ] &&
      curl -fsSL --retry 3 -o "$deb" "$GRAPHVIZ_URL"; then
      # A download that fails falls back to apt; one that does not match
      # the pin fails the step.
      echo "$GRAPHVIZ_SHA256  $deb" | sha256sum -c -
      # The package declares no dependencies; these are the libraries its
      # own link against, Pango's included, so labels are measured with
      # real fonts rather than estimated.
      $sudo apt-get install -y -qq libexpat1 libltdl7 zlib1g \
        libcairo2 libglib2.0-0t64 libpango-1.0-0 libpangocairo-1.0-0 fonts-liberation \
        "$deb"
    else
      echo "::warning title=asbuilt::Installing Graphviz from apt on ${PRETTY_NAME:-this runner} ($arch), where it may be too old: Ubuntu's 2.43 fails to lay out some views that a current release draws. If a render fails, install Graphviz from https://graphviz.org/download/ before this step."
      $sudo apt-get install -y -qq graphviz
    fi
    ;;
  macOS) brew install graphviz ;;
  Windows)
    choco install graphviz -y --no-progress
    bin="C:\Program Files\Graphviz\bin"
    echo "$bin" >> "$GITHUB_PATH"
    PATH="$(cygpath -u "$bin"):$PATH"
    # Registers the layout plugins, which the installer may not.
    dot -c
    ;;
esac
dot -V
