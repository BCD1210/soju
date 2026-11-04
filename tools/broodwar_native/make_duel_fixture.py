"""Prepare a private Terran versus Zerg match from an owned two-start map."""
import argparse
import ctypes as C
import hashlib
import json
from pathlib import Path
import struct
import tempfile

from client import RUNTIME
from make_fixture import read_chk, sections, storm_library, unit
from map_assets import dependency


def scenario(original):
    data = dict(original)
    records = [data[b"UNIT"][i:i+36] for i in range(0, len(data[b"UNIT"]), 36)]
    starts = [struct.unpack_from("<HH", u, 4) for u in records
              if struct.unpack_from("<H", u, 8)[0] == 214]
    if len(starts) != 2:
        raise ValueError("A two-start source map is required")
    placed = [u for u in records if struct.unpack_from("<H", u, 8)[0] in (176, 177, 178, 188)]
    for player, (x, y) in enumerate(starts):
        kinds = [214, 106 if player == 0 else 131] + [7 if player == 0 else 41]*4
        if player == 1:
            kinds.append(42)
        for index, kind in enumerate(kinds):
            dx = (index-3)*24 if index >= 2 else 0
            dy = 80 if index >= 2 else 0
            placed.append(unit(40000+player*100+index, x+dx, y+dy, kind, player))
    triggers = []
    for player in range(2):
        trigger = bytearray(2400)
        trigger[15] = 22
        struct.pack_into("<6IH4B2x", trigger, 320, 0, 0, 0, 0, player, 50, 0, 26, 7, 4, 0)
        if player == 1:
            struct.pack_into("<6IH4B2x", trigger, 352, 1, 0, 0, 0, 0,
                             int.from_bytes(b"ZMCx", "little"), 0, 16, 0, 4, 0)
        trigger[2372+player] = 1
        triggers.append(trigger)
    width, height = struct.unpack("<HH", data[b"DIM "])
    locations = bytearray(data.get(b"MRGN", bytes(5100)))
    x, y = starts[1]
    struct.pack_into("<4IHH", locations, 0, max(0, x-256), max(0, y-256),
                     min(width*32, x+256), min(height*32, y+256), 0, 0)
    owners = bytes([6, 5]+[0]*10)
    replacements = {b"UNIT": b"".join(placed), b"OWNR": owners, b"IOWN": owners,
                    b"SIDE": bytes([1, 0]+[0]*10), b"FORC": bytes([0, 1]+[2]*6)+bytes(12),
                    b"TRIG": b"".join(triggers), b"MBRF": bytes(2400), b"MRGN": bytes(locations)}
    packed = b"".join(name+struct.pack("<I", len(replacements.get(name, value)))+
                      replacements.get(name, value) for name, value in original)
    return packed, starts


def build(source, destination):
    if destination.exists():
        raise ValueError("Output already exists")
    lib = storm_library(dependency(RUNTIME, "StormLib"))
    original = sections(read_chk(lib, source))
    packed, starts = scenario(original)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        chk = Path(tmp)/"scenario.chk"
        chk.write_bytes(packed)
        archive = C.c_void_p()
        if not lib.SFileCreateArchive(bytes(destination), 0, 16, C.byref(archive)):
            raise RuntimeError("Cannot create private match")
        try:
            if not lib.SFileAddFileEx(archive, bytes(chk), b"staredit\\scenario.chk", 0x200, 2, 2):
                raise RuntimeError("Cannot write private match")
        finally:
            lib.SFileCloseArchive(archive)
    if read_chk(lib, destination) != packed:
        raise RuntimeError("Private match verification failed")
    report = {"map": str(destination), "starts": starts, "source": str(source),
              "sourceSHA256": hashlib.sha256(source.read_bytes()).hexdigest(),
              "sha256": hashlib.sha256(destination.read_bytes()).hexdigest(),
              "players": ["Terran human", "Zerg computer"], "script": "ZMCx"}
    destination.with_suffix(".provenance.json").write_text(json.dumps(report, indent=2)+"\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    print(json.dumps(build(args.source.resolve(), args.destination.resolve()), indent=2))
