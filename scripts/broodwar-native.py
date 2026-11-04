#!/usr/bin/env python3
"""Original Brood War client bridge and experimental native HD scene."""
import argparse
import hashlib
import json
import os
import pathlib
import secrets
import shutil
import socket
import subprocess
import sys
import time
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools/broodwar_native"))
from client import RUNTIME, request

ENGINE = pathlib.Path.home() / ".battlenet-macos/cx26-engine"
PREFIX = RUNTIME / "bottle"
GAME = PREFIX / "drive_c/Program Files (x86)/StarCraft"
LOADER_URL = "https://raw.githubusercontent.com/BoneHorror/sezam/bd2368cb2b64bc427d7a7e0f15e58c119cb7ef8a/bin/samase64.exe"
LOADER_SHA = "8cc8d84d2c8c533ee048760b13cf49ed369502a821840f6e35ebe537afec668c"
EXE_SHA = "ce3ab05dc9a6aa35418947e5d95e19651ad6fbcd5e2c1856cd729c5b7c31281c"
DEFAULT_MAP = GAME / "Maps/campaign/(1)Enslavers01.scm"

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def windows(path):
    return "Z:" + str(path.resolve()).replace("/", "\\")

def environment():
    env = os.environ.copy()
    env.update(WINEPREFIX=str(PREFIX), WINEDEBUG="-all", WINEMSYNC="1",
        ROSETTA_ADVERTISE_AVX="1", WINE_SIMULATE_WRITECOPY="1",
        CX_ACTIVE_GRAPHICS_BACKEND="d3dmetal", CX_GRAPHICS_BACKEND="d3dmetal",
        CX_APPLEGPTK_LIBD3DSHARED_PATH=str(ENGINE / "lib/external/libd3dshared.dylib"),
        DYLD_FALLBACK_LIBRARY_PATH=f"{ENGINE}/lib:/usr/lib",
        SOJU_BW_LOG=windows(RUNTIME / "native.log"))
    return env

def check_prefix():
    if not PREFIX.is_dir() or PREFIX.is_symlink():
        raise RuntimeError("Run setup first; a dedicated real directory is required.")
    if PREFIX.resolve() != PREFIX:
        raise RuntimeError("Refusing a redirected test bottle.")
    exe = GAME / "x86_64/StarCraft.exe"
    if not exe.is_file() or digest(exe) != EXE_SHA:
        raise RuntimeError("This bridge is validated only for the original x64 1.23.10.13515 executable.")

def setup(source):
    source = source.expanduser().resolve()
    if not (source / "drive_c/Program Files (x86)/StarCraft/x86_64/StarCraft.exe").is_file():
        raise RuntimeError("Source must be your existing Wine bottle containing StarCraft.")
    RUNTIME.mkdir(parents=True, exist_ok=True)
    if not PREFIX.exists():
        subprocess.run(["cp", "-cR", str(source), str(PREFIX)], check=True)
    check_prefix()
    # Keep this bottle's game preferences and saves separate too.
    docs = PREFIX / "drive_c/users/crossover/Documents"
    if docs.is_symlink():
        old = docs.resolve() / "StarCraft"
        isolated = RUNTIME / "documents"
        isolated.mkdir(exist_ok=True)
        if old.is_dir() and not (isolated / "StarCraft").exists():
            shutil.copytree(old, isolated / "StarCraft", symlinks=True)
        docs.unlink()
        docs.symlink_to(isolated, target_is_directory=True)
    fonts = PREFIX / "drive_c/windows/Fonts"
    for name in ("tahoma.ttf", "tahomabd.ttf"):
        shutil.copy2(ENGINE / "share/wine/fonts" / name, fonts / name)
    # Samase 0.9.26 requires a font under its emoji filename even with no emoji.
    # Wine's open replacement is sufficient for the debug overlay's text.
    shutil.copy2(ENGINE / "share/wine/fonts/tahoma.ttf", fonts / "seguiemj.ttf")
    loader = RUNTIME / "samase64.exe"
    if not loader.is_file() or digest(loader) != LOADER_SHA:
        data = urllib.request.urlopen(LOADER_URL, timeout=60).read()
        binary = data
        if hashlib.sha256(binary).hexdigest() != LOADER_SHA:
            raise RuntimeError("Samase binary checksum mismatch")
        loader.write_bytes(binary)
    print("Original-client environment prepared. Build the native desktop/OpenXR scene separately.")

