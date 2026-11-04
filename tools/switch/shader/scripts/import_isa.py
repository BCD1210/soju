"""Extract declarative ISA facts from the pinned, explicitly supplied envytools file.
No network access. See SOURCES.md and LICENSE.envytools.
Usage: python3 scripts/import_isa.py .references/gm107.c
"""
import hashlib
import pathlib
import re
import sys

source = pathlib.Path(sys.argv[1]).read_text()
assert hashlib.sha256(source.encode()).hexdigest() == "97195e120336becb52f4aeabce28fb8ee0ead4c3f30efed10879e309eae3d3e6", "Unexpected ISA revision"
root = pathlib.Path(__file__).resolve().parents[1]
classes = 'ffma fadd fmul fmnmx fset fsetp mufu f2f f2i i2f i2i iadd iadd3 iscadd imad xmad iset isetp imnmx lop lop3 shl shr bfe bfi popc flo sel mov psetp p2r r2p ld st ldg stg lds sts ldl stl ldc ipa ald ast out tex texs tld tlds tld4 txq bra ssy sync pbk brk exit kil nop depbar bar'.split()
tables = dict(re.findall(r'static struct insn tab(\w+)\[\] = \{(.*?)\n\};', source, re.S))
rows = []
for line in tables['root'].splitlines():
    m = re.search(r'\{ (0x[0-9a-f]+)ull, (0x[0-9a-f]+)ull,.*?N\(\s*"([^"]+)"\)(.*)', line)
    if not m: continue
    value, mask, name, tail = m.groups()
    if name.endswith('32i'): name = name[:-3]
    if name not in classes or 'SYS_20' in tail: continue
    if name in ('ld', 'st'):
        space = {'AMEM': 'a', 'SMEM':'s', 'LMEM':'l', 'C36_08_S16_20':'c'}
        for token, prefix in space.items():
            if token in tail:
                name = {'a': {'ld':'ald','st':'ast'}, 's':{'ld':'lds','st':'sts'}, 'l':{'ld':'ldl','st':'stl'}, 'c':{'ld':'ldc'}}[prefix][name]
                break
    ops, flags, options, pending = [], [], [], []
    # ONV/ZNV modifiers in these texture rows are SM60-only, not GM20B.
    tail = re.sub(r'[OZ]N[V]\([^)]*\)', '', tail)
    tokens = re.findall(r'ON\([^)]*\)|ZN\([^)]*\)|T\([^)]*\)|N\([^)]*\)|\b(?:REG_\d+|PRED\d+|INV_PRED41|[USF]\d+_\d+|C\d+_\w+|AMEM28|AMEMIDX|AMEM|LMEM|SMEM|GMEM|NCGMEM_20|NCGMEM|BTARG)\b', tail)
    for token in tokens:
        if token.startswith(('ON(', 'ZN(')):
            bit, label = token[3:-1].split(',', 1)
            label = label.strip().strip('"')
            entry = (int(bit), label, token.startswith('ZN'))
            if label in ('neg','abs','inv','h1','not'): pending.append(entry)
            else: flags.append((*entry, -1))
        elif token.startswith('T('):
            key = token[2:-1]
            if key.startswith('pred'):
                pos = int(key[4:]); ops.append(f'p{pos}')
                flags.append((pos + 3, 'not', False, len(ops)-1))
            else:
                field_mask = 0
                for n in re.findall(r'\{ (?:0x[0-9a-f]+)ull, (0x[0-9a-f]+)ull', tables.get(key, '')): field_mask |= int(n,16)
                options.append((key,field_mask))
                if key == "f0a8_1": ops.extend(["BAR_ID", "BAR_COUNT"])
        elif token.startswith('N('):
            options.append((token[2:-1].strip().strip('"'),0))
        else:
            ops.append(token)
            flags.extend((*x, len(ops)-1) for x in pending); pending=[]
    assert not pending, (name,pending)
    fmt_flags = ','.join(f'({b},"{n}",{str(z).lower()},{i})' for b,n,z,i in flags)
    fmt_options = ','.join(f'("{n}",0x{m:016x})' for n,m in options)
    rows.append(f'    Encoding {{ opcode: Opcode::{name.upper()}, value: {value}, mask: {mask}, predicated: {str("T(pred)" in line).lower()}, fields: "{" ".join(ops)}", flags: &[{fmt_flags}], options: &[{fmt_options}] }},')
text = '// ISA facts from envytools gm107.c; see LICENSE.envytools and SOURCES.md.\n// Regenerate with scripts/import_isa.py, then cargo fmt.\nuse super::{Encoding, Opcode};\n#[rustfmt::skip]\npub(super) static ENCODINGS: &[Encoding] = &[\n' + '\n'.join(rows) + '\n];\n'
(root/'src/encodings.rs').write_text(text)
print(f'{len(rows)} encodings; source sha256 {hashlib.sha256(source.encode()).hexdigest()}')
