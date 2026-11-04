"""Install a checksum-verified portable Blender for this project's asset work."""
import hashlib,pathlib,subprocess,urllib.request,shutil
from client import RUNTIME
VERSION="4.5.15"
NAME=f"blender-{VERSION}-macos-arm64.dmg"
BASE="https://download.blender.org/release/Blender4.5/"
def main():
    root=RUNTIME/"blender";root.mkdir(exist_ok=True)
    image=root/NAME;checksum=urllib.request.urlopen(BASE+f"blender-{VERSION}.sha256",timeout=30).read().decode()
    expected=next(line.split()[0] for line in checksum.splitlines() if NAME in line)
    if not image.exists():
        with urllib.request.urlopen(BASE+NAME,timeout=60) as response,image.with_suffix(".partial").open("wb") as output:
            shutil.copyfileobj(response,output)
        image.with_suffix(".partial").rename(image)
    if hashlib.file_digest(image.open("rb"),"sha256").hexdigest()!=expected:raise RuntimeError("Blender checksum mismatch")
    (root/"release.sha256").write_text(checksum)
    mount=root/"mount";mount.mkdir(exist_ok=True)
    subprocess.run(["hdiutil","attach","-nobrowse","-readonly","-mountpoint",str(mount),str(image)],check=True)
    try:
        destination=root/"Blender.app"
        if not destination.exists():shutil.copytree(mount/"Blender.app",destination,symlinks=True)
    finally:subprocess.run(["hdiutil","detach",str(mount)],check=True)
    subprocess.run([str(root/"Blender.app/Contents/MacOS/Blender"),"--version"],check=True)
if __name__=="__main__":main()
