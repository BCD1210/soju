#!/usr/bin/env bash
# Remove orphaned Wine services. SOJU_SWEEP_DRY=1 only reports candidates.
# Never infer ownership from command-line arguments or a failed lsof query.
set -u

# macOS Wine rewrites comm to the Windows executable path. A log reader
# with "services.exe.log" in its arguments has comm=/usr/bin/tail.
service_exe() {
  local exe name
  exe=$(ps -o comm= -p "$1" 2>/dev/null) || return 1
  [[ "$exe" =~ ^[A-Za-z]:[\\/] ]] || return 1
  name=${exe##*\\}; name=${name##*/}
  name=$(printf '%s' "$name" | tr '[:upper:]' '[:lower:]')
  case "$name" in
    services.exe|winedevice.exe|plugplay.exe|rpcss.exe|svchost.exe|explorer.exe|conhost.exe|tabtip.exe|control.exe|wineboot.exe|winedbg.exe|start.exe)
      printf '%s\n' "$exe" ;;
    *) return 1 ;;
  esac
}
server_pids() {
  { pgrep -x wineserver; pgrep -x wineserver64; } 2>/dev/null | sort -un
}
csv() { tr '\n' ',' | sed 's/,$//'; }

declare -a IDENTITIES
CAND=""
for p in $(pgrep -if '\.exe' 2>/dev/null || true); do
  service_exe "$p" >/dev/null || continue
  identity=$(ps -o lstart= -p "$p" 2>/dev/null) || continue
  [ -n "$identity" ] || continue
  IDENTITIES[$p]="$identity"
  CAND="$CAND $p"
done
[ -n "${CAND// /}" ] || exit 0

SERVERS=$(server_pids)
DIRS=""
if [ -n "$SERVERS" ]; then
  CWD=$(lsof -a -p "$(printf '%s\n' "$SERVERS" | csv)" -d cwd -Fpn 2>/dev/null) || {
    echo "soju-sweep: could not read all server directories; not touching anything" >&2; exit 1;
  }
  for p in $SERVERS; do
    dir=$(printf '%s\n' "$CWD" | awk -v target="$p" '
      /^p/ { pid=substr($0,2) }
      /^n/ && pid==target { print substr($0,2); exit }')
    [ -n "$dir" ] || { echo "soju-sweep: missing server directory; not touching anything" >&2; exit 1; }
    DIRS="${DIRS}${dir}|"
  done
fi

LISTING=$(lsof -a -p "$(printf '%s\n' $CAND | csv)" -Fpn 2>/dev/null) || {
  echo "soju-sweep: incomplete process listing; not touching anything" >&2; exit 1;
}
# Require positive Wine-prefix evidence; a missing PID record is unknown.
ORPHANS=$(printf '%s\n' "$LISTING" | awk -v dirs="$DIRS" '
  BEGIN { n=split(dirs,d,"|") }
  /^p/ { pid=substr($0,2) }
  /^n/ {
    if ($0 ~ /\/drive_c\/windows(\/|$)/) wine[pid]=1
    for (i=1;i<=n;i++)
      if (d[i]!="" && index($0,"n" d[i] "/")==1) attached[pid]=1
  }
  END { for (p in wine) if (!attached[p]) print p }')

# A server may have started while lsof ran. Reject that stale snapshot.
[ "$(server_pids)" = "$SERVERS" ] || {
  echo "soju-sweep: servers changed during inspection; retry later" >&2; exit 1;
}
for p in $CAND; do
  case " $(printf '%s\n' "$ORPHANS" | tr '\n' ' ') " in *" $p "*) ;; *) continue;; esac
  exe=$(service_exe "$p") || continue
  [ "$(ps -o lstart= -p "$p" 2>/dev/null)" = "${IDENTITIES[$p]}" ] || continue
  if [ "${SOJU_SWEEP_DRY:-0}" = 1 ]; then
    echo "soju-sweep (dry run): would remove $p $exe"
  else
    echo "soju-sweep: removing orphaned Wine service $p $exe"
    kill -9 "$p" 2>/dev/null || true
  fi
done
