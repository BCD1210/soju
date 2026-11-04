"""Reproduce rig-fitting pilots without installing failed artwork or touching games."""
import argparse
import pathlib
import shutil
import subprocess
import sys
from client import RUNTIME

HERE=pathlib.Path(__file__).resolve().parent
BLENDER=RUNTIME/'blender/Blender.app/Contents/MacOS/Blender'


def python(name,*args):
    subprocess.run([sys.executable,str(HERE/name),*map(str,args)],check=True)


def blender(name,*args):
    subprocess.run([str(BLENDER),'-b','-t','4','--python-exit-code','1','--python',str(HERE/name),'--',*map(str,args)],check=True)


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--report-only',action='store_true');a=p.parse_args()
    if not a.report_only:
        for kind,pose in ((0,'Stand Ready_full'),(7,'Stand_full')):
            blender('remastered_rig_blender.py','extract','--types',kind,'--pose',pose)
        python('remastered_rig_fit.py','camera')
        for kind,iterations in ((0,160),(7,120)):
            python('remastered_rig_fit.py','fit','--types',kind,'--iterations',iterations)
        for label in ('baseline','fit'):blender('remastered_rig_render.py','--label',label)
        for kind,pose in ((0,'Stand Ready_full'),(7,'Stand_full')):
            blender('remastered_rig_blender.py','extract','--types',kind,'--pose',pose,'--subdivisions',2,'--variant','rigged-dense')
            base=RUNTIME/'models-remastered'/str(kind)
            shutil.copy2(base/'rigged/camera.json',base/'rigged-dense/camera.json')
            shutil.copytree(base/'rigged/references',base/'rigged-dense/references',dirs_exist_ok=True)
        blender('remastered_rig_death.py')
        python('remastered_rig_fit.py','fit','--variant','rigged-dense','--iterations',180,'--regularization',.5)
        blender('remastered_rig_render.py','--variant','rigged-dense')
        for kind in (0,7):
            base=RUNTIME/'models-remastered'/str(kind);dest=base/'rigged-e32';dest.mkdir(exist_ok=True)
            for name in ('source.npz','source.blend','source.json','death-retarget.json'):
                shutil.copy2(base/'rigged-dense'/name,dest/name)
            shutil.copytree(base/'rigged-dense/references',dest/'references',dirs_exist_ok=True)
        python('remastered_rig_fit.py','camera','--variant','rigged-e32','--elevation',32)
        python('remastered_rig_fit.py','fit','--variant','rigged-e32','--iterations',300,'--normal-weight',.5,'--regularization',1)
        blender('remastered_rig_render.py','--variant','rigged-e32')
        blender('remastered_rig_blender.py','seed','--types',7,'--variant','rigged-dense')
        python('remastered_rig_fit.py','fit','--types',7,'--variant','rigged-dense','--initial','seed.npz','--label','seeded',
               '--iterations',200,'--normal-weight',.5,'--regularization',2)
        blender('remastered_rig_render.py','--types',7,'--variant','rigged-dense','--label','seeded')
        for kind,variant in ((0,'rigged-e32'),(7,'rigged-dense')):
            python('remastered_rig_fit.py','fit','--types',kind,'--variant',variant,'--label','arap','--iterations',200,
                   '--normal-weight',.5,'--regularization',1,'--arap',1)
            blender('remastered_rig_render.py','--types',kind,'--variant',variant,'--label','arap')
    from remastered_validate import validate
    from remastered_rig_report import collect
    for kind in (0,7):
        for geometry in sorted((RUNTIME/'models-remastered'/str(kind)).glob('rigged*/*/geometry.json')):
            validate(geometry.parent)
    report=collect()
    print('Pilot gate passed' if report['allPilotsPassed'] else 'Pilot gate FAILED; no models installed; bulk processing not started')
    return 0 if report['allPilotsPassed'] else 1

if __name__=='__main__':raise SystemExit(main())
