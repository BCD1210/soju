"""Summarize pilot experiments and optionally build an isolated desktop preview."""
import argparse
import hashlib
import json
import pathlib
import shutil
import numpy as np
from PIL import Image
from client import RUNTIME
from remastered_validate import metrics
from remastered_rig_fit import project, raster

ROOT=pathlib.Path(__file__).resolve().parents[2]
SELECTED={0:'rigged-e32/fit',7:'rigged-dense/seeded'}


def audit_geometry(folder,camera):
    mesh=np.load(folder/'uv-mesh.npz');v,t=mesh['vertices'],mesh['triangles']
    original=np.load(folder.parent/'source.npz')['vertices']
    edges=np.unique(np.sort(np.concatenate([t[:,[0,1]],t[:,[1,2]],t[:,[2,0]]]),axis=1),axis=0)
    before=np.linalg.norm(original[edges[:,1]]-original[edges[:,0]],axis=1)
    after=np.linalg.norm(v[edges[:,1]]-v[edges[:,0]],axis=1)
    ratio=after[before>1e-6]/before[before>1e-6]
    a=np.cross(original[t[:,1]]-original[t[:,0]],original[t[:,2]]-original[t[:,0]])
    b=np.cross(v[t[:,1]]-v[t[:,0]],v[t[:,2]]-v[t[:,0]])
    deformation={'normalReversalsRelativeToSource':int(np.sum(np.sum(a*b,axis=1)<0)),
                 'edgeStretchMedian':float(np.median(ratio)),'edgeStretchP95':float(np.percentile(ratio,95)),
                 'edgeStretchMaximum':float(ratio.max()),'note':'Distortion diagnostic, not an exact self-intersection count.'}
    agreement=[]
    for d in range(17):
        _,face,_=raster(project(v,camera,d),t,camera['width'],camera['height'])
        ours=face>=0;blender=np.asarray(Image.open(folder/'renders'/f'{d:02}.png'))[...,3]>127
        agreement.append(float(np.count_nonzero(ours&blender)/max(1,np.count_nonzero(ours|blender))))
    raster_report={'minimumIoU':min(agreement),'directions':agreement,
                   'protocol':'Software raster vs Blender alpha; same camera, no registration or resizing'}
    (folder/'deformation-audit.json').write_text(json.dumps(deformation,indent=2)+'\n')
    (folder/'raster-agreement.json').write_text(json.dumps(raster_report,indent=2)+'\n')
    return deformation,raster_report


def collect():
    rows=[]
    for kind,selected in SELECTED.items():
        base=RUNTIME/'models-remastered'/str(kind)
        for candidate in sorted(base.glob('rigged*/*/validation.json')):
            folder=candidate.parent;validation=json.loads(candidate.read_text())
            camera=json.loads((folder/'reconstruction.json').read_text())['camera']
            deformation,raster_report=audit_geometry(folder,camera)
            material=json.loads((folder/'materials.json').read_text())
            depth=json.loads((folder/'depth-audit.json').read_text())
            refs=[];oracles=[]
            for d in range(17):
                ref=np.asarray(Image.open(folder/'references'/f'{d:02}-diffuse.png').convert('RGBA'))/255.
                ours=np.asarray(Image.open(folder/'renders'/f'{d:02}.png').convert('RGBA'))/255.
                mask=ref[...,3]>.5;ys,xs=np.nonzero(mask)
                refs.append({'direction':d,'foregroundPixels':int(mask.sum()),'bboxPixels':[int(np.ptp(xs)+1),int(np.ptp(ys)+1)]})
                # Diagnostic only: impossible screen-space reference paint on the rendered alpha.
                oracle=ref.copy();oracle[...,3]=ours[...,3]
                oracles.append(metrics(ref,oracle)['diffuseSSIM'])
            history=folder.parent/f'{folder.name}.json'
            fit=json.loads(history.read_text()) if history.exists() else None
            row={'type':kind,'experiment':str(folder.relative_to(base)),'selected':str(folder.relative_to(base))==selected,
                 'validation':str(candidate),'modelSha256':hashlib.sha256((folder/'model.glb').read_bytes()).hexdigest(),
                 'minimumIoU':validation['minimumIoU'],'minimumSSIM':validation['minimumSSIM'],
                 'passingDirections':validation['passingDirections'],'passed':validation['passed'],
                 'camera':camera,'deformation':deformation,'rasterAgreement':raster_report,'materials':material,'depth':depth,'sourceResolution':refs,
                 'screenSpaceTextureOracleMinimumSSIM':min(oracles),
                 'oracleWarning':'Diagnostic reference repaint of rendered alpha, not a realizable UV texture or acceptance result.',
                 'geometry':validation['geometry'],
                 'optimizer':None if fit is None else {k:fit[k] for k in ('iterations','bestIteration','best','maxDisplacementUnits')},
                 'last30MeanIoUGain':None if fit is None or len(fit['history'])<31 else fit['history'][-1]['meanIoU']-fit['history'][-31]['meanIoU']}
            rows.append(row)
    selected=[r for r in rows if r['selected']]
    if len(selected)!=2:raise ValueError('Both selected pilot validations must exist')
    report={'thresholds':{'silhouetteIoU':.95,'diffuseSSIM':.85},'experiments':rows,
            'selected':selected,'allPilotsPassed':all(r['passed'] for r in selected),
            'bulkProcessingAuthorizedByGate':all(r['passed'] for r in selected),
            'limitations':['Measured optimizer failure is not proof that the requested fidelity is mathematically impossible.',
                           'The fitted camera is conditional on a different source mesh; elevation and pose remain ambiguous.',
                           'Only 17 measured directions are fitted. Missing opposite-side and top/underside observations are not invented.',
                           'Diffuse is prelit; a view-independent atlas averages both correspondence errors and view-dependent source shading.',
                           'Death motion is partial named-bone retargeting; source debris and particles are omitted.']}
    target=RUNTIME/'3d-proof/rm/rigged-summary.json';target.write_text(json.dumps(report,indent=2)+'\n')
    return report


