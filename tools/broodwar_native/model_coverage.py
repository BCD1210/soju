"""Build a private model catalog and an exhaustive units.dat coverage report."""
import json
import pathlib
import re
from client import RUNTIME
from prepare_models import UNIT_MODELS, MODELS, HERO_BASES, SKIN_TYPES, COMMIT, SOURCE, BASE, FORM_CLIPS

ROOT=pathlib.Path(__file__).resolve().parents[2]


def clip_aliases(clips):
    patterns={
        "idle":["Stand_full","Stand A_full","Stand 01_full","Cover_full"],
        "walk":["Walk_full","Run_full","Fly Walk_full"],
        "attack":["Attack_full","Attack 01_full","Attack A_full"],
        "work":["Work_full","Attack_full"],
        "carryIdle":["Stand Gather_full"],"carryWalk":["Walk Gather_full"],
        "death":["Death_full"],"construction":["Birth_full","Build A Stand_full"],
        "flyIdle":["Fly Stand_full","Stand_full"],"fly":["Fly Walk_full","Walk_full","Stand_full"],
        "burrow":["Burrow_full"],"unburrow":["Unburrow_full"],
        "burrowed":["Burrow Stand_full"],"cast":["Spell_full"],
        "siege":["Morph_full"],"lift":["Lift Off_full"],"land":["Land_full"]}
    return {action:[c for c in names if c in clips] for action,names in patterns.items() if any(c in clips for c in names)}


def write_catalog(folder,models,failures):
    names=json.loads((ROOT/'resources/broodwar-unit-types.json').read_text())['types']
    defaults=json.loads((ROOT/'resources/broodwar-models.json').read_text())['models']
    provenance=json.loads((RUNTIME/'models-source/provenance.json').read_text())
    source_records={r['local']:r for r in provenance['files']}
    catalog={};rows=[]
    for kind,name in names.items():
        number=int(kind);stem=UNIT_MODELS.get(number);model=models.get(stem)
        row={'type':number,'name':name,'status':'missing','reason':'No matching source model in pinned tree'}
        if model and not any(m.get('teamMask') or m.get('emission') for m in model['materials']):
            row['reason']='Source mesh converted, but colour textures are absent from pinned repository'
            model=None
        if model and model.get('untexturedTriangles',0)>model['triangles']*0.2:
            row['reason']='Source colour textures missing on more than 20% of retained triangles'
            model=None
        if model:
            building=106<=number<=175
            bounds=model['bounds'];height=bounds[1][1]-bounds[0][1]
            # Retain source proportions. These placements are provisional until art review.
            scale=1 if building or number>=176 else 2
            aliases={**clip_aliases(model['animations']),**FORM_CLIPS.get(number,{})}
            team_materials=[m['material'] for m in model['materials'] if m.get('teamMask')]
            definition={'name':name,'file':model['file'],'scale':scale,'footPivot':True,
                'yawDegrees':0 if building or number>=176 else 180,'faceHeading':not building and number<176,
                'flightHeight':1.5,'animations':aliases,'requiredAnimations':[],
                'teamMask':'alpha' if team_materials and number<176 else 'none','teamMaterials':team_materials if number<176 else [],
                'provenance':{'repository':SOURCE,'commit':COMMIT,'source':BASE+MODELS[stem],
                    'sourceSha256':source_records.get(stem+'.m3',{}).get('sha256'),
                    'convertedSha256':model['sha256']},
                'placementReview':'provisional source proportions; not final art approval'}
            definition={**defaults.get(kind,{}),**definition}
            if kind in defaults:
                definition.update({k:v for k,v in defaults[kind].items() if k in ('targetHeight','scale','requiredAnimations')})
                definition['animations']={**aliases,**defaults[kind].get('animations',{})}
            base=HERO_BASES.get(number,number)
            note=SKIN_TYPES.get(base,'')
            if number in HERO_BASES:note=('Shared geometry with type '+str(base)+'; '+note).strip('; ')
            if number in FORM_CLIPS:note='Authored Hatchery rig morph form: '+name
            definition['sourceVariant']=note or 'base model'
            variants={k[len(stem)+2:]:v['file'] for k,v in models.items() if k.startswith(stem+'--')}
            if variants:definition['availableVariants']=variants
            raw=(RUNTIME/'models-source'/(stem+'.m3')).read_bytes()
            refs={p.decode('ascii').replace('\\','/').split('/')[-1] for p in re.findall(rb'Assets[\\/][ -~]{1,240}?\.(?:dds|tga)',raw,re.I)}
            missing=sorted(refs.intersection(provenance.get('missingTextures',[])))
            definition['missingSourceTextures']=missing
            definition['omittedEffectLayers']=model.get('omittedEffectLayers',[])
            definition['untexturedTriangles']=model.get('untexturedTriangles',0)
            catalog[kind]=definition
            row.update(status='converted',file=model['file'],triangles=model['triangles'],clips=model['animations'],
                actions=sorted(aliases),sourceVariant=definition['sourceVariant'],height=height*definition['scale'],
                separateVariants=variants,reason='Renderer verification pending')
        elif stem and row['reason']=='No matching source model in pinned tree':
            row['reason']=failures.get(stem,'Conversion unavailable')
        if number in (4,6,18,24,26,31):row.update(status='subunit',reason='Turret geometry belongs to parent rig; not a separate placed model')
        rows.append(row)
    (folder/'catalog.json').write_text(json.dumps({'schemaVersion':1,'models':catalog},indent=2)+'\n')
    (folder/'provenance.json').write_text(json.dumps({'repository':SOURCE,'commit':COMMIT,'models':{k:v['provenance'] for k,v in catalog.items()}},indent=2)+'\n')
    report={'sourceCommit':COMMIT,'typeCount':228,'convertedTypes':len(catalog),
        'distinctModels':len({v['file'] for v in catalog.values()}),'rows':rows,'conversionFailures':failures,
        'limitations':['Separate death/birth rigs are archived, not automatically bound to base rigs.',
            'Alternate skins and shared hero geometry are explicitly labelled.','Placement and Remastered fidelity need art review.']}
    proof=RUNTIME/'3d-proof';proof.mkdir(exist_ok=True)
    (proof/'coverage.json').write_text(json.dumps(report,indent=2)+'\n')
    print('Coverage:',len(catalog),'types,',report['distinctModels'],'distinct models',flush=True)


