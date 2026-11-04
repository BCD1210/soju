#!/usr/bin/env bash
# Experimental: two verified 2048 homebrew NROs; not a general Switch emulator.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SWITCH_HOME="${SOJU_SWITCH_HOME:-$HOME/.local/share/soju/switch}"
PYTHON="$SWITCH_HOME/venv/bin/python"
case "${1:-help}" in
  setup)
    mkdir -p "$SWITCH_HOME"
    if [ ! -x "$PYTHON" ]; then python3 -m venv "$SWITCH_HOME/venv"; fi
    "$PYTHON" -m pip install 'unicorn==2.1.4' 'fonttools==4.68.0'
    "$PYTHON" "$ROOT/tools/switch/prepare_fonts.py" "$SWITCH_HOME"
    if command -v cc >/dev/null 2>&1; then
      cc -O3 -dynamiclib "$ROOT/tools/switch/frame_convert.c" -o "$SWITCH_HOME/libsoju-switch-frame.dylib"
    fi
    echo "Switch CPU dependency installed. SDL2 must also be installed (brew install sdl2)."
    ;;
  play)
    shift
    if [ "$#" -ne 1 ]; then echo "Usage: soju switch play /path/to/2048.nro" >&2; exit 64; fi
    if [ ! -x "$PYTHON" ]; then echo "Run 'soju switch setup' first." >&2; exit 1; fi
    if [ ! -f "$1" ]; then echo "NRO file not found: $1" >&2; exit 1; fi
    exec "$PYTHON" "$ROOT/tools/switch/trace_nro.py" "$1" --play --max-steps 10000000
    ;;
  help|-h|--help)
    echo "Experimental Switch homebrew: soju switch setup | soju switch play /path/to/2048.nro"
    echo "Two 2048 homebrew games have been checked. Account saves are experimental; SD files do not persist."
    echo "Controls: arrow keys, B to reset, Escape to close."
    ;;
  *) echo "Unknown switch command. Run 'soju switch help'." >&2; exit 64 ;;
esac
