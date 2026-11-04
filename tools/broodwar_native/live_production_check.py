# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Observe addon, construction cancellation, rally and repair on a private fixture."""
import json
import math
import time
from client import RUNTIME,request


def main():
    initial=request();assert 'Soju-Acceptance-M4-' in initial['map']['path']
    folder=RUNTIME/'3d-proof/m4/production';folder.mkdir(exist_ok=True,parents=True)
    results=[]
    def unit(kind):
        return next(u for u in request()['units'] if u['type']==kind and u['owner']==initial['player'])
    def observe(name,predicate,seconds=45):
        deadline=time.monotonic()+seconds
        while time.monotonic()<deadline:
            state=request()
            if predicate(state):
                (folder/(name+'.json')).write_text(json.dumps(state,indent=2)+'\n')
                results.append({'check':name,'frame':state['frame'],'passed':True})
                (folder/'results.json').write_text(json.dumps(results,indent=2)+'\n')
                print(json.dumps(results[-1]),flush=True);return state
            time.sleep(.1)
        (folder/(name+'-failed.json')).write_text(json.dumps(state,indent=2)+'\n')
        raise AssertionError('Native transition not observed: '+name)
    before=request();scv=unit(7)
    request('build',units=[scv['id']],type=109,tileX=28,tileY=8)
    state=observe('construction_started',lambda s:any(u['type']==109 and not u['completed'] for u in s['units']))
    building=next(u for u in state['units'] if u['type']==109 and not u['completed'])
    request('cancel',units=[building['id']],action='construction')
    observe('construction_cancelled',lambda s:not any(u['id']==building['id'] for u in s['units']) and s['minerals']>state['minerals'])
    factory=unit(113);before=request()
    request('addon',units=[factory['id']],type=120,tileX=50,tileY=16)
    state=observe('addon_started',lambda s:any(u['type']==120 for u in s['units']))
    addon=next(u for u in state['units'] if u['type']==120)
    request('cancel',units=[factory['id']],action='addon')
    observe('addon_cancelled',lambda s:not any(u['id']==addon['id'] for u in s['units']) and s['minerals']>state['minerals'])
    cc=unit(106);before=request();old={u['id'] for u in before['units']};target=(28*32,31*32)
    request('rally',units=[cc['id']],x=target[0],y=target[1])
    observe('rally_position_set',lambda s:any(u['id']==cc['id'] and u.get('rally',{}).get('x')==target[0] and u.get('rally',{}).get('y')==target[1] for u in s['units']))
    request('train',units=[cc['id']],type=7)
    observe('rally_new_worker_arrived',lambda s:any(u['type']==7 and u['id'] not in old and math.dist((u['x'],u['y']),target)<64 for u in s['units']),60)
    tank=unit(5);request('move',units=[tank['id']],x=55*32,y=31*32)
    state=observe('repair_target_damaged',lambda s:any(u['id']==tank['id'] and u['hp']<tank['hp'] for u in s['units']))
    damaged=next(u for u in state['units'] if u['id']==tank['id'])
    request('move',units=[tank['id']],x=35*32,y=28*32)
    request('repair',units=[unit(7)['id']],target=tank['id'])
    observe('repair_hp_increased',lambda s:any(u['id']==tank['id'] and u['hp']>damaged['hp'] for u in s['units']))
    mineral=next(u for u in request()['units'] if u['type']==176);request('rally',units=[cc['id']],target=mineral['id'])
    observe('rally_unit_set',lambda s:any(u['id']==cc['id'] and u.get('rally',{}).get('target')==mineral['id'] for u in s['units']))
    print('PASS: additional production and repair transitions',flush=True)

if __name__=='__main__':main()
