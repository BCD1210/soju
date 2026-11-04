"""Local-only experimental commander viewer; owns one OpenBW subprocess."""
import argparse
import hashlib
import http.server
import json
import os
import pathlib
import secrets
import subprocess
import threading
import urllib.parse

ROOT=pathlib.Path(__file__).resolve().parent
THREE_SHA256="ce1fa418de16a19495a9f72495580e3015d7745c296d3ce0485897f902ddedfb"

class Game:
    def __init__(self,home):
        self.lock=threading.Lock()
        self.meta=None
        self.state=None
        self.results=[]
        self.error=None
        self.proc=subprocess.Popen([str(home/"bridge"),str(home/"MatchPoint.scx")],cwd=home,
            env={**os.environ,"OPENBW_MPQ_PATH":str(home/"data")},
            stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        threading.Thread(target=self.read,daemon=True).start()
        threading.Thread(target=self.logs,daemon=True).start()
    def logs(self):
        for line in self.proc.stderr:
            print(line.rstrip(),flush=True)
    def read(self):
        for line in self.proc.stdout:
            try: msg=json.loads(line)
            except ValueError: continue
            with self.lock:
                kind=msg.get("kind")
                if kind=="map":self.meta=msg
                elif kind=="state":self.state=msg
                elif kind=="result":self.results=(self.results+[msg])[-64:]
        with self.lock:self.error="Game process ended"
    def snapshot(self):
        with self.lock:return {"map":self.meta,"state":self.state,"results":list(self.results),"error":self.error}
    def command(self,msg):
        if not isinstance(msg,dict):raise ValueError("command must be an object")
        payload=json.dumps(msg,allow_nan=False)
        if len(payload)>4096:raise ValueError("command too large")
        with self.lock:
            if self.proc.poll() is not None:raise ValueError("game is not running")
            self.proc.stdin.write(payload+"\n");self.proc.stdin.flush()
    def close(self):
        if self.proc.poll() is None:
            self.proc.stdin.close()
            try:self.proc.wait(timeout=3)
            except subprocess.TimeoutExpired:self.proc.terminate();self.proc.wait(timeout=3)

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--home",type=pathlib.Path,default=pathlib.Path.home()/".local/share/soju/broodwar-vr")
    ap.add_argument("--port",type=int,default=8794)
    args=ap.parse_args();home=args.home.resolve()
    for name in ("bridge","MatchPoint.scx","data/Patch_rt.mpq","data/StarDat.mpq","data/BrooDat.mpq","three.module.js"):
        if not (home/name).is_file():ap.error(f"Missing {home/name}")
    if hashlib.sha256((home/"three.module.js").read_bytes()).hexdigest()!=THREE_SHA256:
        ap.error("three.module.js does not match pinned Three.js 0.170.0")
    token=secrets.token_urlsafe(32);game=Game(home)
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self,*args):pass
        def allowed(self):
            return self.headers.get("Host") in (f"localhost:{args.port}",f"127.0.0.1:{args.port}")
        def send(self,code,data,mime="application/json"):
            self.send_response(code);self.send_header("Content-Type",mime)
            self.send_header("Content-Length",str(len(data)))
            self.send_header("Cache-Control","no-store");self.send_header("X-Content-Type-Options","nosniff")
            self.end_headers()
            try:self.wfile.write(data)
            except (BrokenPipeError,ConnectionResetError):pass
        def do_GET(self):
            if not self.allowed():return self.send(403,b'{}')
            path=urllib.parse.urlsplit(self.path).path
            if path=="/":return self.send(200,(ROOT/"viewer.html").read_text().replace("__TOKEN__",token).encode(),"text/html; charset=utf-8")
            if path=="/viewer.js":return self.send(200,(ROOT/"viewer.js").read_bytes(),"text/javascript")
            if path=="/three.module.js":return self.send(200,(home/"three.module.js").read_bytes(),"text/javascript")
            if path=="/state":return self.send(200,json.dumps(game.snapshot()).encode())
            self.send(404,b'{}')
        def do_POST(self):
            origin=self.headers.get("Origin")
            if (not self.allowed() or self.path!="/command" or self.headers.get("X-Soju-Token")!=token
                or (origin and origin!=f"http://{self.headers.get('Host')}")):
                return self.send(403,b'{}')
            try:
                size=int(self.headers.get("Content-Length","0"))
                if not 0<size<=4096:raise ValueError("invalid body size")
                game.command(json.loads(self.rfile.read(size)))
                self.send(202,b'{"queued":true}')
            except (ValueError,BrokenPipeError) as e:self.send(400,json.dumps({"error":str(e)}).encode())
    server=http.server.ThreadingHTTPServer(("127.0.0.1",args.port),Handler)
    print(f"Soju Brood War commander prototype: http://localhost:{args.port}",flush=True)
    try:server.serve_forever()
    except KeyboardInterrupt:pass
    finally:server.server_close();game.close()

if __name__=="__main__":main()
