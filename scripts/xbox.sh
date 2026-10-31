#!/usr/bin/env bash
# Xbox PC / Microsoft Store through the Xodus backend: sign-in and downloads.
# Launching Xbox PC games is not available yet; nothing here starts Wine.
#   xbox.sh install | login | logout [--device] | status | download <ProductId> | kill
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
    echo "Xbox PC is ready. Sign in from Soju > Accounts (or: soju xbox-login)."
    echo "Downloads go to $XBOX_HOME/drive_c/XboxGames. Launching Xbox PC games is not available yet." ;;
  login)
    need_backend
    echo "Opening the Microsoft sign-in page. Finish signing in there, then this window closes."
    "$XODUS_CLI" login
    "$XODUS_CLI" account --json | python3 -c 'import json,sys
v=json.load(sys.stdin)
sys.exit(0 if v.get("stored") else 1)' || { echo "Sign-in was not completed." >&2; exit 1; }
    echo "Signed in." ;;
  logout)
    need_backend
    if [ "${2:-}" = "--device" ]; then "$XODUS_CLI" logout --device; else "$XODUS_CLI" logout; fi
    rm -f "$BASE/account-libraries/xbox.json"
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
    pkill -f "$XODUS_CLI streaming" 2>/dev/null && echo "Download stopped. Download again to resume." || echo "No Xbox download is running." ;;
  *) echo "Usage: xbox.sh install|login|logout [--device]|status|download <ProductId>|kill" >&2; exit 64 ;;
esac
