"""Prepare pinned external 3D tools/art in the private runtime directory."""
import pathlib,shutil,subprocess,sys
from client import RUNTIME
HERE=pathlib.Path(__file__).resolve().parent
GLTF_COMMIT="be89e833aa85bde67bd27d4ecff3b7816c58ce56"
def run(*command,**kw):subprocess.run(list(map(str,command)),check=True,**kw)
def main():
    RUNTIME.mkdir(parents=True,exist_ok=True)
    python=shutil.which("python3.13") or shutil.which("python3")
    env=RUNTIME/"asset-venv"
    if not (env/"bin/python").exists():run(python,"-m","venv",env)
    run(env/"bin/python","-m","pip","install","Pillow==13.0.0")
    converter=RUNTIME/"converter/bin/m3-to-glb"
    if not converter.exists():run(pathlib.Path.home()/".cargo/bin/cargo","install","m3-to-glb","--version","0.2.0","--locked","--root",RUNTIME/"converter")
    vendor=RUNTIME/"vendor/GLTFKit2";vendor.parent.mkdir(exist_ok=True)
    if not vendor.exists():
        run("git","clone","https://github.com/warrenm/GLTFKit2.git",vendor)
        run("git","checkout","--detach",GLTF_COMMIT,cwd=vendor)
    commit=subprocess.check_output(["git","rev-parse","HEAD"],cwd=vendor,text=True).strip()
    if commit!=GLTF_COMMIT:raise RuntimeError("GLTFKit2 checkout differs from pinned revision")
    run("xcodebuild","-project",vendor/"GLTFKit2/GLTFKit2.xcodeproj","-scheme","GLTFKit2",
        "-configuration","Release","-destination","generic/platform=macOS","-derivedDataPath",RUNTIME/"gltf-build",
        "ARCHS=arm64 x86_64","ONLY_ACTIVE_ARCH=NO","CODE_SIGNING_ALLOWED=NO")
    run(sys.executable,HERE/"prepare_models.py")
    run(env/"bin/python",HERE/"convert_models.py")
    print("External model preparation complete. Creator source and provenance remain in the private runtime.")
if __name__=="__main__":main()
