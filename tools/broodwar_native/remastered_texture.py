"""Rasterize a UV atlas and project visibility-weighted owned sprite materials."""
import json
import pathlib
import sys
import numpy as np
from PIL import Image
from scipy.ndimage import distance_transform_edt, map_coordinates
from remastered_reconstruct import basis, project
from remastered_layers import normals


def bake(folder, size=2048):
    config = json.loads((folder/'reconstruction.json').read_text())
    fitted_camera = config.get('camera')
    if fitted_camera:
        from remastered_rig_fit import project as rig_project, axes
    mesh = np.load(folder/'uv-mesh.npz')
    vertices, triangles, uv, vnormals = (mesh[k] for k in ('vertices','triangles','uv','normals'))
    xyz = np.zeros((size, size, 3), np.float32)
    normal = np.zeros_like(xyz)
    tangent = np.zeros_like(xyz)
    bitangent = np.zeros_like(xyz)
    occupied = np.zeros((size, size), bool)
    for index, ids in enumerate(triangles):
        coords = uv[index]*[size, -size]+[0, size]
        lo = np.maximum(np.floor(coords.min(0)).astype(int), 0)
        hi = np.minimum(np.ceil(coords.max(0)).astype(int)+1, size)
        if np.any(hi <= lo):
            continue
        x, y = np.meshgrid(np.arange(lo[0],hi[0])+.5, np.arange(lo[1],hi[1])+.5)
        matrix = np.column_stack((coords[1]-coords[0], coords[2]-coords[0]))
        if abs(np.linalg.det(matrix)) < 1e-8:
            continue
        bary = (np.stack((x,y),-1)-coords[0])@np.linalg.inv(matrix).T
        weights = np.concatenate((1-bary.sum(-1,keepdims=True),bary),-1)
        valid = np.all(weights>=-1e-5,axis=-1)
        rows, cols = np.nonzero(valid)
        rows += lo[1];cols += lo[0]
        xyz[rows,cols] = weights[valid]@vertices[ids]
        normal[rows,cols] = weights[valid]@vnormals[ids]
        duv = uv[index,1:]-uv[index,0]
        if abs(np.linalg.det(duv)) > 1e-10:
            tb = np.linalg.solve(duv,vertices[ids[1:]]-vertices[ids[0]])
            tangent[rows,cols] = tb[0]
            bitangent[rows,cols] = tb[1]
        occupied[rows,cols] = True
    rows, cols = np.nonzero(occupied)
    points = xyz[rows,cols]
    ns = normal[rows,cols];ns /= np.maximum(np.linalg.norm(ns,axis=-1,keepdims=True),1e-8)
    ts = tangent[rows,cols];ts -= ns*np.sum(ts*ns,axis=-1,keepdims=True)
    ts /= np.maximum(np.linalg.norm(ts,axis=-1,keepdims=True),1e-8)
    bs = np.cross(ns,ts)*np.sign(np.sum(np.cross(ns,ts)*bitangent[rows,cols],axis=-1,keepdims=True))
    zbuffers = np.load(folder/'reconstruction.npz')['zbuffers']
    names = config['layers']
    accum = {k:np.zeros((len(points),3),np.float32) for k in names}
    accum['normal'] = np.zeros((len(points),3),np.float32)
    total = np.zeros(len(points),np.float32)
    diffuse_second = np.zeros((len(points),3),np.float32)
    contributors = np.zeros(len(points),np.int16)
    for d in range(config.get('viewCount',17)):
        if fitted_camera:
            x,y,z = rig_project(points, fitted_camera, d).T
            x=x-.5;y=y-.5
        else:
            x,y,z = project(points,d,config['elevation'],config['width'],config['height'])
        images = {k:np.asarray(Image.open(folder/'references'/f'{d:02}-{k}.png')).astype(np.float32)/255 for k in names}
        alpha = map_coordinates(images['diffuse'][...,3],[y,x],order=1,mode='constant')
        front = map_coordinates(zbuffers[d],[y,x],order=1,mode='constant',cval=-1e6)
        if fitted_camera:
            right, down, toward = axes(fitted_camera['yaws'][d], fitted_camera['elevation'])
            up = -down
        else:
            right,up,toward = basis(d,config['elevation'])
        weight = np.maximum(ns@toward,0)**4 * (alpha>.5) * (abs(front-z)<config.get('visibilityTolerancePixels',config.get('voxelSizePixels',1)*4))
        total += weight
        contributors += weight>1e-5
        for k, pixels in images.items():
            if k=='normal':
                camera = normals((pixels*255).astype(np.uint8))
                sampled = np.column_stack([map_coordinates(camera[...,c],[y,x],order=1,mode='constant') for c in range(3)])
                world = sampled@np.stack((right,up,toward))
                value = np.column_stack((np.sum(world*ts,axis=1),np.sum(world*bs,axis=1),np.sum(world*ns,axis=1)))
            else:
                value = np.column_stack([map_coordinates(pixels[...,c],[y,x],order=1,mode='constant') for c in range(3)])
            accum[k] += value*weight[:,None]
            if k=='diffuse':diffuse_second += value*value*weight[:,None]
    known = total>1e-6
    if not known.any():
        raise ValueError('No observed UV surface; camera or visibility is invalid')
    coverage = np.zeros((size,size),bool);coverage[rows[known],cols[known]]=True
    _, indices = distance_transform_edt(~coverage,return_indices=True)
    outputs = {}
    for k,value in accum.items():
        value /= np.maximum(total[:,None],1e-8)
        if k=='normal':
            value /= np.maximum(np.linalg.norm(value,axis=1,keepdims=True),1e-8)
            value = value*.5+.5
        atlas = np.zeros((size,size,3),np.float32);atlas[rows,cols]=value
        # Pad UV islands and label unobserved texels; nearest fill does not establish fidelity.
        atlas[~coverage] = atlas[indices[0][~coverage],indices[1][~coverage]]
        if k=='specular':
            roughness = np.sqrt(np.clip(1-atlas.mean(-1),.08,1))
            Image.fromarray(np.uint8(roughness*255)).save(folder/'roughness.png')
        Image.fromarray(np.uint8(np.clip(atlas,0,1)*255)).save(folder/f'{k}.png')
        outputs[k]=f'{k}.png'
    variance=np.maximum(0,diffuse_second/np.maximum(total[:,None],1e-8)-accum['diffuse']**2)
    multi=known & (contributors>=2)
    report={'size':size, 'multiViewTexels':int(multi.sum()),
            'weightedDiffuseDisagreementByteRMS':float(np.sqrt(variance[multi].mean())*255) if multi.any() else None,
            'meanContributingViews':float(contributors[known].mean()),
            'seamProtocol':'Non-overlapping UV islands; shared world-space projection at seams, nearest padding. No view-dependent texture switching.', 'coveredTexels':int(known.sum()),'surfaceTexels':len(points),
            'observedFraction':float(known.mean()),'textures':outputs,
            'roughness':'sqrt(1 - mean specular RGB), approximation',
            'unobservedTexels':'nearest observed texel fill; not recovered surface evidence'}
    (folder/'materials.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report),flush=True)


if __name__=='__main__':
    bake(pathlib.Path(sys.argv[1]))
