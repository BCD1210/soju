"""Record per-type source-layer and team-paint checks for a frozen scene."""
import argparse
import json
from pathlib import Path
import numpy as np
from PIL import Image, ImageDraw
from client import RUNTIME
from remastered_reference import ReferenceRenderer, player_color


def audit(snapshot: Path, out: Path) -> None:
    capture=json.loads(snapshot.read_text());state=capture.get('state',capture)
    renderer=ReferenceRenderer();rows=[];panels=[];seen=set()
    for unit in state['units']:
        if unit['type'] in seen:continue
        seen.add(unit['type'])
        native=unit['image'];entry=renderer.catalog['images'][str(native['id'])]
        color=player_color(state['playerColors'],unit['owner'])
        before=renderer.sprite(native,(0,0,0));after=renderer.sprite(native,color)
        values=np.asarray(after);mask=values[...,3]>127
        old=np.asarray(before)[...,:3][mask];new=values[...,:3][mask]
        files=entry.get('relief',{}).get('files',{})
        f=entry['frames'][native['frame']]
        depth=None
        if 'ao_depth' in files:
            pixels=np.asarray(Image.open(renderer.folder/files['ao_depth']))
            samples=pixels[f['y']:f['y']+f['h'],f['x']:f['x']+f['w'],0]
            if native['flags']&2:samples=samples[:,::-1]
            depth=samples[mask]
        rows.append({'type':unit['type'],'image':native['id'],'frame':native['frame'],'playerColor':color,
            'layers':list(files),'meanRGBBefore':old.mean(axis=0).tolist(),'meanRGBAfter':new.mean(axis=0).tolist(),
            'nearBlackFractionBefore':float((old.max(axis=1)<12).mean()),'nearBlackFractionAfter':float((new.max(axis=1)<12).mean()),
            'depthRangeBytes':[int(depth.min()),int(depth.max())] if depth is not None else None,
            'mainHasNormalAndDepth':'normal' in files and 'ao_depth' in files,
            'nonBlackSource':bool(new.mean()>12)})
        panel=Image.new('RGB',(400,220),(30,30,30));draw=ImageDraw.Draw(panel)
        draw.text((5,5),f"Type {unit['type']} image {native['id']} | zero slot / player palette",fill='white')
        for index,im in enumerate((before,after)):
            im=im.copy();im.thumbnail((190,190));panel.paste(im,(index*200+(200-im.width)//2,25+(190-im.height)//2),im)
        panels.append(panel)
    sheet=Image.new('RGB',(1600,220*((len(panels)+3)//4)),(30,30,30))
    for i,panel in enumerate(panels):sheet.paste(panel,((i%4)*400,(i//4)*220))
    sheet.save(out/'type-paint-audit.png')
    report={'source':str(snapshot),'types':len(rows),'allMainLayersHaveNormalAndDepth':all(r['mainHasNormalAndDepth'] for r in rows),
        'allSourcesNonBlack':all(r['nonBlackSource'] for r in rows),'rows':rows,
        'limitations':'Source-pixel audit, not a screen-space illumination measurement. Original dark armor remains dark; zero-color initialization is rejected.'}
    (out/'visible-type-audit.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({k:v for k,v in report.items() if k!='rows'},indent=2))


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('snapshot',type=Path)
    p.add_argument('--out',type=Path,default=RUNTIME/'3d-proof/m9');a=p.parse_args();audit(a.snapshot,a.out)
