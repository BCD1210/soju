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

# Installation paths call this even when the Steam client already exists.
# Selection alone remains read-only for play/doctor/uninstall.
soju_steam_runtime_ready() {
  [ -x "$STEAM_WINE_ROOT/bin/wine" ] && [ -x "$STEAM_WINE_ROOT/bin/wineserver" ] &&
    [ "$("$STEAM_WINE_ROOT/bin/wine" --version 2>/dev/null || true)" = wine-11.0 ]
}
soju_ensure_steam_runtime() {
  local repo_root="$1"
  soju_steam_runtime_ready && return 0
  if [ -n "${SOJU_STEAM_WINE:-}" ]; then
    echo "SOJU_STEAM_WINE must point to a working Wine 11.0 runtime." >&2
    return 1
  fi
  python3 "$repo_root/scripts/steam-session.py" idle "$STEAM_WINE_ROOT" || return 1
  python3 "$repo_root/scripts/fetch-steam-wine.py" || return 1
  # Do not select an incompatible prepared renderer again after downloading.
  STEAM_WINE_ROOT="$SOJU_STEAM_BASE/steam-wine"
  soju_steam_runtime_ready || { echo "Steam Wine runtime validation failed." >&2; return 1; }
}
