"""Sweep orthographic elevation and azimuth sign against measured silhouettes.

This coarse voxel score is calibration evidence, not the Blender acceptance gate.
"""
import argparse
import json
import numpy as np
from scipy import ndimage
from remastered_layers import layers, source_manifest, frame
from remastered_reconstruct import project
from client import RUNTIME


def sweep(kind, image, resolution=100):
    info, atlases=layers(source_manifest(),image)
    width,height=info['width'],info['height']
    masks=[frame(info,atlases['diffuse'],d)[...,3]>127 for d in range(17)]
    extent=max(width,height)*.47
    axis=np.linspace(-extent,extent,resolution,dtype=np.float32)
    points=np.stack(np.meshgrid(axis,axis,axis,indexing='ij'),-1).reshape(-1,3)
    rows=[]
    for sign in (1,-1):
        for elevation in range(15,76,5):
            inside=np.ones(len(points),bool)
            for d,mask in enumerate(masks):
                x,y,_=project(points,d*sign,elevation,width,height)
                inside &= ndimage.map_coordinates(mask.astype(float),[y,x],order=0,mode='constant')>.5
            occupied=points[inside];ious=[]
            for d,mask in enumerate(masks):
                x,y,_=project(occupied,d*sign,elevation,width,height)
                xi,yi=np.rint(x).astype(int),np.rint(y).astype(int)
                valid=(xi>=0)&(xi<width)&(yi>=0)&(yi<height)
                image_mask=np.zeros((height,width),bool);image_mask[yi[valid],xi[valid]]=True
                image_mask=ndimage.maximum_filter(image_mask,size=3)
                ious.append(float((image_mask&mask).sum()/max(1,(image_mask|mask).sum())))
            rows.append({'azimuthMultiplier':-sign,'elevation':elevation,'meanIoU':float(np.mean(ious)),
                         'minIoU':min(ious),'directions':ious})
    return {'type':kind,'resolution':resolution,'bestMeanIoU':max(rows,key=lambda row:row['meanIoU']),
            'protocol':'Voxel occupancy projection with 3x3 dilation; not final mesh renders','rows':rows}


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--resolution',type=int,default=100);args=parser.parse_args()
    report=[sweep(k,i,args.resolution) for k,i in ((0,239),(7,247))]
    target=RUNTIME/'3d-proof/rm/calibration.json';target.write_text(json.dumps(report,indent=2)+'\n')
    print(target)
