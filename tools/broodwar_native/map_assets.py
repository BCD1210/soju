"""Prepare and atomically select terrain for the map loaded by the native game."""
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import time
import uuid
from client import RUNTIME, request

def local_map(native_path, runtime=RUNTIME):
    path=native_path.replace('\\','/')
    if path[:3].lower()=='z:/':result=pathlib.Path(path[2:])
    elif path[:3].lower()=='c:/':result=runtime/'bottle/drive_c'/path[3:]
    elif path.lower().startswith('maps/') and '..' not in pathlib.PurePosixPath(path).parts:
        result=runtime/'bottle/drive_c/Program Files (x86)/StarCraft'/path
    else:raise ValueError('Native map path is not a supported local Wine path')
    result=result.resolve()
    if result.suffix.lower() not in ('.scm','.scx') or not result.is_file():
        raise ValueError('Native map is missing or is not SCM/SCX')
    return result

def dependency(runtime,name):
    root=runtime/'deps'/name/'build'
    stem='storm' if name=='StormLib' else 'casc'
    candidates=[root/(stem+'.framework')/stem,root/('lib'+stem+'.dylib')]
    for p in candidates:
        if p.is_file():return p
    raise RuntimeError(f'Build the local {name} dependency before automatic map preparation')

def ensure_map(runtime=RUNTIME,state=None):
    state=state or request()
    native=state['map'].get('path','')
    source=local_map(native,runtime)
    digest=hashlib.sha256(source.read_bytes()).hexdigest()
    key=hashlib.sha256((native.lower()+':'+digest).encode()).hexdigest()[:24]
    active=runtime/'scene-assets'
    def matches(folder):
        try:
            m=json.loads((folder/'manifest.json').read_text())['map']
            return m.get('nativePath','').lower()==native.lower() and m.get('fileHash')==digest
        except (OSError,ValueError,KeyError):return False
    if matches(active):return {'changed':False,'map':str(source),'key':key}
    release=runtime/'terrain-releases'/key
    if not matches(release):
        stage=runtime/'terrain-releases'/('staging-'+uuid.uuid4().hex)
        stage.mkdir(parents=True)
        script=pathlib.Path(__file__).with_name('scene_assets.py')
        subprocess.run([sys.executable,str(script),'--casc-lib',str(dependency(runtime,'CascLib')),
            '--storm',str(dependency(runtime,'StormLib')),'--map',str(source),'--out',str(stage),'--terrain-only'],check=True)
        if not matches(stage):raise RuntimeError('Prepared terrain does not match the loaded map')
        # A concurrent process can have prepared the same immutable release.
        if not release.exists():os.replace(stage,release)
        elif not matches(release):raise RuntimeError('Conflicting terrain cache; current terrain retained')
    current=request()
    if current['map'].get('path','').lower()!=native.lower():raise RuntimeError('Game changed maps during preparation; retry')
    temporary=runtime/('scene-assets-'+uuid.uuid4().hex+'.tmp')
    temporary.symlink_to(release,target_is_directory=True)
    backup=None
    if active.exists() and not active.is_symlink():
        backup=runtime/'terrain-releases'/('legacy-'+str(time.time_ns()))
        os.replace(active,backup)
    try:os.replace(temporary,active)
    except OSError:
        if backup is not None:os.replace(backup,active)
        raise
    return {'changed':True,'map':str(source),'key':key}
