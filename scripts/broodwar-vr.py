#!/usr/bin/env python3
"""Developer entry point for the experimental offline Brood War commander."""
import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import urllib.request

ROOT=pathlib.Path(__file__).resolve().parents[1]
TOOLS=ROOT/"tools/broodwar_vr"
HOME=pathlib.Path(os.environ.get("SOJU_BW_HOME",pathlib.Path.home()/".local/share/soju/broodwar-vr"))
SHA="ce1fa418de16a19495a9f72495580e3015d7745c296d3ce0485897f902ddedfb"

def main():
    p=argparse.ArgumentParser(description=__doc__)
    sub=p.add_subparsers(dest="action",required=True)
    setup=sub.add_parser("setup",help="Build from an existing matching OpenBW development environment")
    setup.add_argument("--stardust",type=pathlib.Path,required=True)
    setup.add_argument("--extractor",type=pathlib.Path,required=True,help="Existing sc_data CASC-to-MPQ tool")
    setup.add_argument("--game",type=pathlib.Path,required=True,help="Installed StarCraft directory")
    setup.add_argument("--map",type=pathlib.Path,required=True)
    play=sub.add_parser("play");play.add_argument("--port",type=int,default=8794)
    sub.add_parser("verify");sub.add_parser("status")
    args=p.parse_args()
    if args.action=="setup":
        for path in (args.stardust,args.extractor,args.game,args.map):
            if not path.exists():p.error(f"Missing {path}")
        HOME.mkdir(parents=True,exist_ok=True)
        subprocess.run([sys.executable,str(TOOLS/"build.py"),"--stardust",str(args.stardust),"--output",str(HOME/"bridge")],check=True)
        if not (HOME/"data/Patch_rt.mpq").is_file():
            with tempfile.TemporaryDirectory(dir=HOME,prefix="extract-") as tmp:
                target=pathlib.Path(tmp)
                subprocess.run([str(args.extractor.resolve()),"mpq",str(args.game.resolve()),str(target/"Patch_rt.mpq")],check=True)
                if (target/"Patch_rt.mpq").stat().st_size<1024*1024:raise RuntimeError("extracted archive is unexpectedly small")
                os.link(target/"Patch_rt.mpq",target/"StarDat.mpq");os.link(target/"Patch_rt.mpq",target/"BrooDat.mpq")
                target.rename(HOME/"data")
        shutil.copy2(args.map,HOME/"MatchPoint.scx")
        three=HOME/"three.module.js"
        if not three.is_file() or hashlib.sha256(three.read_bytes()).hexdigest()!=SHA:
            with urllib.request.urlopen("https://cdn.jsdelivr.net/npm/three@0.170.0/build/three.module.js",timeout=30) as response:data=response.read()
            if hashlib.sha256(data).hexdigest()!=SHA:raise RuntimeError("Three.js digest mismatch")
            three.write_bytes(data)
        (HOME/"setup.json").write_text(json.dumps({"stardust":str(args.stardust.resolve()),"game":str(args.game.resolve()),
            "map":str(args.map.resolve()),"backend":"OpenBW","three":"0.170.0"},indent=2)+"\n")
        print("Setup complete. Run: soju bw-vr play")
    elif args.action=="play":
        os.execv(sys.executable,[sys.executable,str(TOOLS/"server.py"),"--home",str(HOME),"--port",str(args.port)])
    elif args.action=="verify":
        subprocess.run([sys.executable,str(TOOLS/"verify.py"),"--bridge",str(HOME/"bridge"),"--data",str(HOME/"data"),
                        "--map",str(HOME/"MatchPoint.scx")],check=True)
    else:
        names=("bridge","MatchPoint.scx","data/Patch_rt.mpq","data/StarDat.mpq","data/BrooDat.mpq","three.module.js")
        print(json.dumps({"backend":"OpenBW","ready":all((HOME/n).is_file() for n in names),
              "headset_verified":False,"home":str(HOME),"missing":[n for n in names if not (HOME/n).is_file()]},indent=2))
if __name__=="__main__":main()
