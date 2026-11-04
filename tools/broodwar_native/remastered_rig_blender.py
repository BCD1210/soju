"""Extract a posed SCEvo rig, apply fitted displacements through inverse skinning."""
import argparse
import hashlib
import json
import pathlib
import sys
import bpy
import numpy as np
from mathutils import Vector

ROOT = pathlib.Path.home()/'.local/share/soju/broodwar-native'
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))


def idle(rig, name="Stand_full"):
    animation = rig.animation_data
    animation.action = None
    for track in animation.nla_tracks:
        track.mute = True
    action = bpy.data.actions[name]
    animation.action = action
    animation.action_slot = action.slots[0]
    bpy.context.scene.frame_set(0)
    bpy.context.view_layer.update()


def posed(obj):
    evaluated = obj.evaluated_get(bpy.context.evaluated_depsgraph_get())
    mesh = evaluated.to_mesh()
    vertices = np.array([evaluated.matrix_world @ v.co for v in mesh.vertices])
    evaluated.to_mesh_clear()
    return vertices


def extract(kind, pose="Stand_full", subdivisions=0):
    name = {0:'marine', 7:'scv'}[kind]
    source = ROOT/'models-3d'/f'{name}.glb'
    folder = ROOT/'models-remastered'/str(kind)/'rigged'
    folder.mkdir(parents=True, exist_ok=True)
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(source))
    rig = next(o for o in bpy.context.scene.objects if o.type == 'ARMATURE')
    obj = next(o for o in bpy.context.scene.objects if o.type == 'MESH' and o.find_armature())
    for other in list(bpy.context.scene.objects):
        if other.type == 'MESH' and other != obj:
            bpy.data.objects.remove(other, do_unlink=True)
    if subdivisions:
        bpy.context.view_layer.objects.active = obj
        modifier = obj.modifiers.new('Fitting resolution', 'SUBSURF')
        modifier.subdivision_type = 'SIMPLE'
        modifier.levels = subdivisions
        # Subdivide before the armature, retaining interpolated skin weights.
        bpy.ops.object.modifier_move_up(modifier=modifier.name)
        bpy.ops.object.modifier_apply(modifier=modifier.name)
    idle(rig, pose)
    obj.data.calc_loop_triangles()
    triangles = np.array([t.vertices[:] for t in obj.data.loop_triangles], dtype=np.int32)
    vertices = posed(obj)
    # Per-vertex linear blend skin matrix maps a rest-space displacement to world.
    transforms = {}
    for group in obj.vertex_groups:
        bone = rig.pose.bones.get(group.name)
        if bone:
            transforms[group.index] = np.array((rig.matrix_world @ bone.matrix @ bone.bone.matrix_local.inverted() @ rig.matrix_world.inverted() @ obj.matrix_world).to_3x3())
    matrices = []
    for v in obj.data.vertices:
        weights = [(g.weight, transforms[g.group]) for g in v.groups if g.group in transforms]
        total = sum(w for w,_ in weights)
        matrices.append(sum(w*m for w,m in weights)/total if total else np.array(obj.matrix_world.to_3x3()))
    np.savez_compressed(folder/'source.npz', vertices=vertices, triangles=triangles,
                        inverseSkin=np.linalg.inv(matrices), rest=np.array([v.co[:] for v in obj.data.vertices]))
    report = {'type':kind,'source':str(source),'sha256':hashlib.sha256(source.read_bytes()).hexdigest(),
              'bones':len(rig.data.bones),'vertices':len(vertices),'triangles':len(triangles),
              'clips':[a.name for a in bpy.data.actions], 'idleFrame':0, 'idleClip':pose, 'subdivisions':subdivisions,
              'coordinateSystem':'Blender Z-up; original rig units and weights retained'}
    (folder/'source.json').write_text(json.dumps(report,indent=2)+'\n')
    bpy.ops.wm.save_as_mainfile(filepath=str(folder/'source.blend'))
    print(json.dumps(report), flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('stage', choices=['extract'])
    parser.add_argument('--types', default='0,7')
    parser.add_argument('--pose', default='Stand_full')
    parser.add_argument('--subdivisions', type=int, default=0)
    args = parser.parse_args(sys.argv[sys.argv.index('--')+1:])
    for kind in map(int,args.types.split(',')):
        extract(kind, args.pose, args.subdivisions)

if __name__ == '__main__':
    main()
