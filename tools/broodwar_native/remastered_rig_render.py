"""Build and independently render fitted skinned assets using private Blender."""
import argparse
import json
import pathlib
import subprocess
import sys
import bpy
import bmesh
import numpy as np
from mathutils import Vector
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from remastered_rig_blender import ROOT, HERE, idle, posed
from remastered_blender import image_node


def build(kind,label,variant="rigged"):
    root=ROOT/'models-remastered'/str(kind)/variant
    folder=root/label;folder.mkdir(exist_ok=True)
    refs=folder/'references'
    if not refs.exists():refs.symlink_to(root/'references',target_is_directory=True)
    source=json.loads((root/'source.json').read_text())
    camera=json.loads((root/'camera.json').read_text())['camera']
    bpy.ops.wm.open_mainfile(filepath=str(root/'source.blend'))
    obj=next(o for o in bpy.context.scene.objects if o.type=='MESH' and o.find_armature())
    rig=obj.find_armature();idle(rig,source.get('idleClip','Stand_full'))
    data=np.load(root/'source.npz')
    fitted=data['vertices'] if label=='baseline' else np.load(root/f'{label}.npz')['vertices']
    displacement=fitted-data['vertices']
    rest=data['rest']+np.einsum('nij,nj->ni',data['inverseSkin'],displacement)
    for vertex,co in zip(obj.data.vertices,rest):vertex.co=co
    obj.data.update();bpy.context.view_layer.update()
    error=float(np.abs(posed(obj)-fitted).max())
    if error>1e-4:raise ValueError(f'Inverse skinning residual {error}; fitted geometry not reproduced')
    bpy.ops.object.select_all(action='DESELECT');obj.select_set(True);bpy.context.view_layer.objects.active=obj
    # SCEvo UVs overlap/mirror. Repack a non-overlapping atlas without changing skin topology.
    bpy.ops.object.mode_set(mode='EDIT');bpy.ops.mesh.select_all(action='SELECT')
    bpy.ops.uv.smart_project(angle_limit=np.radians(66),island_margin=.004)
    bpy.ops.object.mode_set(mode='OBJECT')
    obj.data.calc_loop_triangles()
    evaluated=obj.evaluated_get(bpy.context.evaluated_depsgraph_get());mesh=evaluated.to_mesh();mesh.calc_loop_triangles()
    vertices=np.array([evaluated.matrix_world@v.co for v in mesh.vertices])
    normal_matrix=evaluated.matrix_world.to_3x3().inverted().transposed()
    normals=np.array([(normal_matrix@v.normal).normalized() for v in mesh.vertices])
    triangles=np.array([t.vertices[:] for t in mesh.loop_triangles])
    uv=np.array([[obj.data.uv_layers.active.data[l].uv[:] for l in t.loops] for t in obj.data.loop_triangles])
    evaluated.to_mesh_clear()
    np.savez_compressed(folder/'uv-mesh.npz',vertices=vertices,triangles=triangles,uv=uv,normals=normals)
    config={'type':kind,'width':camera['width'],'height':camera['height'],'camera':camera,
            'elevation':camera['elevation'],'layers':[p.name[3:-4] for p in (root/'references').glob('00-*.png')],
            'viewCount':17,'visibilityTolerancePixels':1.5,
            'limitations':['Fitted SCEvo topology and source animation; not recovered Remastered animation.',
                            'Camera/pose/shape are ambiguous with different source art.',
                            'Bright/normal projection is approximate; diffuse gate uses unlit prelit source.',
                            'Unobserved undersides are filled, not reconstructed.']}
    (folder/'reconstruction.json').write_text(json.dumps(config,indent=2)+'\n')
    (folder/'provenance.json').write_text(json.dumps({'rig':source,'layers':str(root/'references'),'fit':label},indent=2)+'\n')
    subprocess.run([str(ROOT/'asset-venv/bin/python'),str(HERE/'remastered_rig_material.py'),str(folder)],check=True)
    obj.data.materials.clear()
    mat=bpy.data.materials.new('OwnedRemasteredLayers');mat.use_nodes=True;obj.data.materials.append(mat)
    for p in obj.data.polygons:p.material_index=0
    nodes=mat.node_tree.nodes;links=mat.node_tree.links;shader=nodes.get('Principled BSDF')
    bright=image_node(nodes,folder/'bright.png');links.new(bright.outputs['Color'],shader.inputs['Base Color'])
    rough=image_node(nodes,folder/'roughness.png',True);links.new(rough.outputs['Color'],shader.inputs['Roughness'])
    normal=image_node(nodes,folder/'normal.png',True);nm=nodes.new('ShaderNodeNormalMap')
    links.new(normal.outputs['Color'],nm.inputs['Color']);links.new(nm.outputs['Normal'],shader.inputs['Normal'])
    if (folder/'emissive.png').exists():
        em=image_node(nodes,folder/'emissive.png');links.new(em.outputs['Color'],shader.inputs['Emission Color']);shader.inputs['Emission Strength'].default_value=1
    for p in obj.data.polygons:p.use_smooth=True
    rig.select_set(True)
    bpy.ops.export_scene.gltf(filepath=str(folder/'model.glb'),export_format='GLB',use_selection=True,
                              export_animations=True,export_animation_mode='NLA_TRACKS')
    idle(rig,source.get('idleClip','Stand_full'))
    # Report source topology honestly: split material/UV boundaries need not be manifold.
    bm=bmesh.new();bm.from_mesh(obj.data)
    bmesh.ops.remove_doubles(bm,verts=list(bm.verts),dist=1e-6)
    nonmanifold=sum(not e.is_manifold for e in bm.edges);bm.free()
    animations=['idle','walk','attack']+(['work'] if 'Work_full' in source['clips'] else [])
    if 'Death_Retarget_full' in bpy.data.actions:animations.append('death')
    (folder/'geometry.json').write_text(json.dumps({'triangles':len(triangles),'vertices':len(vertices),'bones':source['bones'],
        'nonManifoldEdges':nonmanifold,'watertight':nonmanifold==0,'triangleBudgetPassed':15000<=len(triangles)<=40000,
        'animations':animations,'clips':[a.name for a in bpy.data.actions], 'inverseSkinMaximumError':error,
        'skinWeights':'preserved from source; subdivided weights interpolated if configured'},indent=2)+'\n')
    scene=bpy.context.scene;scene.render.engine='CYCLES';scene.cycles.samples=8
    scene.render.resolution_x=camera['width'];scene.render.resolution_y=camera['height'];scene.render.resolution_percentage=100
    scene.render.film_transparent=True;scene.view_settings.view_transform='Standard';scene.view_settings.look='None'
    scene.world=bpy.data.worlds.new('BlackWorld');scene.world.color=(0,0,0)
    cam_data=bpy.data.cameras.new('FittedSpriteCamera');cam=bpy.data.objects.new('FittedSpriteCamera',cam_data)
    bpy.context.collection.objects.link(cam);scene.camera=cam;cam_data.type='ORTHO'
    cam_data.ortho_scale=max(camera['width'],camera['height'])/camera['pixelsPerUnit'];cam_data.clip_end=100
    diffuse=image_node(nodes,folder/'diffuse.png');em=nodes.new('ShaderNodeEmission')
    links.new(diffuse.outputs['Color'],em.inputs['Color']);links.new(em.outputs[0],nodes.get('Material Output').inputs['Surface'])
    out=folder/'renders';out.mkdir(exist_ok=True)
    target=Vector(camera['target'])
    for d,yaw in enumerate(camera['yaws']):
        a,e=np.radians([yaw,camera['elevation']])
        toward=Vector((np.sin(a)*np.cos(e),np.cos(a)*np.cos(e),np.sin(e)))
        cam.location=target+toward*10;cam.rotation_euler=(-toward).to_track_quat('-Z','Y').to_euler()
        scene.render.filepath=str(out/f'{d:02}.png');bpy.ops.render.render(write_still=True)
    links.new(shader.outputs[0],nodes.get('Material Output').inputs['Surface'])
    bpy.ops.wm.save_as_mainfile(filepath=str(folder/'candidate.blend'))

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--types',default='0,7');p.add_argument('--label',default='fit')
    p.add_argument('--variant',default='rigged')
    a=p.parse_args(sys.argv[sys.argv.index('--')+1:])
    for kind in map(int,a.types.split(',')):build(kind,a.label,a.variant)
