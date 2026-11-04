"""Derive plateau/cliff geometry and constrained ramp heights from native VF4 flags."""
from collections import deque
import struct

def prepare(width,height,tiles,vf4):
    sw,sh=width*4,height*4
    flags=[0]*(sw*sh)
    for ty in range(height):
        for tx in range(width):
            record=struct.unpack_from("<16H",vf4,tiles[ty*width+tx]*32)
            for y in range(4):
                for x in range(4):flags[(ty*4+y)*sw+tx*4+x]=record[y*4+x]
    level=lambda f:2 if f&4 else 1 if f&2 else 0
    heights=[float(level(f)*2) for f in flags]
    candidates=set()
    for ty in range(height):
        for tx in range(width):
            cells=[(ty*4+y)*sw+tx*4+x for y in range(4) for x in range(4)]
            if any(flags[i]&16 for i in cells):
                candidates.update(i for i in cells if flags[i]&1)
    def neighbors(i):
        x,y=i%sw,i//sw
        if x:yield i-1
        if x+1<sw:yield i+1
        if y:yield i-sw
        if y+1<sh:yield i+sw
    remaining=set(candidates);ramps=[];unresolved=[]
    while remaining:
        seed=remaining.pop();region={seed};queue=deque([seed])
        while queue:
            for j in neighbors(queue.popleft()):
                if j in remaining:remaining.remove(j);region.add(j);queue.append(j)
        boundary={}
        for i in region:
            for j in neighbors(i):
                if j not in region and flags[j]&1 and not flags[j]&16:
                    boundary.setdefault(level(flags[j]),set()).add(i)
        levels=sorted(boundary)
        if len(levels)!=2 or levels[1]-levels[0]!=1:
            unresolved.append(len(region));continue
        def distances(seeds):
            out={i:0 for i in seeds};q=deque(seeds)
            while q:
                i=q.popleft()
                for j in neighbors(i):
                    if j in region and j not in out:out[j]=out[i]+1;q.append(j)
            return out
        low,high=levels;dl=distances(boundary[low]);dh=distances(boundary[high])
        if any(dl[i]+dh[i]==0 for i in region):
            unresolved.append(len(region));continue
        for i in region:heights[i]=2*(low+(dl[i]+0.5)/(dl[i]+dh[i]+1))
        ramps.extend(sorted(region))
    return {"source":"native VF4 minitile flags","heightStep":2,"width":sw,"height":sh,
        "flags":flags,"heights":heights,"rampCells":ramps,"unresolvedRampComponents":unresolved}
