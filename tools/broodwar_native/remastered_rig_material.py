"""Visibility/depth audit and 2K UV projection for a fitted rig."""
import json
import pathlib
import sys
import numpy as np
from PIL import Image
from remastered_rig_fit import project, raster
from remastered_texture import bake


def prepare(folder):
    config=json.loads((folder/'reconstruction.json').read_text());camera=config['camera']
    mesh=np.load(folder/'uv-mesh.npz');vertices,triangles=mesh['vertices'],mesh['triangles']
    depths=[];audit=[]
    for d in range(17):
        depth,face,_=raster(project(vertices,camera,d),triangles,camera['width'],camera['height'])
        depths.append(depth)
        ref=np.asarray(Image.open(folder/'references'/f'{d:02}-diffuse.png'))
        a=np.asarray(Image.open(folder/'references'/f'{d:02}-ao_depth.png'))[...,3]
        mask=(face>=0)&(ref[...,3]>240)
        x=a[mask].astype(float);y=depth[mask]
        scale,offset=np.linalg.lstsq(np.column_stack((x,np.ones(len(x)))),y,rcond=None)[0]
        rmse=float(np.sqrt(np.mean((scale*x+offset-y)**2)))
        audit.append({'direction':d,'pixelsPerDepthByte':float(scale),'offset':float(offset),'rmsePixels':rmse,
                      'correlation':float(np.corrcoef(x,y)[0,1]),'eligible':bool(scale>0 and rmse<=5)})
    np.savez_compressed(folder/'reconstruction.npz',zbuffers=np.array(depths))
    (folder/'depth-audit.json').write_text(json.dumps({'views':audit,'acceptedViews':sum(a['eligible'] for a in audit),
        'usedForFitting':False,'reason':'An affine agreement with the fitted prior alone does not establish cross-view depth consistency.'},indent=2)+'\n')
    bake(folder)

if __name__=='__main__':prepare(pathlib.Path(sys.argv[1]))
