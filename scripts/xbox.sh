#!/usr/bin/env bash
# Xbox PC / Microsoft Store through the Xodus backend: sign-in, downloads and play.
#   xbox.sh install | prepare | login | logout [--device] | status | download <ProductId> | run <ProductId> | kill
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASE="${SOJU_BASE:-$HOME/.battlenet-macos}"
export SOJU_BASE="$BASE"
XODUS_CLI="${SOJU_XODUS_BIN:-$BASE/xodus/bin/xodus-cli}"
XBOX_HOME="$BASE/xbox-bottle"
# Tokens go to the macOS Keychain ("Xodus Service"); keep backend logs quiet so
# nothing account-related reaches Soju's log files.
export XODUS_LOG=error

need_backend() {
  if [ -n "${SOJU_XODUS_BIN:-}" ]; then
    [ -x "$XODUS_CLI" ] || { echo "SOJU_XODUS_BIN must point to an xodus-cli build with the Nexus commands." >&2; exit 1; }
    return
  fi
  python3 "$ROOT/scripts/fetch-xodus-backend.py" --check || {
    echo "Set up Xbox PC in Platforms first (or run: soju xbox-install)." >&2; exit 1; }
}

case "${1:-}" in
  install)
    if [ -z "${SOJU_XODUS_BIN:-}" ]; then python3 "$ROOT/scripts/fetch-xodus-backend.py"; fi
    need_backend
    mkdir -p "$XBOX_HOME/drive_c/XboxGames" "$XBOX_HOME/drive_c/ProgramData/Soju"
    marker="$XBOX_HOME/drive_c/ProgramData/Soju/xbox.json"
    if [ ! -f "$marker" ]; then
      printf '{"schema":1,"backend":"%s","created":"%s"}\n' \
        "$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["version"])' "$ROOT/resources/xodus-backend.json")" \
        "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$marker"
    fi
    # Games run on the CX engine with Apple's D3DMetal; without them only downloads work for now.
    if [ -x "${ENGINE:-$BASE/cx26-engine}/bin/wine" ] && [ -f "${ENGINE:-$BASE/cx26-engine}/lib/external/libd3dshared.dylib" ]; then
      python3 "$ROOT/scripts/xbox-prefix.py"
    else
      if [ -x "${ENGINE:-$BASE/cx26-engine}/bin/wine" ]; then
        echo "Playing Xbox PC games needs Apple's Game Porting Toolkit: add it in Platforms (or: soju gptk)."
      else
        echo "Playing Xbox PC games needs the CX engine: set up Xbox PC again in Platforms (or: soju install)."
      fi
    fi
    echo "Xbox PC is ready. Sign in from Soju > Accounts (or: soju xbox-login)."
    echo "Downloads go to $XBOX_HOME/drive_c/XboxGames." ;;
  prepare)
    exec python3 "$ROOT/scripts/xbox-prefix.py" ;;
  run)
    need_backend
    exec python3 "$ROOT/scripts/xbox-run.py" "${2:?Choose a game (Store product ID)}" ;;
  login)
    need_backend
    echo "Opening the Microsoft sign-in page. Finish signing in there, then this window closes."
    # Cancel in Soju sends TERM here: close the sign-in window too, so nothing is stored afterwards.
    # Some backend lines name the account; keep e-mail addresses out of the log.
    "$XODUS_CLI" login > >(sed -E 's/[^[:space:]]+@[^[:space:]]+/<account>/g') 2>&1 &
    login_pid=$!
    trap 'kill -TERM "$login_pid" 2>/dev/null; wait "$login_pid" 2>/dev/null; exit 143' TERM INT HUP
    wait "$login_pid"
    trap - TERM INT HUP
    "$XODUS_CLI" account --json | python3 -c 'import json,sys
v=json.load(sys.stdin)
sys.exit(0 if v.get("stored") else 1)' || { echo "Sign-in was not completed." >&2; exit 1; }
    echo "Signed in." ;;
  logout)
    # Signing out must work even when the backend is gone or its recipe changed:
    # any xodus-cli that still runs clears the Keychain entry, and Soju's own
    # copy of the library is removed either way.
    if [ -x "$XODUS_CLI" ] && "$XODUS_CLI" --version >/dev/null 2>&1; then
      if [ "${2:-}" = "--device" ]; then "$XODUS_CLI" logout --device; else "$XODUS_CLI" logout; fi
    else
      echo "The Xbox PC backend is not installed; removing Soju's copy of the library only."
    fi
    rm -f "$BASE/account-libraries/xbox.json" "$BASE"/library-artwork/xbox-*.jpg
    echo "Signed out. Downloaded games are kept." ;;
  status)
    need_backend
    # Only whether an account is stored; the account name is never printed.
    "$XODUS_CLI" account --json | python3 -c 'import json,sys
print("Signed in" if json.load(sys.stdin).get("stored") else "Not signed in")' ;;
  download)
    need_backend
    exec python3 "$ROOT/scripts/xbox-download.py" "${2:?Choose a game (Store product ID)}" ;;
  kill)
    # Downloads and games record their pid; only a live xbox-download.py or
    # xbox-run.py with that pid is signalled. SIGTERM lets the download keep its
    # checkpoint, and lets xbox-run.py stop the game and copy its saves back.
    stopped=0
    for f in "$BASE"/run/xbox-*.pid; do
      [ -f "$f" ] || continue
      p="$(cat "$f" 2>/dev/null)"
      case "$p" in ''|*[!0-9]*) continue ;; esac
      cmd="$(ps -p "$p" -o command= 2>/dev/null || true)"
      case "$cmd" in
        *xbox-download.py*) kill -TERM "$p" && echo "Download stopped. Download again to resume." && stopped=1 ;;
        *xbox-run.py*) kill -TERM "$p" && echo "Game stop requested." && stopped=1 ;;
      esac
    done
    [ "$stopped" = 1 ] || echo "No Xbox download or game is running." ;;
  *) echo "Usage: xbox.sh install|prepare|login|logout [--device]|status|download <ProductId>|run <ProductId>|kill" >&2; exit 64 ;;
esac
