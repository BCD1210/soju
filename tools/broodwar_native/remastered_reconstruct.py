"""Experimental orthographic visual-hull/depth fusion from owned directional ANIMs.

Outputs are candidates, never approved model releases. Run with the private asset
venv; run remastered_blender.py next with the private Blender executable.
"""
import argparse
import json
import pathlib
import numpy as np
from scipy import ndimage
from skimage.measure import marching_cubes
from remastered_layers import source_manifest, layers, frame
from client import RUNTIME


def basis(direction, elevation):
    a = -direction * np.pi / 16
    e = np.radians(elevation)
    right = np.array([-np.cos(a), np.sin(a), 0.])
    toward = np.array([np.sin(a)*np.cos(e), np.cos(a)*np.cos(e), np.sin(e)])
    up = np.cross(toward, right)
    return right, up, toward


def project(points, direction, elevation, width, height):
    right, up, toward = basis(direction, elevation)
    return points @ right + width/2, height/2 - points @ up, points @ toward


def reconstruct(kind, image, source, resolution=160, elevation=32., group=0):
    info, atlases = layers(source, image)
    if len(info['frames']) < (group+1)*17:
        raise ValueError('A complete directional group is required; animation frames are not extra views')
    out = RUNTIME / 'models-remastered' / str(kind)
    out.mkdir(parents=True, exist_ok=True)
    references = out / 'references'
    references.mkdir(exist_ok=True)
    from PIL import Image
    views = [{k: frame(info, atlas, group*17+d) for k, atlas in atlases.items()} for d in range(17)]
    for d, view in enumerate(views):
        for name, pixels in view.items():
            Image.fromarray(pixels).save(references / f'{d:02}-{name}.png')
    width, height = info['width'], info['height']
    extent = max(width, height)*.47
    axis = np.linspace(-extent, extent, resolution, dtype=np.float32)
    step = float(axis[1]-axis[0])
    points = np.stack(np.meshgrid(axis, axis, axis, indexing='ij'), -1).reshape(-1, 3)
    hull = np.ones(len(points), dtype=bool)
    for d, view in enumerate(views):
        x, y, _ = project(points, d, elevation, width, height)
        mask = ndimage.map_coordinates(view['diffuse'][..., 3], [y, x], order=1, mode='constant') > 127
        hull &= mask
    if hull.sum() < 100:
        raise ValueError('Directional intersection is empty: verify offsets, poses and camera')
    grid = hull.reshape((resolution,)*3)
    # A second half-turn is not fabricated: only measured directions constrain the volume.
    # Closed padding guarantees no cut boundary at the volume edge.
    sdf = ndimage.distance_transform_edt(grid) - ndimage.distance_transform_edt(~grid)
    occupied = points[hull]
    fits = []
    depth_sum = np.zeros(len(occupied), dtype=np.float32)
    depth_weight = np.zeros(len(occupied), dtype=np.float32)
    zbuffers = []
    for d, view in enumerate(views):
        x, y, z = project(occupied, d, elevation, width, height)
        xi, yi = np.rint(x).astype(int), np.rint(y).astype(int)
        valid = (xi >= 0) & (xi < width) & (yi >= 0) & (yi < height)
        front = np.full((height, width), -1e6, dtype=np.float32)
        np.maximum.at(front, (yi[valid], xi[valid]), z[valid])
        front = ndimage.maximum_filter(front, size=3)
        zbuffers.append(front)
        if 'ao_depth' not in view:
            continue
        depth = view['ao_depth'][..., 3].astype(float)
        mask = (view['diffuse'][..., 3] > 240) & (front > -1e5)
        values, target = depth[mask], front[mask]
        scale, offset = np.linalg.lstsq(np.column_stack((values, np.ones(len(values)))), target, rcond=None)[0]
        residual = float(np.sqrt(np.mean((values*scale+offset-target)**2)))
        fits.append({'direction': d, 'pixelsPerDepthByte': float(scale), 'offset': float(offset), 'rmsePixels': residual})
        # Reject depth whose fitted sign contradicts the format or has poor alignment.
        if scale <= 0 or residual > 5:
            continue
        measured = ndimage.map_coordinates(depth, [y, x], order=1)*scale+offset
        signed = measured-z
        # Truncated integration only near a measured surface; unobserved interior stays solid.
        weight = (signed >= -step*3) & (signed <= step*3)
        depth_sum += np.clip(signed/step, -3, 3)*weight
        depth_weight += weight
    refined = sdf.reshape(-1).copy()
    candidate = refined[hull]
    use = depth_weight > 0
    candidate[use] = np.minimum(candidate[use], depth_sum[use]/depth_weight[use])
    refined[hull] = candidate
    refined = refined.reshape(grid.shape)
    refined[[0, -1], :, :] = -1
    refined[:, [0, -1], :] = -1
    refined[:, :, [0, -1]] = -1
    refined = ndimage.gaussian_filter(refined, .55)
    verts, faces, _, _ = marching_cubes(refined, level=0, spacing=(step,)*3, allow_degenerate=False)
    verts += axis[0]
    np.savez_compressed(out/'reconstruction.npz', vertices=verts, faces=faces, zbuffers=np.stack(zbuffers))
    config = {'type': kind, 'image': image, 'sourceManifest': str(source), 'group': group,
              'width': width, 'height': height, 'elevation': elevation, 'resolution': resolution,
              'voxelSizePixels': step, 'initialTriangles': len(faces), 'occupiedVoxels': int(hull.sum()),
              'depthFits': fits, 'usedDepthViews': sum(f['pixelsPerDepthByte'] > 0 and f['rmsePixels'] <= 5 for f in fits),
              'status': 'experimental-unvalidated', 'layers': list(atlases),
              'limitations': ['Only 17 measured views; unseen undersides are inferred.',
                             'Camera and per-view depth affine fits are provisional.',
                             'Static pose only; action/iscript mapping unresolved.']}
    (out/'reconstruction.json').write_text(json.dumps(config, indent=2)+'\n')
    (out/'provenance.json').write_text(json.dumps({'source': 'local-owned-remastered-install', 'manifest': str(source),
                                                 'image': image, 'frameGroup': group}, indent=2)+'\n')
    print(json.dumps({'type':kind,'triangles':len(faces),'depthViews':config['usedDepthViews']}), flush=True)
    return out


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--types', default='0,7')
    parser.add_argument('--source', type=pathlib.Path, default=source_manifest())
    parser.add_argument('--resolution', type=int, default=160)
    parser.add_argument('--elevation', type=float, default=32)
    args = parser.parse_args()
    for kind in map(int, args.types.split(',')):
        reconstruct(kind, {0:239, 7:247}[kind], args.source, args.resolution, args.elevation)
