"""Verify native gameplay against an already running private commander test map."""
import json
import time
from client import request, RUNTIME

evidence={"source":"blizzard-starcraft","events":[]}
def record(name,**data):
    event={"check":name,**data}
    evidence["events"].append(event)
    print(json.dumps(event),flush=True)
    (RUNTIME/"native-verification.json").write_text(json.dumps(evidence,indent=2))
def owned(s,kind):
    return [u for u in s["units"] if u["owner"]==s["player"] and u["type"]==kind]
def wait_for(predicate,seconds=180):
    deadline=time.monotonic()+seconds
    while time.monotonic()<deadline:
        s=request()
        if predicate(s):return s
        time.sleep(0.5)
    raise TimeoutError("gameplay condition not observed")

s=request()
assert s["source"]=="blizzard-starcraft" and s["broodWar"]
assert not s["truncated"]
record("native_connection",frame=s["frame"],pid=s["pid"])
base=s["minerals"]
workers=owned(s,7)
minerals=[u for u in s["units"] if u["type"] in(176,177,178)]
assert workers
if not minerals:
    # A fresh normal-menu start correctly keeps the mineral line under fog.
    # Walk a real SCV toward the known fixture location before targeting it.
    scout=workers[0]
    request("move",units=[scout["id"]],x=3450,y=1900)
    s=wait_for(lambda s:any(u["type"] in (176,177,178) for u in s["units"]),60)
    record("scout_minerals",frame=s["frame"],unit=scout["id"])
    workers=owned(s,7)
    minerals=[u for u in s["units"] if u["type"] in (176,177,178)]
target=min(minerals,key=lambda u:(u["x"]-workers[0]["x"])**2+(u["y"]-workers[0]["y"])**2)
request("gather",units=[u["id"] for u in workers],target=target["id"])
s=wait_for(lambda s:s["minerals"]>base)
record("gather",before=base,after=s["minerals"],frame=s["frame"])
marine=owned(s,0)[0]
try:
    request("gather",units=[marine["id"]],target=target["id"])
except RuntimeError as e:
    assert "workers" in str(e)
    record("reject_non_worker",error=str(e))
else:raise AssertionError("marine incorrectly accepted as worker")
try:
    request("move",units=["invalid"],x=1,y=1)
except RuntimeError as e:
    assert "stale unit" in str(e)
    record("reject_stale_unit",error=str(e))
else:raise AssertionError("invalid handle accepted")
s=wait_for(lambda s:s["minerals"]>=56)
old={u["id"] for u in owned(s,7)}
cc=owned(s,106)[0]
before=s["minerals"]
ack=request("train",units=[cc["id"]],type=7)
s=wait_for(lambda s:len(owned(s,7))>len(old),60)
worker=next(u for u in owned(s,7) if u["id"] not in old)
record("train_scv",before_count=len(old),after_count=len(owned(s,7)),minerals_before=before,ack=ack,frame=s["frame"])
s=wait_for(lambda s:s["minerals"]>=100)
before=s["minerals"]
ack=request("build",units=[worker["id"]],type=109,tileX=117,tileY=60)
s=wait_for(lambda s:bool(owned(s,109)),30)
depot=owned(s,109)[0]
assert (depot["x"],depot["y"])==(3792,1952),depot
record("depot_started",minerals_before=before,minerals_after=s["minerals"],unit=depot,ack=ack)
s=wait_for(lambda s:any(u["completed"] for u in owned(s,109)),90)
record("depot_completed",frame=s["frame"],unit=owned(s,109)[0])
marine=owned(s,0)[0];worker=next(u for u in owned(s,7) if u["id"]==worker["id"])
request("follow",units=[marine["id"]],target=worker["id"])
request("move",units=[worker["id"]],x=3600,y=1800)
s=wait_for(lambda s:any(u["id"]==marine["id"] and abs(u["x"]-marine["x"])+abs(u["y"]-marine["y"])>50 for u in owned(s,0)),30)
record("follow",before=marine,after=next(u for u in owned(s,0) if u["id"]==marine["id"]),frame=s["frame"])
record("PASS",vr_headset_tested=False)
