"""Reproduce the Marine/SCV experiment and report failure without promoting assets."""
import argparse
import pathlib
import subprocess
import sys
from client import RUNTIME

HERE=pathlib.Path(__file__).resolve().parent


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--resolution',type=int,default=224)
    parser.add_argument('--calibrate',action='store_true')
    args=parser.parse_args()
    def python(script,*arguments):
        subprocess.run([sys.executable,str(HERE/script),*map(str,arguments)],check=True)
    python('remastered_layers.py')
    if args.calibrate:python('remastered_calibrate.py')
    python('remastered_reconstruct.py','--resolution',args.resolution)
    folders=[RUNTIME/'models-remastered'/str(k) for k in (0,7)]
    subprocess.run([str(RUNTIME/'blender/Blender.app/Contents/MacOS/Blender'),'-b','-t','4','--python-exit-code','1',
                    '--python',str(HERE/'remastered_blender.py'),'--',*map(str,folders)],check=True)
    python('remastered_validate.py',*folders)
    python('remastered_catalog.py')
    python('remastered_inventory.py')
    import json
    passed=all(json.loads((p/'validation.json').read_text())['passed'] for p in folders)
    print('Pilot gate passed' if passed else 'Pilot gate FAILED: no active assets replaced; bulk reconstruction remains unapproved')
    return 0 if passed else 1


if __name__=='__main__':raise SystemExit(main())