def build():
    check_prefix()
    env = os.environ.copy()
    env.update(CARGO_TARGET_DIR=str(RUNTIME / "build"),
        CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="/opt/homebrew/bin/x86_64-w64-mingw32-gcc")
    subprocess.run([str(pathlib.Path.home()/".cargo/bin/cargo"),"build","--release","--locked",
        "--target","x86_64-pc-windows-gnu"],cwd=ROOT/"tools/broodwar_native",env=env,check=True)
    mod = RUNTIME / "mod/samase"
    mod.mkdir(parents=True,exist_ok=True)
    shutil.copy2(RUNTIME/"build/x86_64-pc-windows-gnu/release/soju_bw.dll",mod/"soju_bw.dll.new")
    os.replace(mod/"soju_bw.dll.new",mod/"soju_bw.dll")
    (mod/"plugins64").write_text("samase/soju_bw.dll\n")

def bridge_listening():
    # Unlike a frame request, binding also detects a bridge paused in menus.
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as probe:
        try:
            probe.bind(("127.0.0.1",8795))
        except OSError:
            return True
    return False

def play(map_path):
    check_prefix()
    loader = RUNTIME/"samase64.exe"
    if not loader.is_file() or digest(loader) != LOADER_SHA:
        raise RuntimeError("Pinned Samase loader missing; run setup.")
    if not (RUNTIME/"mod/samase/soju_bw.dll").is_file():
        raise RuntimeError("Native module missing; run build.")
    if bridge_listening():
        raise RuntimeError("Bridge port already in use, possibly by a game in menus or paused; use status or stop.")
    if map_path is not None:
        map_path=map_path.expanduser().resolve()
        if not map_path.is_file():
            raise RuntimeError(f"Map does not exist: {map_path}")
    token_file=RUNTIME/"session-token"
    token_file.write_text(secrets.token_hex(32))
    token_file.chmod(0o600)
    env=environment()
    env["SOJU_BW_TOKEN"]=token_file.read_text()
    print("Starting original StarCraft 1.23.10.13515, Brood War single-player.")
    print("Native commander bridge enabled. Experimental scene: soju bw-native scene. Headset play is not verified.",flush=True)
    command=[str(ENGINE/"bin/wine"),str(loader),"--starcraft-exe",windows(GAME/"x86_64/StarCraft.exe")]
    if map_path is not None:
        command.extend(["--map",windows(map_path)])
    command.append(windows(RUNTIME/"mod"))
    subprocess.run(command,env=env,cwd=RUNTIME,check=True)

