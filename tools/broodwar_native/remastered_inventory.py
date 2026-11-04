"""Inventory all original unit image chains from owned CASC; publish honest coverage."""
import json
import pathlib
import struct
from client import RUNTIME
from scene_assets import Casc, anim
from map_assets import dependency

ROOT=pathlib.Path(__file__).resolve().parents[2]


def inventory():
    game=RUNTIME/'bottle/drive_c/Program Files (x86)/StarCraft'
    casc=Casc(dependency(RUNTIME,'CascLib'),game)
    names=json.loads((ROOT/'resources/broodwar-unit-types.json').read_text())['types']
    rows=[];cache={}
    try:
        images=casc.read('arr/images.dat')
        if len(images)!=37962:raise ValueError('Unsupported images.dat layout')
        units=casc.read('arr/units.dat');flingy=casc.read('arr/flingy.dat');sprites=casc.read('arr/sprites.dat');relations=casc.read('images.rel')
        for kind,name in names.items():
            number=int(kind);row={'type':number,'name':name,'status':'not-reconstructed'}
            try:
                sprite=struct.unpack_from('<H',flingy,units[number]*2)[0]
                image=struct.unpack_from('<H',sprites,sprite*2)[0]
                source=image;seen=set()
                while True:
                    if source in seen:raise ValueError('Cyclic HD relation')
                    seen.add(source);mode,target=struct.unpack_from('<II',relations,source*8)
                    if mode!=0x200:break
                    source=target
                row.update(image=image,sourceImage=source,directional=bool(images[999*4+image]),iscriptId=struct.unpack_from('<I',images,999*10+image*4)[0])
                if source not in cache:
                    data=None
                    for prefix in ('','HD2/'):
                        try:data=casc.read(f'{prefix}anim/main_{source:03}.anim');break
                        except FileNotFoundError:continue
                    if data is None:raise FileNotFoundError(f'ANIM {source}')
                    info,textures=anim(data)
                    cache[source]={'frameCount':len(info['frames']),'layers':list(textures),'canvas':[info['width'],info['height']]}
                row.update(cache[source])
                row['reason']='Directional/action layout not yet verified; pilot fidelity gate has not passed'
                if not row['directional'] or row['frameCount']<17:
                    row['status']='insufficient-multiview';row['reason']='Non-directional image or fewer than 17 frames; no measured 17-direction static pose'
            except (OSError,ValueError,struct.error) as error:
                row.update(status='source-unavailable',reason=str(error))
            validation=RUNTIME/'models-remastered'/kind/'validation.json'
            if validation.exists():
                v=json.loads(validation.read_text());row.update(status='passed' if v['passed'] else 'failed-gate',
                    minimumIoU=v['minimumIoU'],minimumSSIM=v['minimumSSIM'],passingDirections=v['passingDirections'],
                    reason='; '.join(['Missing animations: '+','.join(v['missingAnimations'])]+([] if v['fidelityPassed'] else ['Below per-direction fidelity thresholds'])),validation=str(validation))
            rows.append(row)
    finally:casc.close()
    report={'typeCount':len(rows),'passedTypes':sum(r['status']=='passed' for r in rows),'rows':rows,
            'policy':'SCEvo remains temporary fallback. Only fully validated reconstructions may replace it. No pilot has passed.',
            'limitations':['Main images only: turrets, overlays and separate death sprites need iscript/image-chain resolution.',
                           'Frame count is not proof of a 17-direction group.', 'Unit/graphics mapping uses classic DAT tables from the owned install.']}
    path=RUNTIME/'3d-proof/remastered-coverage.json';path.write_text(json.dumps(report,indent=2)+'\n')
    lines=['# Remastered reconstruction coverage','',
           'This is an experimental reconstruction inventory, not completed Remastered-quality art.',
           f"{report['passedTypes']} / {report['typeCount']} types pass the complete gate. SCEvo stays the temporary fallback.",'',
           'Private results: `runtime/models-remastered/{0,7}/validation.json`, `comparisons/{00..16}.png`.',
           'IoU/SSIM below are the minimum over all 17 directions; — means not measured.',
           'Full source inventory and limitations: `runtime/3d-proof/remastered-coverage.json`.','',
           '| Type | Name | Frames | Status | Min IoU | Min SSIM | Reason |',
           '| --- | --- | ---: | --- | ---: | ---: | --- |']
    for r in rows:
        iou=f"{r['minimumIoU']:.4f}" if 'minimumIoU' in r else '—'
        ssim=f"{r['minimumSSIM']:.4f}" if 'minimumSSIM' in r else '—'
        lines.append(f"| {r['type']} | {r['name']} | {r.get('frameCount','—')} | {r['status']} | {iou} | {ssim} | {r['reason'].replace('|','/')} |")
    (ROOT/'docs/BROODWAR-REMASTERED-COVERAGE.md').write_text('\n'.join(lines)+'\n')
    print(json.dumps({'rows':len(rows),'passed':report['passedTypes'],'uniqueImages':len(cache)}),flush=True)


if __name__=='__main__':inventory()
