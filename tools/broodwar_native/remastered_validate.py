"""Measure every sprite direction; reject incomplete or below-threshold candidates."""
import argparse
import hashlib
import json
import pathlib
import numpy as np
from PIL import Image, ImageDraw
from skimage.metrics import structural_similarity
from client import RUNTIME


def metrics(reference, rendered):
    a=reference[...,3]>.5;b=rendered[...,3]>.5
    union=a|b
    iou=float((a&b).sum()/max(1,union.sum()))
    # Compare premultiplied RGB on black, sampling the union foreground only.
    # Large empty canvases must not inflate SSIM.
    first=reference[...,:3]*reference[...,3,None]
    second=rendered[...,:3]*rendered[...,3,None]
    _,similarity=structural_similarity(first,second,channel_axis=2,data_range=1.,full=True)
    ssim=float(similarity.mean(-1)[union].mean()) if union.any() else 0.
    return {'silhouetteIoU':iou,'diffuseSSIM':ssim,'passed':iou>=.95 and ssim>=.85}


def validate(folder):
    config=json.loads((folder/'reconstruction.json').read_text())
    geometry=json.loads((folder/'geometry.json').read_text())
    rows=[]
    out=folder/'comparisons';out.mkdir(exist_ok=True)
    for d in range(17):
        ref=Image.open(folder/'references'/f'{d:02}-diffuse.png').convert('RGBA')
        ours=Image.open(folder/'renders'/f'{d:02}.png').convert('RGBA')
        if ref.size!=ours.size:raise ValueError('Render canvas mismatch; resizing is forbidden')
        row={'direction':d,'cameraAzimuthDegrees':config.get('camera',{}).get('yaws',[-i*11.25 for i in range(17)])[d],**metrics(np.asarray(ref)/255.,np.asarray(ours)/255.)}
        panel=Image.new('RGB',(ref.width*3,ref.height+40),(20,20,20))
        panel.paste(ref,(0,40),ref);panel.paste(ours,(ref.width,40),ours)
        ref_rgb=np.asarray(ref).astype(float)/255;ours_rgb=np.asarray(ours).astype(float)/255
        difference=np.abs(ref_rgb[...,:3]*ref_rgb[...,3,None]-ours_rgb[...,:3]*ours_rgb[...,3,None])
        panel.paste(Image.fromarray(np.uint8(difference*255)),(ref.width*2,40))
        ImageDraw.Draw(panel).text((8,8),f"Remastered | ours | difference    IoU {row['silhouetteIoU']:.4f}    SSIM {row['diffuseSSIM']:.4f}",fill='white')
        path=out/f'{d:02}.png';panel.save(path);row['comparison']=str(path);rows.append(row)
    fidelity=all(r['passed'] for r in rows)
    required=['idle','walk','attack','death']+(['work'] if config['type']==7 else [])
    missing=[a for a in required if a not in geometry['animations']]
    report={'type':config['type'],'modelSha256':hashlib.sha256((folder/'model.glb').read_bytes()).hexdigest(),'passed':fidelity and geometry['watertight'] and geometry['triangleBudgetPassed'] and not missing,
            'fidelityPassed':fidelity,'minimumIoU':min(r['silhouetteIoU'] for r in rows),
            'minimumSSIM':min(r['diffuseSSIM'] for r in rows),'passingDirections':sum(r['passed'] for r in rows),
            'thresholds':{'silhouetteIoU':.95,'diffuseSSIM':.85},'directions':rows,'geometry':geometry,
            'missingAnimations':missing,'renderProtocol':'Blender orthographic, Standard transfer, diffuse-only emission, transparent background. Prelit diffuse comparison; not a PBR lighting match.',
            'ssimProtocol':'7x7 RGB SSIM, data range 1, premultiplied black; mean over union foreground, no image registration or resizing.',
            'limitations':config['limitations']}
    (folder/'validation.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({k:report[k] for k in ('type','passed','minimumIoU','minimumSSIM','passingDirections','missingAnimations')}),flush=True)
    return report


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('folders',nargs='*',type=pathlib.Path)
    a=p.parse_args()
    for folder in a.folders or [RUNTIME/'models-remastered/0',RUNTIME/'models-remastered/7']:validate(folder)
