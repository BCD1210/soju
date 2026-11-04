"""Authenticated localhost client for the original StarCraft process."""
import json
import pathlib
import socket
import time
from wire import receive

RUNTIME = pathlib.Path.home() / ".local/share/soju/broodwar-native"

def request(op="state", **kwargs):
    token = (RUNTIME / "session-token").read_text().strip()
    payload = {"op": op, "token": token, **kwargs}
    if op != "state":
        payload.setdefault("seq", time.time_ns() // 1000)
    data = json.dumps(payload).encode()
    if len(data) > 4096:
        raise ValueError("command exceeds native input size")
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.settimeout(5)
        sock.setsockopt(socket.SOL_SOCKET,socket.SO_RCVBUF,1024*1024)
        sock.connect(("127.0.0.1", 8795))
        sock.send(data)
        result = json.loads(receive(sock))
    if not result.get("ok"):
        raise RuntimeError(result.get("error", "native bridge error"))
    return result

if __name__ == "__main__":
    import sys
    args = json.loads(sys.argv[1]) if len(sys.argv) > 1 else {}
    print(json.dumps(request(**args), indent=2))