def finalize(folder, renderer_report, live_report=None, document=None):
    proof=RUNTIME/'3d-proof/coverage.json'
    report=json.loads(proof.read_text())
    catalog=json.loads((folder/'catalog.json').read_text())['models']
    checks=json.loads(renderer_report.read_text())
    verified={m['type'] for m in checks['models'] if m['passed']}
    expected=set(map(int,catalog))
    if not checks['passed'] or verified!=expected:
        raise ValueError(f'Renderer coverage mismatch: missing {sorted(expected-verified)}, extra {sorted(verified-expected)}')
    report['rendererVerifiedTypes']=len(verified)
    report['rendererReport']=str(renderer_report)
    report['missingTypes']=sum(r['status']=='missing' for r in report['rows'])
    report['subunitTypes']=sum(r['status']=='subunit' for r in report['rows'])
    for row in report['rows']:
        if row['type'] in verified:
            row['reason']='Renderer rig/animation checks and four-view renders passed; art remains provisional'
            definition=catalog[str(row['type'])]
            row['missingSourceTextures']=definition['missingSourceTextures']
            row['omittedEffectLayers']=definition['omittedEffectLayers']
    if live_report:
        live=json.loads(live_report.read_text())
        seen={r['type'] for r in live['rendered']}
        report['liveRenderedTypes']=len(seen & expected)
        report['notObservedLive']=sorted(expected-seen)
        report['liveReport']=str(live_report)
    proof.write_text(json.dumps(report,indent=2)+'\n')
    if document:
        lines=['# Brood War model coverage','',f"Pinned source: `{COMMIT}`.",'',
            f"{len(expected)}/228 units.dat IDs use {report['distinctModels']} distinct converted models. "
            f"{report['missingTypes']} IDs are missing; {report['subunitTypes']} are parent-rig turret subunits.",
            f"{len(verified)} types passed native renderer checks and four-view exports.",'',
            'Private artifacts: `runtime/models-m2/{manifest,catalog,provenance}.json`, '
            '`runtime/3d-proof/coverage.json`, `runtime/3d-proof/m2/verified-models/`.', '',
            'Converted does not mean final art approval. Hero geometry aliases and alternate skins are labelled. '
            'Missing body textures exclude a model when more than 20% of retained triangles lack diffuse colour. '
            'Unsupported transparent effect layers without a colour/mask texture are omitted and recorded; '
            'these still need authored effects. Missing normals/specular/emission are listed in the private catalog. '
            'Separate birth/death rigs are converted and archived but are not bound automatically to base rigs.', '']
        if live_report:
            lines += [f"Live original-engine renders: {report['liveRenderedTypes']}/{len(expected)} types. "
                      f"Not observed live: {report['notObservedLive']}. Private evidence: `runtime/3d-proof/m2/live/`.",'']
        lines += ['| ID | Unit | Status | Source variant / limitation |','| --- | --- | --- | --- |']
        for row in report['rows']:
            detail=row.get('sourceVariant',row['reason'])
            lines.append(f"| {row['type']} | {row['name']} | {row['status']} | {detail.replace('|','/')} |")
        document.write_text('\n'.join(lines)+'\n')
    return report


if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--models-dir',type=pathlib.Path,required=True)
    parser.add_argument('--renderer-report',type=pathlib.Path,required=True)
    parser.add_argument('--live-report',type=pathlib.Path)
    parser.add_argument('--doc',type=pathlib.Path)
    args=parser.parse_args()
    result=finalize(args.models_dir,args.renderer_report,args.live_report,args.doc)
    print(json.dumps({k:result[k] for k in ('convertedTypes','distinctModels','rendererVerifiedTypes','missingTypes','subunitTypes')}))
