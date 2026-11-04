"""Exercise native combat only inside the private Soju combat fixture."""
import collections
import json
import time
from client import RUNTIME, request

def main():
    state=request()
    assert 'Soju-Combat-Test-' in state['map']['path'], 'Private combat fixture required'
    assert state.get('protocol') == 2
    player=state['player']
    marines=[u['id'] for u in state['units'] if u['owner']==player and u['type']==0]
    base=next(u for u in state['units'] if u['owner']==player and u['townHall'])
    assert len(marines)==2, 'Fresh fixture with two Marines required'
    sequence=state['eventSeq'];start=state['frame'];events=[];bullets=[];targeted=False
    request('attack_move',units=marines,x=base['x']-420,y=base['y']+70)
    deadline=time.monotonic()+45
    death_at=None
    while time.monotonic()<deadline:
        state=request(since=sequence)
        assert not state.get('eventsLost'), 'Native event journal overflow'
        sequence=state['eventSeq'];events.extend(state['events']);bullets.extend(state['bullets'])
        enemies=[u for u in state['units'] if u['owner']!=player and u['owner']<8 and state['alliances'][u['owner']]==0]
        if enemies and not targeted:
            target=next((u for u in enemies if u['type']==0),enemies[0])
            living=[u['id'] for u in state['units'] if u['id'] in marines]
            request('attack',units=living,target=target['id']);targeted=True
        if any(e['kind']=='death' for e in events) and death_at is None:death_at=time.monotonic()
        if death_at and time.monotonic()-death_at>3:break
        time.sleep(.05)
    counts=dict(collections.Counter(e['kind'] for e in events))
    diagnostic=json.loads((RUNTIME/'3d-xr-diagnostics.json').read_text())
    checks={
        'nativeFramesAdvance':state['frame']>start,
        'targetAttackAcknowledged':targeted,
        'nativeAttack':counts.get('attack',0)>=2,
        'nativeDamage':counts.get('damage',0)>0,
        'nativeDeath':counts.get('death',0)>0,
        'rendererLive':time.time()-diagnostic['time']<3 and diagnostic.get('protocol')==2,
        'rendererReceivedAttack':diagnostic.get('combatEvents',{}).get('attack',0)>0,
        'rendererRetiredDeath':diagnostic.get('deathAnimations',0)>0,
        'noMissingAssets':not diagnostic.get('missingTypes'),
    }
    report={'passed':all(checks.values()),'checks':checks,'events':counts,'nativeProjectilesObserved':len(bullets),
        'startFrame':start,'endFrame':state['frame'],'map':state['map']['path'],
        'evidence':events,'renderer':diagnostic}
    path=RUNTIME/'integration-tests/live-combat.json';path.parent.mkdir(exist_ok=True)
    path.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({k:v for k,v in report.items() if k not in ('evidence','renderer')},indent=2))
    assert report['passed'], 'Live combat acceptance failed; see saved report'

if __name__=='__main__':main()
