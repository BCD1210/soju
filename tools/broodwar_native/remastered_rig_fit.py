"""Multi-view camera and regularized silhouette fitting of an existing posed rig.

Private outputs only. Camera parameters are held fixed during mesh fitting.
The software rasterizer is a fitting diagnostic; Blender supplies the final gate.
"""
import argparse
import json
import pathlib
import numpy as np
from numba import njit
from scipy import ndimage, optimize, sparse
from scipy.sparse.linalg import factorized
from PIL import Image
from client import RUNTIME
from remastered_layers import source_manifest, layers, frame


@njit(cache=True)
def raster(points, triangles, width, height):
    depth = np.full((height,width), -1e20, np.float64)
    face = np.full((height,width), -1, np.int32)
    bary = np.zeros((height,width,3), np.float64)
    for i in range(len(triangles)):
        a,b,c = points[triangles[i]]
        det = (b[0]-a[0])*(c[1]-a[1])-(c[0]-a[0])*(b[1]-a[1])
        if abs(det)<1e-10:
            continue
        x0=max(0,int(np.floor(min(a[0],b[0],c[0]))))
        x1=min(width-1,int(np.ceil(max(a[0],b[0],c[0]))))
        y0=max(0,int(np.floor(min(a[1],b[1],c[1]))))
        y1=min(height-1,int(np.ceil(max(a[1],b[1],c[1]))))
        for y in range(y0,y1+1):
            for x in range(x0,x1+1):
                u=((x+.5-a[0])*(c[1]-a[1])-(y+.5-a[1])*(c[0]-a[0]))/det
                v=((b[0]-a[0])*(y+.5-a[1])-(b[1]-a[1])*(x+.5-a[0]))/det
                w=1-u-v
                if min(u,v,w)>=-1e-9:
                    z=w*a[2]+u*b[2]+v*c[2]
                    if z>depth[y,x]:
                        depth[y,x]=z;face[y,x]=i
                        bary[y,x,0]=w;bary[y,x,1]=u;bary[y,x,2]=v
    return depth,face,bary


def axes(yaw, elevation):
    a,e=np.radians([yaw,elevation])
    right=np.array([-np.cos(a),np.sin(a),0.])
    toward=np.array([np.sin(a)*np.cos(e),np.cos(a)*np.cos(e),np.sin(e)])
    return np.stack((right,-np.cross(toward,right),toward))


def project(vertices, camera, direction):
    matrix=axes(camera['yaws'][direction],camera['elevation'])
    pixels=(vertices-np.array(camera['target']))@matrix.T*camera['pixelsPerUnit']
    pixels[:,:2]+=[camera['width']/2,camera['height']/2]
    return pixels


def score(vertices, triangles, camera, masks):
    results=[]
    for d,mask in enumerate(masks):
        _,face,_=raster(project(vertices,camera,d),triangles,camera['width'],camera['height'])
        ours=face>=0
        results.append(float(np.count_nonzero(ours&mask)/max(1,np.count_nonzero(ours|mask))))
    return results


def prepare(folder,kind):
    source=source_manifest();info,atlases=layers(source,{0:239,7:247}[kind])
    refs=folder/'references';refs.mkdir(exist_ok=True)
    for d in range(17):
        for name,atlas in atlases.items():
            Image.fromarray(frame(info,atlas,d)).save(refs/f'{d:02}-{name}.png')
    masks=np.array([frame(info,atlases['diffuse'],d)[...,3]>127 for d in range(17)])
    return info,masks


