"""Reject structural differences between game-evaluated rows and displayed cards."""
import argparse
import json
from pathlib import Path
import struct


def compare(case: dict, strings: bytes) -> list:
    expected = []
    for row in case['native']['buttons']:
        if row['conditionState'] == 0:
            continue
        offset = struct.unpack_from('<H', strings, row['enabledString'] * 2)[0]
        key = strings[offset]
        label = ''.join(chr(c) for c in strings[offset+2:].split(b'\0', 1)[0] if c >= 32).strip()
        expected.append({'position': row['position'], 'enabled': row['conditionState'] == 1,
                         'recordIndex': row['recordIndex'], 'icon': row['icon'],
                         'enabledString': row['enabledString'], 'disabledString': row['disabledString'],
                         'hotkey': 'Esc' if key == 27 else chr(key).upper() if key >= 32 else '',
                         'title': label})
    actual = [{key: row.get(key) for key in expected[0]} for row in case['renderedCard']] if expected else case['renderedCard']
    failures = []
    if expected != actual:
        failures.append({'expected': expected, 'actual': actual})
    if len({r['position'] for r in expected}) != len(expected):
        failures.append({'duplicateSlots': expected})
    if case['native'].get('selectionRestored') is not True:
        failures.append({'selectionNotRestored': True})
    for row in case['renderedCard']:
        if str(row['payload'].get('op', '')).startswith('unsupported'):
            failures.append({'unsupportedAction': row})
    return failures


def expected_root(units: list, sets: dict, definitions: dict) -> int:
    active = units[0]['buttonSet']
    mixed = None
    for unit in units:
        selected = unit['buttonSet']
        if selected == active or sets[selected]['recordsAddress'] == sets[active]['recordsAddress']:
            continue
        if sets[selected]['linked'] == active:
            continue
        if sets[active]['linked'] == selected:
            active = selected
            mixed = selected
        else:
            mixed = 244
    if mixed is None:
        return active
    flags = [definitions[u['type']]['baseFlags'] for u in units]
    if all(f & 8 for f in flags):
        return 245
    if all(f & 0x200 for f in flags):
        return 246
    if any(f & 0x00100000 for f in flags) and all(not u['airborne'] and definitions[u['type']]['groupFlags'] & 1 for u in units):
        return 247
    return mixed


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--strings', type=Path, required=True)
    parser.add_argument('--fixtures', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--definitions', type=Path, required=True)
    parser.add_argument('--buttonsets', type=Path, required=True)
    parser.add_argument('reports', nargs='+', type=Path)
    args = parser.parse_args()
    strings = args.strings.read_bytes()
    definitions = {r['id']: r for r in json.loads(args.definitions.read_text())['types']}
    sets = {r['type']: r for r in json.loads(args.buttonsets.read_text())['sets']}
    expected_types = {r['type'] for r in json.loads(args.fixtures.read_text())['rows']}
    types, cases, differences = set(), [], []
    for path in args.reports:
        report = json.loads(path.read_text())
        for index, case in enumerate(report['cases']):
            types.update(u['type'] for u in case['units'])
            errors = compare(case, strings)
            root = expected_root(case['units'], sets, definitions)
            if root != case['native']['rootSet']:
                errors.append({'expectedSharedSet': root, 'actualSharedSet': case['native']['rootSet']})
            row = {'report': path.name, 'case': index, 'types': [u['type'] for u in case['units']],
                   'buttonSet': case['native']['buttonSet'], 'menu': case['menu'], 'mismatches': errors}
            cases.append(row)
            if errors:
                differences.append(row)
    missing = sorted(expected_types-types)
    output = {'passed': not differences and not missing, 'structuralMismatches': len(differences),
              'fixtureTypes': len(expected_types), 'coveredTypes': sorted(types), 'missingTypes': missing,
              'samples': len(cases), 'multiSelectionSamples': sum(len(c['types'])>1 for c in cases),
              'submenuSamples': sum(c['menu']!=228 for c in cases), 'differences': differences, 'cases': cases}
    args.output.write_text(json.dumps(output, indent=2)+'\n')
    print(json.dumps({k:v for k,v in output.items() if k not in ('cases','differences')}))
    if not output['passed']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
