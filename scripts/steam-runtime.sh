#!/usr/bin/env bash
# Existing users retain Wine Stable until a verified private runtime is ready.
SOJU_STEAM_BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
STEAM_WINE_ROOT="${SOJU_STEAM_WINE:-/Applications/Wine Stable.app/Contents/Resources/wine}"
if [ -z "${SOJU_STEAM_WINE:-}" ] && [ -f "$SOJU_STEAM_BASE/steam-runtime/.soju-runtime" ]; then
  STEAM_WINE_ROOT="$SOJU_STEAM_BASE/steam-runtime"
fi
STEAM_SUPPORT="$SOJU_STEAM_BASE/steam-support"
if [ -d "$STEAM_SUPPORT/prebuilt" ]; then STEAM_SUPPORT="$STEAM_SUPPORT/prebuilt"; fi