def calibrate(folder,kind):
    data=np.load(folder/'source.npz');vertices,triangles=data['vertices'],data['triangles']
    info,masks=prepare(folder,kind)
    center=(vertices.min(0)+vertices.max(0))/2
    spans=[]
    for m in masks:
        ys,xs=np.nonzero(m);spans.append(ys.max()-ys.min())
    scale=float(np.median(spans)/np.ptp(vertices[:,2]))
    camera={'width':info['width'],'height':info['height'],'target':center.tolist(),'pixelsPerUnit':scale}
    # Broad absolute-yaw/sign/elevation search followed by all-view refinement.
    grid=[]
    for sign in (-1,1):
        for elev in (15,25,35,45,55,65):
            for yaw in range(0,360,45):
                c={**camera,'elevation':elev,'yaws':[yaw+sign*d*11.25 for d in range(17)]}
                vals=score(vertices,triangles,c,masks)
                grid.append({'elevation':elev,'yaw':yaw,'sign':sign,'meanIoU':float(np.mean(vals))})
    best=max(grid,key=lambda x:x['meanIoU'])
    print('GRID',kind,best,flush=True)
    sign=best['sign']
    def decode(x):
        return {**camera,'elevation':float(x[0]),'yaws':[float(x[1]+sign*d*11.25) for d in range(17)],
                'pixelsPerUnit':float(x[2]),'target':x[3:6].tolist()}
    def loss(x):
        return 1-np.mean(score(vertices,triangles,decode(x),masks))
    initial=np.array([best['elevation'],best['yaw'],scale,*center])
    bounds=[(5,80),(best['yaw']-40,best['yaw']+40),(scale*.5,scale*1.8),
            *[(c-.6,c+.6) for c in center]]
    result=optimize.minimize(loss,initial,method='Powell',bounds=bounds,
                             options={'maxiter':18,'xtol':.0002,'ftol':.00002})
    fitted=decode(result.x)
    global_scores=score(vertices,triangles,fitted,masks)
    for d in range(17):
        nominal=fitted['yaws'][d]
        def yaw_loss(yaw):
            c={**fitted,'yaws':list(fitted['yaws'])};c['yaws'][d]=float(yaw)
            _,face,_=raster(project(vertices,c,d),triangles,c['width'],c['height'])
            ours=face>=0;mask=masks[d]
            return 1-np.count_nonzero(ours&mask)/max(1,np.count_nonzero(ours|mask))
        candidates=np.linspace(nominal-5.625,nominal+5.625,31)
        fitted['yaws'][d]=float(min(candidates,key=yaw_loss))
    fitted['projection']='orthographic'
    fitted['azimuthSign']=sign
    fitted['focalLength']='not identifiable in orthographic model'
    fitted['yawRefinementBoundDegrees']=5.625
    vals=score(vertices,triangles,fitted,masks)
    report={'camera':fitted,'grid':grid,'globalCameraIoU':global_scores,'perDirectionIoU':vals,
            'minimumIoU':min(vals),'meanIoU':float(np.mean(vals)),
            'converged':bool(result.success),'evaluations':int(result.nfev),
            'limitations':['Camera estimated against different source geometry; geometry-camera ambiguity remains.',
                          'No independent focal-length or flat-ground calibration; orthographic hypothesis only.',
                          'No per-direction translation, scaling, image registration or image resizing.']}
    (folder/'camera.json').write_text(json.dumps(report,indent=2)+'\n')
    print('CAMERA',kind,json.dumps(fitted),'min',min(vals),'mean',np.mean(vals),flush=True)


def vertex_normals(vertices, triangles):
    edges=vertices[triangles]
    face=np.cross(edges[:,1]-edges[:,0],edges[:,2]-edges[:,0])
    result=np.zeros_like(vertices)
    for j in range(3):np.add.at(result,triangles[:,j],face)
    return result/np.maximum(np.linalg.norm(result,axis=1,keepdims=True),1e-12)