def preview(report):
    destination=RUNTIME/'models-remastered/rigged-preview';destination.mkdir(exist_ok=True)
    baseline=RUNTIME/'models-3d'
    catalog=json.loads((baseline/'catalog.json').read_text())
    for entry in catalog['models'].values():
        target=destination/entry['file'];target.parent.mkdir(parents=True,exist_ok=True)
        if not target.exists():shutil.copy2(baseline/entry['file'],target)
    for row in report['selected']:
        kind=row['type'];folder=pathlib.Path(row['validation']).parent
        target=destination/str(kind);target.mkdir(exist_ok=True)
        for name in ('model.glb','teamcolor.png'):shutil.copy2(folder/name,target/name)
        source=json.loads((folder.parent/'source.json').read_text())
        actions={'idle':[source.get('idleClip','Stand_full')],'walk':['Walk_full'],'attack':['Attack_full'],
                 'death':['Death_Retarget_full']}
        if kind==7:actions['work']=['Work_full']
        catalog['models'][str(kind)]={'name':f'Experimental fitted {kind}; failed fidelity gate',
            'file':f'{kind}/model.glb','scale':1,'targetHeight':1.748 if kind==0 else 1.95,
            'yawDegrees':180,'faceHeading':True,'footPivot':True,
            'teamMask':'texture','teamMaskTexture':f'{kind}/teamcolor.png','teamMaterials':[],
            'animations':actions,'requiredAnimations':list(actions),
            'fidelityPassed':row['passed'],'modelSha256':row['modelSha256']}
    catalog['acceptance']='isolated failed-candidate inspection only; active release unchanged'
    (destination/'catalog.json').write_text(json.dumps(catalog,indent=2)+'\n')
    return destination


def coverage(report):
    path=RUNTIME/'3d-proof/remastered-coverage.json';inventory=json.loads(path.read_text())
    for result in report['selected']:
        row=next(r for r in inventory['rows'] if r['type']==result['type'])
        row.update(status='passed' if result['passed'] else 'failed-gate',
                   minimumIoU=result['minimumIoU'],minimumSSIM=result['minimumSSIM'],
                   passingDirections=result['passingDirections'],validation=result['validation'],
                   reason='Rigged fit below per-direction thresholds; clips retained plus partial death retarget; live checks separate')
    inventory['passedTypes']=sum(r['status']=='passed' for r in inventory['rows'])
    inventory['riggedSummary']=str(RUNTIME/'3d-proof/rm/rigged-summary.json')
    path.write_text(json.dumps(inventory,indent=2)+'\n')
    doc=ROOT/'docs/BROODWAR-REMASTERED-COVERAGE.md';text=doc.read_text().splitlines()
    for result in report['selected']:
        kind=result['type'];r=next(r for r in inventory['rows'] if r['type']==kind)
        prefix=f'| {kind} |'
        text=[f"| {kind} | {r['name']} | {r['frameCount']} | {r['status']} | {r['minimumIoU']:.4f} | {r['minimumSSIM']:.4f} | {r['reason']} |" if line.startswith(prefix) else line for line in text]
    text=[('Private pilot results: `runtime/models-remastered/0/rigged-e32/fit/` and '
           '`runtime/models-remastered/7/rigged-dense/seeded/`; each has `validation.json` and `comparisons/{00..16}.png`.')
          if line.startswith('Private results:') else line for line in text]
    doc.write_text('\n'.join(text)+'\n')

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--preview',action='store_true');parser.add_argument('--coverage',action='store_true')
    args=parser.parse_args();report=collect()
    if args.preview:print(preview(report))
    if args.coverage:coverage(report)
    for row in report['selected']:print(row['type'],row['minimumIoU'],row['minimumSSIM'],row['passed'])
