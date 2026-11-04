"""Exercise the actual OpenBW simulation, not a mock or rendered animation."""
import argparse
import json
import math
import os
import pathlib
import queue
import subprocess
import threading
import time

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument("--bridge",required=True)
    ap.add_argument("--data",required=True)
    ap.add_argument("--map",required=True)
    args=ap.parse_args()
    with subprocess.Popen([args.bridge,args.map,"--fast"], env={**os.environ,"OPENBW_MPQ_PATH":args.data},
                          stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True) as proc:
        messages=queue.Queue()
        def reader():
            for line in proc.stdout:
                try: messages.put(json.loads(line))
                except ValueError: pass
            messages.put({"kind":"eof"})
        threading.Thread(target=reader,daemon=True).start()
        state=None; meta=None; sequence=0; results={}; deadline=time.monotonic()+60
        def poll():
            nonlocal state,meta
            if time.monotonic()>deadline: raise TimeoutError("game verification exceeded 60 seconds")
            msg=messages.get(timeout=10)
            if msg["kind"]=="eof": raise RuntimeError(proc.stderr.read()[-2000:])
            if msg["kind"]=="map": meta=msg
            if msg["kind"]=="state": state=msg
            if msg["kind"]=="result": results[msg["id"]]=msg
            return msg
        def wait(predicate,frames=10000):
            start=state["frame"] if state else 0
            while not predicate():
                poll()
                if state and state["frame"]>start+frames: raise AssertionError("game frame limit exceeded")
        def cmd(**kwargs):
            nonlocal sequence
            sequence+=1; n=sequence
            proc.stdin.write(json.dumps({"id":n,**kwargs})+"\n");proc.stdin.flush()
            wait(lambda:n in results)
            return results[n]
        def unit(id):
            return next((u for u in state["units"] if u["id"]==id),None)
        try:
            wait(lambda:state and meta and state["commander"]>=0)
            player=meta["player"]; hero=state["commander"]; initial=dict(unit(hero))
            workers=[u for u in state["units"] if u["worker"] and u["owner"]==player]
            center=next(u for u in state["units"] if u["type"]==106 and u["owner"]==player)
            minerals=[u for u in state["units"] if u["mineral"]]
            start_minerals=state["minerals"]
            assert not cmd(op="move",unit=minerals[0]["id"],x=1,y=1)["ok"],"neutral unit accepted"
            assert not cmd(op="move",unit=hero,x=-1,y=0)["ok"],"out of map accepted"
            for w in workers:
                m=min(minerals,key=lambda m:(w["x"]-m["x"])**2+(w["y"]-m["y"])**2)
                assert cmd(op="gather",unit=w["id"],target=m["id"])["ok"]
            assert cmd(op="move",unit=hero,x=initial["x"]-128,y=initial["y"]+96)["ok"]
            wait(lambda:state["minerals"]>start_minerals and math.hypot(unit(hero)["x"]-initial["x"],unit(hero)["y"]-initial["y"])>32)
            print("PASS: normal mining increased minerals",start_minerals,"->",state["minerals"],flush=True)
            print("PASS: commander moved through simulation",initial["x"],initial["y"],"->",unit(hero)["x"],unit(hero)["y"],flush=True)
            def build(type,cost):
                wait(lambda:state["minerals"]>=cost)
                worker=workers[0]["id"]
                accepted=None
                for dy in (128,224,320,-128,-224):
                    for dx in (-192,-320,128,256,0):
                        x=(center["x"]+dx)//32*32;y=(center["y"]+dy)//32*32
                        r=cmd(op="build",unit=worker,type=type,x=x,y=y)
                        if r["ok"]: accepted=r; break
                    if accepted:break
                assert accepted,"no valid building placement"
                wait(lambda:any(u["type"]==type and u["owner"]==player and u["completed"] for u in state["units"]))
                b=next(u for u in state["units"] if u["type"]==type and u["owner"]==player and u["completed"])
                m=min(minerals,key=lambda m:(unit(worker)["x"]-m["x"])**2+(unit(worker)["y"]-m["y"])**2)
                cmd(op="gather",unit=worker,target=m["id"])
                print("PASS: completed",b["name"],"at frame",state["frame"],flush=True)
                return b
            supply_before=state["supplyTotal"]
            build(109,100)
            wait(lambda:state["supplyTotal"]>supply_before)
            barracks=build(111,150)
            wait(lambda:state["minerals"]>=50)
            assert cmd(op="train",unit=barracks["id"],type=0)["ok"]
            wait(lambda:any(u["type"]==0 and u["id"]!=hero and u["owner"]==player and u["completed"] for u in state["units"]))
            marine=next(u for u in state["units"] if u["type"]==0 and u["id"]!=hero and u["owner"]==player and u["completed"])
            assert cmd(op="follow",unit=marine["id"],target=hero)["ok"]
            wait(lambda:unit(marine["id"])["target"]==hero)
            before=math.hypot(marine["x"]-unit(hero)["x"],marine["y"]-unit(hero)["y"])
            wait(lambda:math.hypot(unit(marine["id"])["x"]-unit(hero)["x"],unit(marine["id"])["y"]-unit(hero)["y"])<before-16)
            print("PASS: trained Marine follows commander; supply",state["supplyUsed"]/2,"/",state["supplyTotal"]/2,flush=True)
            assert cmd(op="quit")["ok"]
            proc.stdin.close();proc.wait(timeout=5)
            assert proc.returncode==0
            print("PASS: clean shutdown; no headset/VR rendering assertions",flush=True)
        finally:
            if proc.poll() is None:proc.terminate();proc.wait(timeout=5)

if __name__=="__main__":main()
