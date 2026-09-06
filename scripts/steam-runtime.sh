#!/usr/bin/env bash
# Prefer the prepared renderer, then private upstream Wine, then a legacy install.
SOJU_STEAM_BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
STEAM_WINE_ROOT="${SOJU_STEAM_WINE:-/Applications/Wine Stable.app/Contents/Resources/wine}"
if [ -z "${SOJU_STEAM_WINE:-}" ]; then
  if [ -f "$SOJU_STEAM_BASE/steam-runtime/.soju-runtime" ] && [ -x "$SOJU_STEAM_BASE/steam-runtime/bin/wine" ]; then
    STEAM_WINE_ROOT="$SOJU_STEAM_BASE/steam-runtime"
  elif [ -f "$SOJU_STEAM_BASE/steam-wine/.soju-wine" ] && [ -x "$SOJU_STEAM_BASE/steam-wine/bin/wine" ]; then
    STEAM_WINE_ROOT="$SOJU_STEAM_BASE/steam-wine"
  fi
fi
STEAM_SUPPORT="$SOJU_STEAM_BASE/steam-support"
if [ -d "$STEAM_SUPPORT/prebuilt" ]; then STEAM_SUPPORT="$STEAM_SUPPORT/prebuilt"; fi
