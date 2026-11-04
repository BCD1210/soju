"""Build the native desktop scene or the experimental OpenXR Metal variant."""
import argparse
import os
import pathlib
import subprocess
from client import RUNTIME

HERE=pathlib.Path(__file__).resolve().parent
LAB=pathlib.Path.home()/"Projects/soju-vr-lab/oxrsys-winebridge/oxrsys-src/build/macos-x64"

def build(xr=False,sdk=None,loader=None):
    RUNTIME.mkdir(parents=True,exist_ok=True)
    output=RUNTIME/("soju-bw-xr" if xr else "soju-bw-scene")
    temporary=output.with_suffix(".new")
    developer=pathlib.Path(subprocess.check_output(["xcode-select","-p"],text=True).strip())
    mac_sdk=developer/"Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk"
    command=["xcrun","swiftc","-sdk",str(mac_sdk),"-O"]
    if xr:
        sdk=sdk or LAB/"_deps/openxr-src/include"
        loader=loader or LAB/"_deps/openxr-build/src/loader"
        if not (sdk/"openxr/openxr.h").is_file() or not (loader/"libopenxr_loader.dylib").is_file():
            raise RuntimeError("Supply the verified macOS x86_64 OpenXR SDK and loader paths")
        obj=RUNTIME/"OpenXRHost.o"
        subprocess.run(["xcrun","clang++","-std=c++17","-fobjc-arc","-arch","x86_64","-mmacosx-version-min=14.0",
            "-isysroot",str(mac_sdk),"-I",str(sdk),"-c",str(HERE/"OpenXRHost.mm"),"-o",str(obj)],check=True)
        command+=["-target","x86_64-apple-macosx14.0","-D","SOJU_OPENXR","-import-objc-header",
            str(HERE/"OpenXRHost.h"),str(obj),"-lc++","-L",str(loader),"-lopenxr_loader",
            "-Xlinker","-rpath","-Xlinker",str(loader)]
    framework=RUNTIME/"gltf-build/Build/Products/Release"
    if not (framework/"GLTFKit2.framework").is_dir():
        raise RuntimeError("Build the pinned GLTFKit2 framework before scene-build")
    main=RUNTIME/("xr-source" if xr else "scene-source")/"main.swift"
    main.parent.mkdir(exist_ok=True)
    main.write_text((HERE/"NativeScene.swift").read_text())
    sources=[]
    for name in ("Models3D.swift","ModelCatalog.swift","AssetChecks.swift","Terrain3D.swift","CombatEffects.swift","UnitPresentation.swift","EffectSprites.swift","CommanderSelection.swift","RuntimeChecks.swift"):
        staged=main.parent/name;staged.write_text((HERE/name).read_text());sources.append(str(staged))
    command += [str(main),*sources,"-F",str(framework),"-framework","GLTFKit2",
        "-Xlinker","-rpath","-Xlinker",str(framework),"-o",str(temporary),"-framework","Cocoa","-framework","SceneKit","-framework","Metal"]
    subprocess.run(command,check=True)
    (RUNTIME/"default-model-catalog.json").write_text((HERE.parents[1]/"resources/broodwar-models.json").read_text())
    os.replace(temporary,output)
    print(output)

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--xr",action="store_true")
    p.add_argument("--sdk",type=pathlib.Path);p.add_argument("--loader",type=pathlib.Path)
    a=p.parse_args();build(a.xr,a.sdk,a.loader)
