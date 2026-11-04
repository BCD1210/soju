# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
# Run: python3 tools/broodwar_native/live_extended_check.py
"""Observe extended commands in a fresh private M4 acceptance map."""
import json
import time

from client import RUNTIME, request


def main() -> None:
    """Record actual unit/resource transitions, never count ACKs as acceptance."""
    initial = request()
    assert 'Soju-Acceptance-M4-' in initial['map']['path'], 'Private acceptance fixture required'
    folder = RUNTIME/'3d-proof/m4/commands'
    folder.mkdir(parents=True,exist_ok=True)
    results = []

    def unit(kind: int):
        state = request()
        return next(u for u in state['units'] if u['type']==kind and u['owner']==state['player'])

    def observe(name, predicate, seconds=15):
        deadline = time.monotonic()+seconds
        while time.monotonic()<deadline:
            state = request()
            if predicate(state):
                (folder/(name+'.json')).write_text(json.dumps(state,indent=2)+'\n')
                results.append({'check':name,'frame':state['frame'],'passed':True})
                (folder/'results.json').write_text(json.dumps(results,indent=2)+'\n')
                print(json.dumps(results[-1]),flush=True)
                return state
            time.sleep(.1)
        (folder/(name+'-failed.json')).write_text(json.dumps(state,indent=2)+'\n')
        raise AssertionError(f'Native transition not observed: {name}')

    for action, kind, field, expected in [('siege',5,'type',30),('unsiege',30,'type',5),
                                         ('burrow',37,'burrowed',True),('unburrow',37,'burrowed',False),
                                         ('cloak',8,'cloaked',True),('decloak',8,'cloaked',False)]:
        selected = unit(kind)
        request('ability',units=[selected['id']],action=action)
        observe(action,lambda s:any(u['id']==selected['id'] and u[field]==expected for u in s['units']))
    selected = unit(0)
    request('ability',units=[selected['id']],action='stim')
    observe('stim',lambda s:any(u['id']==selected['id'] and u['status']['stim']>0 and u['hp']<selected['hp'] for u in s['units']))
    transport, passenger = unit(11), unit(0)
    request('load',units=[transport['id']],target=passenger['id'])
    observe('load',lambda s:any(u['id']==passenger['id'] and u['transported'] for u in s['units']),30)
    request('unload',units=[transport['id']])
    observe('unload',lambda s:any(u['id']==passenger['id'] and not u['transported'] for u in s['units']))
    cc = unit(106)
    before = request()
    request('train',units=[cc['id']],type=7)
    observe('train_queued',lambda s:s['minerals']<before['minerals'] and any(u['id']==cc['id'] and 7 in u['buildQueue'] for u in s['units']))
    request('cancel',units=[cc['id']],action='train')
    observe('cancel_train',lambda s:s['minerals']==before['minerals'] and any(u['id']==cc['id'] and 7 not in u['buildQueue'] for u in s['units']))
    for operation,kind,target in [('research',112,24),('upgrade',122,0)]:
        producer = unit(kind)
        before = request()
        request(operation,units=[producer['id']],type=target)
        observe(operation+'_started',lambda s:s['minerals']<before['minerals'] and s['gas']<before['gas'])
        request('cancel',units=[producer['id']],action=operation)
        observe('cancel_'+operation,lambda s:s['minerals']==before['minerals'] and s['gas']==before['gas'])
    larva = unit(35)
    before = request()
    request('morph',units=[larva['id']],type=41)
    observe('morph_egg',lambda s:any(u['id']==larva['id'] and u['type']==36 for u in s['units']))
    request('cancel',units=[larva['id']],action='morph')
    observe('cancel_morph',lambda s:s['minerals']==before['minerals'] and
            not any(u['id']==larva['id'] and u['type']==36 for u in s['units']))
    before = request()
    request('train',units=[cc['id']],type=7)
    observe('train_completed',lambda s:sum(u['type']==7 for u in s['units'])>sum(u['type']==7 for u in before['units']),60)
    print('PASS: extended native transitions',flush=True)


if __name__ == '__main__':
    main()
