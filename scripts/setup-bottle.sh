#!/usr/bin/env bash
# Fully clone an existing CrossOver "Battle.net Desktop App" bottle into a new one.
# Moving only the game files breaks CEF/Agent — the registries
# (system/user/userdef) must be cloned along with them.
# Uses APFS clones (cp -c), so almost no extra disk is used.
set -euo pipefail

CX_BOTTLE="${CX_BOTTLE:-$HOME/Library/Application Support/CrossOver/Bottles/Battle.net Desktop App}"
DEST="${DEST:?Set DEST to the new bottle path (e.g. ~/BattleNetBottle)}"

if [[ ! -d "$CX_BOTTLE/drive_c" ]]; then
  echo "CrossOver bottle not found: $CX_BOTTLE" >&2
  echo "Point CX_BOTTLE at the CrossOver bottle that has the game installed." >&2
  exit 1
fi

echo "==> Cloning the bottle: $CX_BOTTLE -> $DEST"
mkdir -p "$DEST"
cp -Rc "$CX_BOTTLE/drive_c" "$DEST/"
for f in system.reg user.reg userdef.reg; do
  [[ -f "$CX_BOTTLE/$f" ]] && cp -c "$CX_BOTTLE/$f" "$DEST/"
done
[[ -d "$CX_BOTTLE/dosdevices" ]] && cp -Rc "$CX_BOTTLE/dosdevices" "$DEST/"

echo "==> Done. Launch with:"
echo "    WINEPREFIX='$DEST' scripts/launch.sh"