def launch_scene(xr=False, probe=False):
    exe=RUNTIME/("soju-bw-xr" if xr or probe else "soju-bw-scene")
    if not exe.is_file():
        raise RuntimeError("Run scene-build with the matching --xr option first.")
    env=os.environ.copy()
    if xr or probe:
        manifest=pathlib.Path.home()/".battlenet-macos/d2r-vr/runtime/runtime/oxrsys-runtime.json"
        if not manifest.is_file():
            raise RuntimeError("The tested Soju OpenXR runtime is missing.")
        env["XR_RUNTIME_JSON"]=str(manifest)
    if xr:
        ports=subprocess.run(["lsof","-nP","-iUDP:9946","-iTCP:9946"],capture_output=True,text=True)
        if ports.returncode==0:
            raise RuntimeError("Another VR session owns port 9946. End that session before starting Brood War OpenXR.")
    while True:
        if not probe:
            from map_assets import ensure_map
            waiting=False
            while True:
                try:
                    result=ensure_map();break
                except (TimeoutError,ConnectionRefusedError,ConnectionResetError):
                    if not waiting:print("Waiting for the original game to resume or load its map",flush=True)
                    waiting=True;time.sleep(.25)
                except RuntimeError as error:
                    if str(error) not in ("offline single-player game required","game unavailable","map unavailable","Game changed maps during preparation; retry"):raise
                    if not waiting:print("Waiting for the original game to load its map",flush=True)
                    waiting=True;time.sleep(.25)
            if result["changed"]:print("Prepared loaded map: "+result["map"],flush=True)
        completed=subprocess.run([str(exe)]+(["--xr-probe"] if probe else ["--xr"] if xr else []),env=env)
        if not probe and completed.returncode==78:continue
        completed.check_returncode();break

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest="action",required=True)
    p=sub.add_parser("setup")
    p.add_argument("--source",type=pathlib.Path,default=pathlib.Path.home()/".battlenet-macos/bottle")
    sub.add_parser("build")
    p=sub.add_parser("scene-build")
    p.add_argument("--xr",action="store_true")
    p=sub.add_parser("scene")
    p.add_argument("--xr",action="store_true")
    sub.add_parser("xr-probe")
    for name in ("assets-check", "assets-install"):
        p=sub.add_parser(name)
        p.add_argument("source",type=pathlib.Path,help="Folder containing GLBs and optional catalog.json")
        p.add_argument("--domain",choices=["units","effects"],default="units")
    p=sub.add_parser("assets-rollback")
    p.add_argument("--domain",choices=["units","effects"],default="units")
    sub.add_parser("model-check")
    p=sub.add_parser("play")
    mode=p.add_mutually_exclusive_group()
    mode.add_argument("--map",type=pathlib.Path,default=DEFAULT_MAP)
    mode.add_argument("--menu",action="store_true",help="Open the normal menu for Remastered account/HD settings.")
    sub.add_parser("state")
    sub.add_parser("status")
    sub.add_parser("stop")
    p=sub.add_parser("order")
    p.add_argument("json",help="Native command JSON: op, units, target or coordinates")
    a=parser.parse_args()
    if a.action=="setup": setup(a.source)
    elif a.action=="build": build()
    elif a.action=="scene-build":
        from build_scene import build as build_scene
        build_scene(a.xr)
    elif a.action=="scene": launch_scene(a.xr)
    elif a.action=="xr-probe": launch_scene(probe=True)
    elif a.action in ("assets-check","assets-install","assets-rollback"):
        from model_assets import main as assets_main
        return assets_main([a.action.removeprefix("assets-")]+([str(a.source)] if hasattr(a,"source") else [])+["--domain",a.domain])
    elif a.action=="model-check":
        subprocess.run([str(RUNTIME/"soju-bw-scene"),"--model-check"],check=True)
    elif a.action=="play": play(None if a.menu else a.map)
    elif a.action=="stop":
        check_prefix()
        subprocess.run([str(ENGINE/"bin/wineserver"),"-k"],env=environment(),check=True)
    elif a.action=="status":
        try:
            s=request()
            print(json.dumps({"connected":True,"client":s["source"],"frame":s["frame"],
                "broodWar":s["broodWar"],"xrRendererImplemented":True,"physicalHeadsetVerified":False},indent=2))
        except (OSError,RuntimeError) as e:
            listening=bridge_listening()
            print(json.dumps({"connected":False,"bridgePortInUse":listening,"xrRendererImplemented":True,"physicalHeadsetVerified":False,
                "hint":"Menus or a paused game do not answer frame requests." if listening else "No bridge listener detected.",
                "error":str(e)},indent=2))
            return 1
    else:
        payload={} if a.action=="state" else json.loads(a.json)
        print(json.dumps(request(**payload),indent=2))
    return 0

if __name__=="__main__":
    try: sys.exit(main())
    except (OSError,RuntimeError,subprocess.CalledProcessError,ValueError) as e:
        print(f"soju bw-native: {e}",file=sys.stderr)
        sys.exit(1)
