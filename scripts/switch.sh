#!/usr/bin/env bash
# Experimental: two verified 2048 homebrew NROs; not a general Switch emulator.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SWITCH_HOME="${SOJU_SWITCH_HOME:-$HOME/.local/share/soju/switch}"
PYTHON="$SWITCH_HOME/venv/bin/python"
CORE="$SWITCH_HOME/soju-switch-core"
case "${1:-help}" in
  import|import-keys|list|remove)
    exec python3 "$ROOT/scripts/switch_library.py" "$@"
    ;;
  setup)
    mkdir -p "$SWITCH_HOME"
    if [ ! -x "$PYTHON" ]; then python3 -m venv "$SWITCH_HOME/venv"; fi
    "$PYTHON" -m pip install 'unicorn==2.1.4' 'fonttools==4.60.2; python_version < "3.10"' 'fonttools==4.68.0; python_version >= "3.10"'
    "$PYTHON" "$ROOT/tools/switch/prepare_fonts.py" "$SWITCH_HOME"
    if command -v cc >/dev/null 2>&1; then
      cc -O3 -dynamiclib "$ROOT/tools/switch/frame_convert.c" -o "$SWITCH_HOME/libsoju-switch-frame.dylib"
    fi
    if command -v cargo >/dev/null 2>&1; then
      cargo build --quiet --release --manifest-path "$ROOT/tools/switch/fs/Cargo.toml"
      install -m 755 "$ROOT/tools/switch/fs/target/release/soju-switch-fs" "$SWITCH_HOME/soju-switch-fs"
    else
      echo "Rust is not installed: game names for .nsp/.xci files will not be shown (brew install rust)."
    fi
    if command -v cargo >/dev/null 2>&1 && [ "$(uname -s)" = Darwin ] && [ "$(uname -m)" = arm64 ]; then
      cargo build --release --manifest-path "$ROOT/tools/switch/core/Cargo.toml" --features desktop
      # Sign the installed copy: replacing or modifying it afterwards invalidates the signature.
      install -m 755 "$ROOT/tools/switch/core/target/release/soju-switch-core" "$CORE"
      codesign --force --sign - --entitlements "$ROOT/tools/switch/core/hypervisor.entitlements" "$CORE"
      codesign --verify --strict "$CORE"
      echo "Rust Hypervisor runner built and signed. SOJU_SWITCH_RUNNER=python selects Unicorn."
    fi
    echo "Switch CPU dependency installed. SDL2 must also be installed (brew install sdl2)."
    ;;
  play)
    shift
    if [ "$#" -ne 1 ]; then echo "Usage: soju switch play /path/to/2048.nro" >&2; exit 64; fi
    case "$1" in *.[nN][rR][oO]) ;; *) echo "Only .nro homebrew can be played." >&2; exit 64 ;; esac
    if [ ! -f "$1" ]; then echo "NRO file not found: $1" >&2; exit 1; fi
    case "${SOJU_SWITCH_RUNNER:-auto}" in
      auto) if [ -x "$CORE" ]; then exec "$CORE" --play "$1"; fi ;;
      rust) if [ -x "$CORE" ]; then exec "$CORE" --play "$1"; fi
            echo "Rust runner missing. Run 'soju switch setup' first." >&2; exit 1 ;;
      python) ;;
      *) echo "SOJU_SWITCH_RUNNER must be auto, rust, or python." >&2; exit 64 ;;
    esac
    if [ ! -x "$PYTHON" ]; then echo "Run 'soju switch setup' first." >&2; exit 1; fi
    exec "$PYTHON" "$ROOT/tools/switch/trace_nro.py" "$1" --play --max-steps 10000000
    ;;
  help|-h|--help)
    echo "Experimental Switch homebrew: soju switch setup | import <file> | import-keys <prod.keys> [title.keys] | list [--json] | remove <id> | play <file.nro>"
    echo "Keys and games must be dumped from your own Switch. Soju never downloads them."
    echo "Two 2048 homebrew games have been checked. Account saves are experimental; the Rust runner also persists SD files on normal exit."
    echo "Controls: arrow keys, B to reset, Escape to close."
    ;;
  *) echo "Unknown switch command. Run 'soju switch help'." >&2; exit 64 ;;
esac
