#!/usr/bin/env bash
# Launch YTLiveStats from its published build.
#
# Usage:
#   ./run-wine.sh [args...]
#
# Environment:
#   WINEPREFIX   prefix to run in (default: ~/.wine-apps)
#   EXE          executable to launch, relative to this repo (default: the
#                published build produced by ./build-wine.sh)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXE="${EXE:-dist/wine-x64/YTLiveStats.exe}"
export WINEPREFIX="${WINEPREFIX:-$HOME/.wine-apps}"

command -v wine >/dev/null || { echo "error: wine is not on PATH" >&2; exit 1; }
if [ ! -f "$HERE/$EXE" ]; then
  echo "error: $EXE does not exist yet - build it with ./build-wine.sh" >&2
  exit 1
fi

exec wine "$HERE/$EXE" "$@"
