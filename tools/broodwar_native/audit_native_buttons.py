# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
# Run: python tools/broodwar_native/audit_native_buttons.py <private-proof-folder>
"""Compare commander candidate slots/hotkeys with exported original button records.

This is a structural rejection check. Original predicates must be evaluated
before an exact enabled-state parity result can be reported.
"""
import json
from pathlib import Path
import struct
import sys


def audit(folder: Path) -> None:
    original = json.loads((folder / "original-buttonsets-250.json").read_text())
    commander = json.loads((folder / "current-card-audit.json").read_text())
    strings = (folder / "stat_txt.tbl").read_bytes()
    string_count = struct.unpack_from("<H", strings)[0]
    fixture_types: set[int] = set()
    for path in folder.rglob("*.json"):
        report = json.loads(path.read_text())
        if not isinstance(report, dict):
            continue
        for key in ("before", "after"):
            snapshot = report.get(key, {})
            fixture_types.update(unit["type"] for unit in snapshot.get("units", []))
    current = {row["type"]: row["commands"] for row in commander["rows"] if not row["alternateState"]}
    native = {row["type"]: row["buttons"] for row in original["sets"]}
    rows = []
    for kind in sorted(fixture_types):
        candidates = []
        for button in native[kind]:
            string_id = button["enabledString"]
            if not 0 < string_id <= string_count:
                continue
            offset = struct.unpack_from("<H", strings, string_id * 2)[0]
            hotkey = chr(strings[offset]).lower()
            candidates.append((button["position"], hotkey))
        differences = []
        for index, button in enumerate(current[kind]):
            position, hotkey = index % 9 + 1, button["hotkey"].lower()
            if (position, hotkey) not in candidates:
                differences.append({"page": index // 9, "position": position,
                                    "hotkey": hotkey, "title": button["title"]})
        rows.append({"type": kind, "originalCandidates": len(native[kind]),
                     "commanderCandidates": len(current[kind]), "layoutDifferences": differences,
                     "nativePredicatesEvaluated": False, "exactStateMismatches": None})
    output = {"passed": False, "fixtureTypes": len(rows), "exportedSets": len(original["sets"]),
              "candidateLayoutDifferences": sum(len(row["layoutDifferences"]) for row in rows),
              "exactStateMismatches": None, "rows": rows,
              "limitation": "Candidate comparison does not evaluate original UI predicates, submenus, selection context or enabled state."}
    (folder / "button-parity.json").write_text(json.dumps(output, indent=2) + "\n")


if __name__ == "__main__":
    audit(Path(sys.argv[1]).expanduser())
