#!/usr/bin/env bash
# Build YTLiveStats as a self-contained single-file Windows exe that runs under Wine.
#
# Usage:
#   ./build-wine.sh              build into dist/wine-x64 next to the project
#   ./build-wine.sh --dry-run    print the three commands instead of running them
#
# Three steps, each one earning its place:
#   1. Linux SDK restores - the only step that downloads anything. NuGet's
#      signed-package verification cannot pass inside Wine (the prefix has no
#      Microsoft roots and Wine's certutil is a stub).
#   2. Wine SDK restores again from that same cache, which rewrites
#      project.assets.json with the Windows package paths step 3 needs.
#   3. Wine SDK publishes Release / win-x64 / self-contained / single file.
#
# Prerequisites (one-time):
#   * Linux .NET SDK at ~/.dotnet:
#       curl -fsSL https://dot.net/v1/dotnet-install.sh | bash -s -- --channel 8.0 --install-dir "$HOME/.dotnet"
#   * Wine prefix ~/.wine-dotnet (win64, Windows 10) with the Windows .NET SDKs
#     unzipped into C:\dotnet
#   * wine on PATH
# Override DOTNET_LINUX, DOTNET_WIN or WINEPREFIX in the environment if yours
# live somewhere else.
set -euo pipefail

APP='YTLiveStats'
PROJECT_REL='YTLiveStats.csproj'   # relative to this script
OUTPUT_REL='dist/wine-x64'     # relative to the project directory

DRY_RUN=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    -h|--help) sed -n '2,25p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "error: unknown argument '$arg' (try --help)" >&2; exit 2 ;;
  esac
done

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$HERE/$(dirname "$PROJECT_REL")"
PROJECT_FILE="$(basename "$PROJECT_REL")"

DOTNET_LINUX="${DOTNET_LINUX:-$HOME/.dotnet/dotnet}"
DOTNET_WIN="${DOTNET_WIN:-C:\\dotnet\\dotnet.exe}"
export WINEPREFIX="${WINEPREFIX:-$HOME/.wine-dotnet}"
export WINEDEBUG="${WINEDEBUG:--all}"
export DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1 DOTNET_SKIP_FIRST_TIME_EXPERIENCE=1

# The Linux SDK's package cache, as a Windows path, so the Wine SDK reuses it
# instead of downloading (and failing signature checks).
WIN_PKG="Z:$(printf '%s' "$HOME" | sed 's|/|\\|g')\\.nuget\\packages"

die() { echo "error: $*" >&2; exit 1; }

step() {
  if [ "$DRY_RUN" = 1 ]; then
    printf '%s\n' "$*"
  else
    printf '  %s\n' "$*"
    "$@"
  fi
}

if [ "$DRY_RUN" = 0 ]; then
  [ -x "$DOTNET_LINUX" ] || die "no Linux .NET SDK at $DOTNET_LINUX (set DOTNET_LINUX)"
  [ -f "$WINEPREFIX/drive_c/dotnet/dotnet.exe" ] || die "no Windows .NET SDK at C:\\dotnet inside $WINEPREFIX (set WINEPREFIX)"
  command -v wine >/dev/null || die "wine is not on PATH"
fi

echo "=== $APP: $PROJECT_REL -> $(dirname "$PROJECT_REL")/$OUTPUT_REL"
cd "$PROJECT_DIR"

# 1. Download packages with the Linux SDK (signature verification works here).
step env DOTNET_NUGET_SIGNATURE_VERIFICATION=false "$DOTNET_LINUX" restore "$PROJECT_FILE" \
  -r win-x64 -p:EnableWindowsTargeting=true

# 2. Restore from that cache with the Wine SDK.
step env NUGET_PACKAGES="$WIN_PKG" wine "$DOTNET_WIN" restore "$PROJECT_FILE" -r win-x64

# 3. Publish, offline.
step env NUGET_PACKAGES="$WIN_PKG" wine "$DOTNET_WIN" publish "$PROJECT_FILE" \
  -c Release -r win-x64 --self-contained true --no-restore \
  -p:PublishSingleFile=true -p:IncludeNativeLibrariesForSelfExtract=true \
  -p:DebugType=none -o "$OUTPUT_REL"

[ "$DRY_RUN" = 1 ] || echo "=== done: $PROJECT_DIR/$OUTPUT_REL"
