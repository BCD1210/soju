"""Assemble native transition evidence without treating queued replies as success."""
import json
from pathlib import Path

from client import RUNTIME
from live_command_check import tables


def main():
    folder=RUNTIME/'3d-proof/m15'
    rows=[]
    for race in ('terran','zerg','protoss'):
        for phase in ('research-v4','gameplay-v4','supplemental','additional','rally'):
            file=folder/race/phase/'results.json'
            if file.exists():
                rows.extend(json.loads(file.read_text()))
    expected={
        'terran':['load','unload','rally-ground','rally-unit','stim','siege','unsiege','cloak','decloak','lift','land',
                  'cancel-train','cancel-build','cancel-research','cancel-upgrade','cancel-addon']+[f'addon-{i}' for i in (107,108,115,117,118,120)],
        'zerg':['load','unload','rally-ground','rally-unit','burrow','unburrow','cancel-building-morph','cancel-morph','cancel-build','cancel-research','cancel-upgrade']+
               [f'morph-{a}-{b}' for a,b in [(35,41),(35,37),(35,38),(35,43),(35,45),(35,46),(35,42),(35,39),(35,47),(38,103),(43,44),(43,62),(131,132),(132,133),(141,137),(143,144),(143,146)]],
        'protoss':['load','unload','rally-ground','rally-unit','cancel-train','cancel-build','cancel-research','cancel-upgrade'],
    }
    for operation,key in [('research','research'),('upgrade','upgrades')]:
        for producer,entries in tables()[key].items():
            race='terran' if producer<=125 else 'zerg' if producer<=149 else 'protoss'
            expected[race].extend(f'{operation}-{kind}' for kind,_ in entries)
    metadata=json.loads((folder/'native-metadata.json').read_text())
    for spell in metadata['spells']:
        caster=spell['casters'][0]
        race='terran' if caster in (1,2,9,12,107,34) else 'zerg' if caster in (45,46) else 'protoss'
        expected[race].append('spell-'+spell['action'])
    for race,commands in expected.items():
        for command in commands:
            if not any(row['race']==race and row['command']==command for row in rows):
                rows.append({'race':race,'command':command,'verified':False,'evidence':None,'error':'No completed native observation'})
    unique={}
    for row in rows:
        key=(row['race'],row['command'])
        if row['verified'] or key not in unique:unique[key]=row
    rows=list(unique.values())
    rows.sort(key=lambda row:(row['race'],row['command']))
    lines=['| Race | Command | Verified | Evidence |','|---|---|---|---|']
    for row in rows:
        path=Path(row['evidence']).relative_to(folder) if row.get('evidence') else None
        evidence=f'`runtime/3d-proof/m15/{path}`' if path else 'Not observed'
        lines.append(f"| {row['race']} | {row['command']} | {'yes' if row['verified'] else 'no'} | {evidence} |")
    (folder/'command-table.md').write_text('\n'.join(lines)+'\n')
    result={'commands':rows,'verified':sum(row['verified'] for row in rows),'total':len(rows),
            'fullyPlayableAccepted':False,'nativeButtonsetParityAccepted':False,
            'fullFootprintGroundingAccepted':False,'cliffStretchAccepted':False}
    (folder/'acceptance.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({key:value for key,value in result.items() if key!='commands'}))


if __name__=='__main__':
    main()
