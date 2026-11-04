"""Original card parity rejects displaced, omitted, and wrongly enabled cells."""
import copy
from pathlib import Path
import struct
import sys

TOOLS = Path(__file__).resolve().parents[1] / 'tools/broodwar_native'
sys.path.insert(0, str(TOOLS))
from button_strings import decode
from audit_evaluated_buttons import compare, expected_root
sys.path.pop(0)


def fixture():
    strings = struct.pack('<HH', 1, 4) + b'z\0\x03Z\x01ero\0'
    native = {'position': 7, 'conditionState': -1, 'recordIndex': 0,
              'icon': 1234, 'enabledString': 1, 'disabledString': 0}
    shown = {'position': 7, 'enabled': False, 'recordIndex': 0, 'icon': 1234,
             'enabledString': 1, 'disabledString': 0, 'hotkey': 'Z', 'title': 'Zero',
             'payload': {'op': 'train', 'type': 65000}}
    return strings, {'native': {'selectionRestored': True, 'buttons': [native]}, 'renderedCard': [shown]}


def test_button_string_internal_nul_and_color_codes():
    strings, _ = fixture()
    assert decode(strings) == {'1': {'hotkey': 'Z', 'label': 'Zero'}}
    for category in (1, 2, 3):
        typed = strings[:5] + bytes([category]) + strings[6:]
        assert decode(typed) == {'1': {'hotkey': 'Z', 'label': 'Zero'}}


def test_structural_gate_rejects_missing_displaced_and_enabled_disabled_button():
    strings, case = fixture()
    assert compare(case, strings) == []
    for change in ('missing', 'position', 'enabled', 'hotkey'):
        mutated = copy.deepcopy(case)
        if change == 'missing':
            mutated['renderedCard'] = []
        else:
            mutated['renderedCard'][0][change] = {'position': 1, 'enabled': True, 'hotkey': 'A'}[change]
        assert compare(mutated, strings)


def test_structural_gate_requires_selection_restoration():
    strings, case = fixture()
    case['native']['selectionRestored'] = False
    assert compare(case, strings)


def test_mixed_ground_burrowers_use_shared_burrow_card():
    sets = {41: {'recordsAddress': 1, 'linked': 65535}, 38: {'recordsAddress': 2, 'linked': 65535}}
    definitions = {41: {'baseFlags': 0x00100008, 'groupFlags': 1}, 38: {'baseFlags': 0x00100000, 'groupFlags': 1}}
    units = [{'type': k, 'buttonSet': k, 'airborne': False} for k in (41, 38)]
    assert expected_root(units, sets, definitions) == 247
    units[1]['airborne'] = True
    assert expected_root(units, sets, definitions) == 244