def fit(folder, iterations=100, regularization=2., label='fit'):
    data=np.load(folder/'source.npz');original,triangles=data['vertices'],data['triangles']
    camera=json.loads((folder/'camera.json').read_text())['camera']
    masks=np.array([np.array(Image.open(folder/'references'/f'{d:02}-diffuse.png'))[...,3]>127 for d in range(17)])
    # Weld positional duplicates for optimization, then expand to the original UV/skin indices.
    _,indices,inverse=np.unique(np.round(original,6),axis=0,return_index=True,return_inverse=True)
    start=original[indices];triangles=inverse[triangles]
    vertices=start.copy()
    edges=np.unique(np.sort(np.concatenate([triangles[:,[0,1]],triangles[:,[1,2]],triangles[:,[2,0]]]),axis=1),axis=0)
    edges=edges[edges[:,0]!=edges[:,1]]
    adjacency=sparse.coo_matrix((np.ones(len(edges)*2),(edges.ravel(),edges[:,::-1].ravel())),shape=(len(start),len(start))).tocsr()
    degree=np.asarray(adjacency.sum(1)).ravel()
    lap=sparse.diags(degree)-adjacency
    solve=factorized((sparse.eye(len(start))+regularization*lap).tocsc())
    target_sdf=np.array([ndimage.distance_transform_edt(~m)-ndimage.distance_transform_edt(m) for m in masks])
    gradients=np.array([np.stack(np.gradient(s),-1) for s in target_sdf])
    history=[];best_score=-1;best=vertices.copy()
    for iteration in range(iterations+1):
        forces=np.zeros_like(vertices);counts=np.zeros(len(vertices));ious=[]
        normals=vertex_normals(vertices,triangles)
        for d,mask in enumerate(masks):
            pixels=project(vertices,camera,d)
            _,faces,bary=raster(pixels,triangles,camera['width'],camera['height'])
            ours=faces>=0
            ious.append(float(np.count_nonzero(ours&mask)/max(1,np.count_nonzero(ours|mask))))
            if iteration==iterations:continue
            boundary=ours & ~ndimage.binary_erosion(ours)
            ys,xs=np.nonzero(boundary)
            ids=triangles[faces[ys,xs]];weights=bary[ys,xs]
            distances=target_sdf[d,ys,xs]
            grad=gradients[d,ys,xs][:,::-1]
            grad/=np.maximum(np.linalg.norm(grad,axis=1,keepdims=True),1e-8)
            screen_force=-np.clip(distances,-8,8)[:,None]*grad
            world_force=screen_force@axes(camera['yaws'][d],camera['elevation'])[:2]/camera['pixelsPerUnit']
            for j in range(3):
                np.add.at(forces,ids[:,j],world_force*weights[:,j,None])
                np.add.at(counts,ids[:,j],weights[:,j])
            # Missing target protrusions pull the closest currently visible contour.
            missing=mask & ~ours
            if missing.any() and ours.any():
                dist,near=ndimage.distance_transform_edt(~ours,return_indices=True)
                yy,xx=np.nonzero(missing & ~ndimage.binary_erosion(mask))
                ny,nx=near[:,yy,xx]
                good=faces[ny,nx]>=0
                yy,xx,ny,nx=yy[good],xx[good],ny[good],nx[good]
                pull=np.stack((xx-nx,yy-ny),-1).astype(float)
                pull*=np.minimum(1,8/np.maximum(np.linalg.norm(pull,axis=1),1e-8))[:,None]
                force=pull@axes(camera['yaws'][d],camera['elevation'])[:2]/camera['pixelsPerUnit']
                ids=triangles[faces[ny,nx]];weights=bary[ny,nx]
                for j in range(3):
                    np.add.at(forces,ids[:,j],force*weights[:,j,None]*.5)
                    np.add.at(counts,ids[:,j],weights[:,j]*.5)
        mean=float(np.mean(ious));minimum=min(ious)
        history.append({'iteration':iteration,'meanIoU':mean,'minimumIoU':minimum,'directions':ious})
        if mean>best_score:best_score=mean;best=vertices.copy();best_iteration=iteration
        if iteration%10==0:print(label,folder.parent.name,iteration,'mean',mean,'min',minimum,flush=True)
        forces/=np.maximum(counts[:,None],1)
        # Normal displacement plus a small tangential component allows contour sliding.
        normal_part=normals*np.sum(forces*normals,axis=1,keepdims=True)
        forces=.85*normal_part+.15*forces
        delta=np.column_stack([solve(forces[:,i]) for i in range(3)])
        displacement=vertices-start
        delta-=.015*np.column_stack([solve(lap@displacement[:,i]) for i in range(3)])
        vertices+=delta*.7
    np.savez_compressed(folder/f'{label}.npz',vertices=best[inverse],triangles=data['triangles'])
    report={'label':label,'iterations':iterations,'regularization':regularization,'bestIteration':best_iteration,
            'best':history[best_iteration],'history':history,'maxDisplacementUnits':float(np.linalg.norm(best-start,axis=1).max()),
            'skinWeights':'unchanged; rest displacements require inverse idle skin matrices',
            'method':'raster contour SDF forces, 85% normal / 15% tangential, welded Laplacian displacement regularization'}
    (folder/f'{label}.json').write_text(json.dumps(report,indent=2)+'\n')


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('stage',choices=['camera','fit']);p.add_argument('--types',default='0,7')
    p.add_argument('--iterations',type=int,default=100);p.add_argument('--regularization',type=float,default=2)
    p.add_argument('--label',default='fit');a=p.parse_args()
    for kind in map(int,a.types.split(',')):
        folder=RUNTIME/'models-remastered'/str(kind)/'rigged'
        if a.stage=='camera':calibrate(folder,kind)
        else:fit(folder,a.iterations,a.regularization,a.label)

if __name__=='__main__':main()
