"""Retarget locally owned SCEvo death-bone motion onto the fitted source rig.

This preserves base-mesh topology, not detached debris or particle death effects.
The first source death pose is aligned to the target idle pose by named bones.
"""
import argparse
import hashlib
import json
import pathlib
import sys
import bpy
import numpy as np
from mathutils import Matrix
sys.path.insert(0,str(pathlib.Path(__file__).resolve().parent))
from remastered_rig_blender import ROOT, idle, posed


def retarget(kind,variant):
    folder=ROOT/'models-remastered'/str(kind)/variant
    metadata=json.loads((folder/'source.json').read_text())
    name={0:'marine',7:'scv'}[kind]
    source=ROOT/'models-source'/f'{name}--death.glb'
    bpy.ops.wm.open_mainfile(filepath=str(folder/'source.blend'))
    rig=next(o for o in bpy.context.scene.objects if o.type=='ARMATURE')
    for track in list(rig.animation_data.nla_tracks):
        if track.name=='Death_Retarget_full':rig.animation_data.nla_tracks.remove(track)
    rig.animation_data.action=None
    old=bpy.data.actions.get('Death_Retarget_full')
    if old is not None:bpy.data.actions.remove(old)
    metadata['clips']=[n for n in metadata['clips'] if n!='Death_Retarget_full']
    idle(rig,metadata.get('idleClip','Stand_full'))
    mesh=next(o for o in bpy.context.scene.objects if o.type=='MESH' and o.find_armature()==rig)
    initial_vertices=posed(mesh)
    start={b.name:b.matrix.copy() for b in rig.pose.bones}
    basis={b.name:b.matrix_basis.copy() for b in rig.pose.bones}
    before=set(bpy.context.scene.objects)
    bpy.ops.import_scene.gltf(filepath=str(source))
    added=set(bpy.context.scene.objects)-before
    death=next(o for o in added if o.type=='ARMATURE')
    clip='Death_full' if kind==0 else 'Death 03_full'
    idle(death,clip)
    first={b.name:b.matrix.copy() for b in death.pose.bones}
    action=death.animation_data.action
    end=int(np.ceil(action.frame_range[1]));frames=range(end+1)
    common=set(start)&set(first)
    motion=[]
    for f in frames:
        bpy.context.scene.frame_set(f);bpy.context.view_layer.update()
        motion.append({name:death.pose.bones[name].matrix@first[name].inverted()@start[name] for name in common})
    for obj in added:bpy.data.objects.remove(obj,do_unlink=True)
    # Imported actions have their own slots; only the sampled named-bone motion is retained.
    for a in list(bpy.data.actions):
        if a.name not in metadata['clips'] and a.users==0:bpy.data.actions.remove(a)
    rig.animation_data.action=None
    for t in rig.animation_data.nla_tracks:t.mute=True
    result=bpy.data.actions.new('Death_Retarget_full');rig.animation_data.action=result
    ordered=sorted(rig.pose.bones,key=lambda b:len(b.parent_recursive))
    for f,matrices in enumerate(motion):
        bpy.context.scene.frame_set(f)
        for bone in ordered:
            bone.matrix_basis=basis[bone.name]
            if bone.name in matrices:bone.matrix=matrices[bone.name]
            bone.rotation_mode='QUATERNION'
            bone.keyframe_insert(data_path='location',frame=f,group=bone.name)
            bone.keyframe_insert(data_path='rotation_quaternion',frame=f,group=bone.name)
            bone.keyframe_insert(data_path='scale',frame=f,group=bone.name)
    track=rig.animation_data.nla_tracks.new();track.name='Death_Retarget_full'
    strip=track.strips.new('Death_Retarget_full',0,result);strip.action_slot=result.slots[0];track.mute=True
    rig.animation_data.action=None
    idle(rig,'Death_Retarget_full')
    alignment_error=float(np.max(np.abs(posed(mesh)-initial_vertices)))
    if alignment_error>1e-4:raise ValueError(f'Death initial-pose alignment failed: {alignment_error}')
    idle(rig,metadata.get('idleClip','Stand_full'))
    bpy.ops.wm.save_as_mainfile(filepath=str(folder/'source.blend'))
    metadata['clips']=[a.name for a in bpy.data.actions if a.name in metadata['clips'] or a==result]
    (folder/'source.json').write_text(json.dumps(metadata,indent=2)+'\n')
    report={'source':str(source),'sourceSha256':hashlib.sha256(source.read_bytes()).hexdigest(),
            'initialPoseMaximumError':alignment_error,
            'sourceClip':clip,'targetClip':result.name,'frames':end+1,'fps':bpy.context.scene.render.fps,
            'matchedBones':sorted(common),'matchedCount':len(common),'targetBones':len(start),
            'unmatchedBones':sorted(set(start)-common),
            'maximumMatrixChange':max(float(np.abs(np.array(m[n])-np.array(start[n])).max()) for m in motion for n in common),
            'limitations':['Named-bone motion transfer, not reconstructed Remastered death.',
                          'Unmatched target appendages follow parents; source debris and particle effects omitted.',
                          'Live playback and deformation require separate renderer verification.']}
    (folder/'death-retarget.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report),flush=True)

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--types',default='0,7');p.add_argument('--variant',default='rigged-dense')
    a=p.parse_args(sys.argv[sys.argv.index('--')+1:])
    for kind in map(int,a.types.split(',')):retarget(kind,a.variant)
