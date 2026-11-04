# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Record two minutes of original private-map building and combat transitions."""
import json
import time
from client import RUNTIME, request


def main():
    state=request()
    assert 'Soju-Acceptance-M4-' in state['map']['path'], 'Private fixture required'
    folder=RUNTIME/'3d-proof/m4/demo-final';folder.mkdir(parents=True,exist_ok=True)
    initial=state
    def owned(kind):
        return next(u['id'] for u in request()['units'] if u['type']==kind and u['owner']==initial['player'])
    events=[(0,{'op':'build','units':[owned(7)],'type':109,'tileX':28,'tileY':20}),
            (8,{'op':'build','units':[owned(41)],'type':143,'tileX':27,'tileY':39}),
            (15,{'op':'train','units':[owned(111)],'type':0}),
            (20,{'op':'morph','units':[owned(35)],'type':37}),
            (28,{'op':'attack_move','units':[owned(k) for k in (0,5,37,38,65)],'x':55*32,'y':31*32}),
            (40,{'op':'train','units':[owned(111)],'type':0}),
            (55,{'op':'train','units':[owned(106)],'type':7})]
    started=time.monotonic();started_unix=time.time();issued=[];snapshots=[];since=state.get('eventSeq',0)
    for second in range(121):
        while time.monotonic()-started<second:time.sleep(.05)
        while events and events[0][0]<=second:
            when,payload=events.pop(0)
            try:result=request(**payload)
            except (RuntimeError,OSError) as error:result={'error':str(error)}
            issued.append({'second':second,'command':payload,'result':result})
        state=request(since=since);since=state.get('eventSeq',since)
        (folder/f'state-{second:03d}.json').write_text(json.dumps(state)+'\n')
        snapshots.append({'second':second,'frame':state['frame'],'events':state.get('events',[]),
            'dying':len(state.get('dyingUnits',[])),'lone':len(state.get('loneSprites',[]))})
        if second%10==0:print(json.dumps(snapshots[-1]),flush=True)
    report={'startedUnix':started_unix,'endedUnix':time.time(),'elapsed':time.monotonic()-started,'commands':issued,'snapshots':snapshots,
            'source':'original private game; commands do not imply completion'}
    (folder/'session.json').write_text(json.dumps(report,indent=2)+'\n')

if __name__=='__main__':main()
