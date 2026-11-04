"""Generate a fidelity-gated catalog or an explicitly isolated experimental preview."""
import argparse
import hashlib
import json
import pathlib
import shutil
from client import RUNTIME


def definition(kind):
    return {'name':f'Remastered candidate {kind}','file':f'{kind}/model.glb','scale':1,
            'targetHeight':1.7 if kind==0 else 1.95,'yawDegrees':180,'footPivot':True,'faceHeading':True,
            'teamMask':'texture','teamMaskTexture':f'{kind}/teamcolor.png','teamMaterials':[],
            'animations':{},'requiredAnimations':['idle','walk','attack','death']+(['work'] if kind==7 else [])}


def write_catalog(root, preview=False):
    models={};rejected={}
    for folder in sorted(root.iterdir()):
        if not folder.name.isdigit() or not (folder/'validation.json').is_file():continue
        report=json.loads((folder/'validation.json').read_text());kind=int(folder.name)
        model=folder/'model.glb'
        accepted=report['passed'] and model.is_file() and report.get('modelSha256')==hashlib.sha256(model.read_bytes()).hexdigest()
        if accepted or preview:
            models[str(kind)]=definition(kind)
            if preview:
                models[str(kind)]['requiredAnimations']=[]
                models[str(kind)]['name']+=' — experimental, failed acceptance'
        else:rejected[str(kind)]={'minimumIoU':report['minimumIoU'],'minimumSSIM':report['minimumSSIM'],'missingAnimations':report['missingAnimations']}
    if preview:
        destination=root/'preview';destination.mkdir(exist_ok=True)
        fallback=RUNTIME/'models-m2'
        baseline=json.loads((fallback/'catalog.json').read_text())['models']
        for entry in baseline.values():
            target=destination/entry['file'];target.parent.mkdir(parents=True,exist_ok=True)
            shutil.copy2(fallback/entry['file'],target)
        for kind in models:
            target=destination/kind;target.mkdir(exist_ok=True)
            for name in ('model.glb','teamcolor.png'):shutil.copy2(root/kind/name,target/name)
        models={**baseline,**models}
    else:destination=root
    catalog={'schemaVersion':1,'models':models,'rejected':rejected,
             'acceptance':'experimental-preview-only' if preview else 'all-direction fidelity plus geometry and actions',
             'priority':'validated Remastered entries override SCEvo; rejected entries retain fallback'}
    (destination/'catalog.json').write_text(json.dumps(catalog,indent=2)+'\n')
    print(destination)
    return catalog


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--preview',action='store_true');args=p.parse_args()
    write_catalog(RUNTIME/'models-remastered',args.preview)
