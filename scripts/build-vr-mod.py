#!/usr/bin/env python3
"""Build the Soju D2R-PCVR patch from local, pinned source checkouts.

No downloads, installation, or game launch. Requires cmake, ninja, and llvm-mingw.
See patches/d2r-pcvr/README.md for preparation and publication.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
BASE = '70ad0e665952f1bd829176ba461a85ebb974c3f0'
SDKS = {
    'D2RLoader-PluginSDK': '717f727a0ec52912d1558764345f8fa3453a2bd6',
    'OpenXR-SDK': '57af7fc61f9f2d492580cb28aab6d0ea59d8d417',
    'box3d': '8441b4a06d6d09dcfb0b0f704df4d847d1437b92',
}


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True, help='D2R-PCVR clone containing the pinned base')
    parser.add_argument('--deps', type=Path, required=True, help='directory of the three upstream SDK checkouts')
    parser.add_argument('--toolchain', type=Path, required=True, help='llvm-mingw-20260616-ucrt-macos-universal directory')
    parser.add_argument('--output', type=Path, default=ROOT / 'dist/vr-mod')
    args = parser.parse_args()
    source, deps, toolchain, out = (p.resolve() for p in (args.source, args.deps, args.toolchain, args.output))
    for name, commit in SDKS.items():
        actual = subprocess.check_output(['git', '-C', str(deps / name), 'rev-parse', 'HEAD'], text=True).strip()
        if actual != commit:
            parser.error(name + ' must be at ' + commit)
        run('git', '-C', str(deps / name), 'diff', '--exit-code', 'HEAD', '--')
    patch = ROOT / 'patches/d2r-pcvr/vr-character-create.patch'
    spec = json.loads((ROOT / 'resources/d2r-vr.json').read_text())['mod_patch']
    if hashlib.sha256(patch.read_bytes()).hexdigest() != spec['patch_sha256']:
        parser.error('source patch differs from the release pin')
    out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='soju-vr-mod-') as work:
        work = Path(work)
        src = work / 'source'
        src.mkdir()
        archive = work / 'base.tar'
        with archive.open('wb') as stream:
            run('git', '-C', str(source), 'archive', BASE, stdout=stream)
        # This archive comes from the exact local Git commit above.
        run('tar', '-xf', str(archive), '-C', str(src))
        run('patch', '-p1', '-i', str(patch), cwd=src)
        cmake_toolchain = work / 'llvm-mingw.cmake'
        prefix = toolchain / 'bin/x86_64-w64-mingw32-'
        cmake_toolchain.write_text(
            'set(CMAKE_SYSTEM_NAME Windows)\nset(CMAKE_SYSTEM_PROCESSOR AMD64)\n'
            + ''.join(f'set({key} "{prefix}{value}")\n' for key, value in {
                'CMAKE_C_COMPILER': 'clang', 'CMAKE_CXX_COMPILER': 'clang++',
                'CMAKE_RC_COMPILER': 'windres'}.items())
            + f'set(CMAKE_FIND_ROOT_PATH "{toolchain}/x86_64-w64-mingw32")\n'
            'set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)\n'
            'set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)\nset(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)\n'
            'set(CMAKE_C_FLAGS_INIT "-fms-extensions")\nset(CMAKE_CXX_FLAGS_INIT "-fms-extensions")\n'
            'set(CMAKE_SHARED_LINKER_FLAGS_INIT "-static -static-libgcc -static-libstdc++")\n'
            'set(CMAKE_EXE_LINKER_FLAGS_INIT "-static")\n')
        run('cmake', '-S', str(src / 'steamvr'), '-B', str(work / 'build'), '-G', 'Ninja',
            '-DCMAKE_BUILD_TYPE=Release', '-DCMAKE_TOOLCHAIN_FILE=' + str(cmake_toolchain),
            '-DD2RL_SDK=' + str(deps / 'D2RLoader-PluginSDK'),
            '-DOPENXR_SDK=' + str(deps / 'OpenXR-SDK'), '-DBOX3D_SDK=' + str(deps / 'box3d'))
        run('cmake', '--build', str(work / 'build'), '--target', 'd2rl-pcvr', '-j', '6')
        dll = out / 'd2rl-pcvr-soju.dll'
        shutil.copy2(work / 'build/d2rl-pcvr.dll', dll)
        with tarfile.open(out / 'd2r-pcvr-soju-source.tar.xz', 'w:xz') as tar:
            tar.add(src, arcname='d2r-pcvr-soju-source')
        shutil.copy2(src / 'LICENSE', out / 'LICENSE')
        shutil.copy2(patch, out / patch.name)
        for artifact in sorted(out.iterdir()):
            if artifact.is_file():
                print(artifact.name, artifact.stat().st_size, hashlib.sha256(artifact.read_bytes()).hexdigest())
        print('Builds may differ byte-for-byte. Pin the actual release DLL; publish source alongside it.')


if __name__ == '__main__':
    main()
