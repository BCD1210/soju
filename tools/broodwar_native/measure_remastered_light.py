# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy", "pillow"]
# ///
# Run: runtime/asset-venv/bin/python tools/broodwar_native/measure_remastered_light.py
"""Measure a Lambert lighting fit from owned prelit/albedo/normal frames."""
import hashlib
import io
import json
import math
import struct

import numpy as np
from PIL import Image

from client import RUNTIME
from map_assets import dependency
from remastered_layers import frame, normals
from scene_assets import Casc, anim


def measure() -> None:
    """Record residuals and sample provenance before deriving renderer parameters."""
    casc=Casc(dependency(RUNTIME,'CascLib'),RUNTIME/'bottle/drive_c/Program Files (x86)/StarCraft')
    samples=[];provenance=[]
    try:
        relations=casc.read('images.rel')
        for image in (239,247,275,278):
            source=image;seen=set()
            while source not in seen:
                seen.add(source);mode,target=struct.unpack_from('<II',relations,source*8)
                if mode!=0x200:break
                source=target
            data=casc.read(f'anim/main_{source:03}.anim');info,layers=anim(data)
            decoded={}
            for name in ('diffuse','bright','normal'):
                with Image.open(io.BytesIO(layers[name][0])) as texture:decoded[name]=np.asarray(texture.convert('RGBA'))
            for index in range(min(17,len(info['frames']))):
                diffuse=frame(info,decoded['diffuse'],index);bright=frame(info,decoded['bright'],index)
                normal=normals(frame(info,decoded['normal'],index))
                a=diffuse[...,3]/255
                mask=(a>.95)&(bright[...,:3].min(axis=2)>35)&(bright[...,:3].max(axis=2)<235)
                # Source color/albedo ratio isolates broad lighting from painted texture.
                d=(diffuse[...,:3]/255)**2.2;b=(bright[...,:3]/255)**2.2
                ratio=np.mean(d/np.maximum(b,.001),axis=2)
                mask&=(ratio>.05)&(ratio<4)
                design=np.column_stack((np.ones(mask.sum()),normal[mask]))
                samples.append((design[::4],ratio[mask][::4]))
            provenance.append({'image':image,'sourceImage':source,'sourceSha256':hashlib.sha256(data).hexdigest(),'frames':min(17,len(info['frames']))})
        x=np.concatenate([a for a,b in samples]);y=np.concatenate([b for a,b in samples]);keep=np.ones(len(y),dtype=bool)
        for _ in range(5):
            coefficients=np.linalg.lstsq(x[keep],y[keep],rcond=None)[0]
            residual=y-x@coefficients;median=np.median(residual);mad=np.median(np.abs(residual-median))
            keep=np.abs(residual-median)<max(.03,3*1.4826*mad)
        direction=coefficients[1:];direction/=np.linalg.norm(direction)
        elevation=math.radians(32)
        world=np.array([direction[0],direction[1]*math.cos(elevation)+direction[2]*math.sin(elevation),-direction[1]*math.sin(elevation)+direction[2]*math.cos(elevation)])
        r2=1-float(np.sum(residual[keep]**2)/np.sum((y[keep]-y[keep].mean())**2))
        report={'source':'owned-remastered-diffuse-bright-normal','frames':provenance,'samples':len(y),'inliers':int(keep.sum()),
                'cameraSpaceLight':direction.tolist(),'worldLight':world.tolist(),'ambientRatio':float(coefficients[0]),'rSquared':r2,
                'rmse':float(np.sqrt(np.mean(residual[keep]**2))),'cameraElevationAssumptionDegrees':32,
                'limitations':['Lambert fit approximates baked lighting; material and ambient occlusion remain in residuals.',
                    'Unit frame alpha is not a ground shadow. Sun shadow softness has not been measured from a registered ground-shadow pair.'],
                'shadowSoftnessMeasured':False}
        out=RUNTIME/'3d-proof/m4/lighting-fit.json';out.write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps(report,indent=2))
    finally:casc.close()


if __name__=='__main__':
    measure()
