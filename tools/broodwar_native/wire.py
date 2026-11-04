"""Bounded reassembly of one authenticated localhost response."""
import struct
HEADER = struct.Struct("<4sIHHI")
CHUNK = 8192 - HEADER.size
MAX_BYTES = 2 * 1024 * 1024

def receive(sock):
    first = sock.recv(65535)
    if not first.startswith(b"SBW1"):
        return first
    parts = {}
    identity = None
    packet = first
    while True:
        if len(packet) < HEADER.size:
            raise ValueError("short snapshot chunk")
        magic, response, index, count, total = HEADER.unpack_from(packet)
        if magic != b"SBW1" or not 0 < total <= MAX_BYTES or count != (total + CHUNK - 1) // CHUNK or index >= count:
            raise ValueError("invalid snapshot chunk bounds")
        current = (response, count, total)
        if identity is None:
            identity = current
        if current != identity or index in parts:
            raise ValueError("mixed or duplicate snapshot chunks")
        payload = packet[HEADER.size:]
        if len(payload) != min(CHUNK, total - index * CHUNK):
            raise ValueError("invalid snapshot chunk length")
        parts[index] = payload
        if len(parts) == count:
            return b"".join(parts[i] for i in range(count))
        packet = sock.recv(65535)