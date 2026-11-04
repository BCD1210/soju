"""Make a labeled frozen eight-player rendering load from an owned native snapshot."""
import argparse
import copy
import hashlib
import json
from pathlib import Path


def make(source: Path, target: Path) -> None:
    data = json.loads(source.read_text())
    originals = data['state']['units']
    army = [u for u in originals if u['type'] in (0,7,5,8,11,37,38,42,64,65)]
    buildings = [u for u in originals if 106<=u['type']<=172]
    units = []
    for owner in range(8):
        cx,cy = (owner%4)*16+2,(owner//4)*32+4
        for i in range(216):
            template = army[i%len(army)] if i<200 else buildings[(i-200)%len(buildings)]
            unit = copy.deepcopy(template)
            x = int((cx+(i%20)*.6)*32)
            y = int((cy+(i//20)*.65)*32)
            dx,dy = x-unit['x'],y-unit['y']
            unit.update(id=f'stress-{owner}-{i}',owner=owner,x=x,y=y)
            for native in [unit['image'],*unit.get('visual',{}).get('images',[])]:
                native['mapX'] += dx
                native['mapY'] += dy
            units.append(unit)
    data['state']['units'] = units
    data['stressProvenance'] = {'source':str(source),'sha256':hashlib.sha256(source.read_bytes()).hexdigest(),
        'players':8,'armyPerPlayer':200,'buildingsPerPlayer':16,'units':len(units),
        'limitations':'Frozen native image chains replicated for rendering load; not live simulation or network load.'}
    target.write_text(json.dumps(data,separators=(',',':'))+'\n')
    print(json.dumps(data['stressProvenance'],indent=2))


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('source',type=Path);p.add_argument('target',type=Path)
    a=p.parse_args();make(a.source,a.target)
