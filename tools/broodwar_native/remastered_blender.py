"""Private Blender: condition geometry, UV, bake owned layers, export and render."""
import bpy
import bmesh
import json
import pathlib
import subprocess
import sys
import numpy as np
from mathutils import Vector

HERE=pathlib.Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
# Keep Blender independent of the private venv's SciPy ABI.
from math import sin,cos,pi,radians


def basis(d,elevation):
    a=-d*pi/16;e=radians(elevation)
    r=Vector((-cos(a),sin(a),0))
    t=Vector((sin(a)*cos(e),cos(a)*cos(e),sin(e)))
    return r,t.cross(r),t


def image_node(nodes,path,noncolor=False):
    node=nodes.new('ShaderNodeTexImage');node.image=bpy.data.images.load(str(path),check_existing=True)
    if noncolor:node.image.colorspace_settings.name='Non-Color'
    return node


def run(folder):
    config=json.loads((folder/'reconstruction.json').read_text())
    bpy.ops.object.select_all(action='SELECT');bpy.ops.object.delete(use_global=False)
    data=np.load(folder/'reconstruction.npz')
    mesh=bpy.data.meshes.new('RemasteredSurface');mesh.from_pydata(data['vertices'].tolist(),[],data['faces'].tolist());mesh.update()
    obj=bpy.data.objects.new('RemasteredCandidate',mesh);bpy.context.collection.objects.link(obj)
    bpy.context.view_layer.objects.active=obj;obj.select_set(True)
    bm=bmesh.new();bm.from_mesh(mesh);bmesh.ops.recalc_face_normals(bm,faces=bm.faces);bm.to_mesh(mesh);bm.free()
    smooth=obj.modifiers.new('Voxel edge cleanup','SMOOTH');smooth.factor=.35;smooth.iterations=2
    bpy.ops.object.modifier_apply(modifier=smooth.name)
    if len(mesh.polygons)>30000:
        dec=obj.modifiers.new('Triangle budget','DECIMATE');dec.ratio=30000/len(mesh.polygons)
        bpy.ops.object.modifier_apply(modifier=dec.name)
    bm=bmesh.new();bm.from_mesh(obj.data)
    # Decimation can collapse tiny detached voxel islands to isolated triangles.
    bmesh.ops.delete(bm,geom=[f for f in bm.faces if all(e.is_boundary for e in f.edges)],context='FACES')
    bmesh.ops.holes_fill(bm,edges=[e for e in bm.edges if e.is_boundary],sides=0)
    bmesh.ops.triangulate(bm,faces=list(bm.faces))
    bmesh.ops.delete(bm,geom=[f for f in bm.faces if all(e.is_boundary for e in f.edges)],context='FACES')
    bmesh.ops.recalc_face_normals(bm,faces=list(bm.faces));bm.to_mesh(obj.data);bm.free()
    bpy.ops.object.mode_set(mode='EDIT');bpy.ops.mesh.select_all(action='SELECT')
    bpy.ops.uv.smart_project(angle_limit=radians(66),island_margin=.004)
    bpy.ops.object.mode_set(mode='OBJECT')
    mesh=obj.data
    bm=bmesh.new();bm.from_mesh(mesh)
    bmesh.ops.delete(bm,geom=[f for f in bm.faces if all(e.is_boundary for e in f.edges)],context='FACES')
    bm.to_mesh(mesh);bm.free();mesh.update();mesh.calc_loop_triangles()
    for p in mesh.polygons:p.use_smooth=True
    triangles=np.array([t.vertices[:] for t in mesh.loop_triangles])
    uv=np.array([[mesh.uv_layers.active.data[l].uv[:] for l in t.loops] for t in mesh.loop_triangles])
    np.savez_compressed(folder/'uv-mesh.npz',vertices=np.array([v.co[:] for v in mesh.vertices]),triangles=triangles,
                        uv=uv,normals=np.array([v.normal[:] for v in mesh.vertices]))
    python=pathlib.Path.home()/'.local/share/soju/broodwar-native/asset-venv/bin/python'
    subprocess.run([str(python),str(HERE/'remastered_texture.py'),str(folder)],check=True)
    mat=bpy.data.materials.new('RemasteredOwnedLayers');mat.use_nodes=True;obj.data.materials.append(mat)
    nodes=mat.node_tree.nodes;links=mat.node_tree.links;shader=nodes.get('Principled BSDF')
    bright=image_node(nodes,folder/'bright.png');links.new(bright.outputs['Color'],shader.inputs['Base Color'])
    rough=image_node(nodes,folder/'roughness.png',True);links.new(rough.outputs['Color'],shader.inputs['Roughness'])
    normal=image_node(nodes,folder/'normal.png',True);nm=nodes.new('ShaderNodeNormalMap');links.new(normal.outputs['Color'],nm.inputs['Color']);links.new(nm.outputs['Normal'],shader.inputs['Normal'])
    if (folder/'emissive.png').exists():
        emission=image_node(nodes,folder/'emissive.png');links.new(emission.outputs['Color'],shader.inputs['Emission Color']);shader.inputs['Emission Strength'].default_value=1
    bpy.ops.export_scene.gltf(filepath=str(folder/'model.glb'),export_format='GLB',use_selection=True,export_animations=True)
    lod=obj.copy();lod.data=obj.data.copy();bpy.context.collection.objects.link(lod);obj.select_set(False);lod.select_set(True);bpy.context.view_layer.objects.active=lod
    dec=lod.modifiers.new('LOD1','DECIMATE');dec.ratio=min(1,4000/len(lod.data.polygons));bpy.ops.object.modifier_apply(modifier=dec.name)
    bpy.ops.export_scene.gltf(filepath=str(folder/'lod1.glb'),export_format='GLB',use_selection=True)
    lod_count=len(lod.data.polygons);bpy.data.objects.remove(lod,do_unlink=True);obj.select_set(True);bpy.context.view_layer.objects.active=obj
    bm=bmesh.new();bm.from_mesh(mesh);boundary=sum(not e.is_manifold for e in bm.edges);bm.free()
    (folder/'geometry.json').write_text(json.dumps({'triangles':len(triangles),'lod1Triangles':lod_count,'nonManifoldEdges':boundary,
        'watertight':boundary==0,'animations':[],'triangleBudgetPassed':15000<=len(triangles)<=40000},indent=2)+'\n')
    scene=bpy.context.scene;scene.render.engine='CYCLES';scene.cycles.samples=8
    scene.render.resolution_x=config['width'];scene.render.resolution_y=config['height'];scene.render.resolution_percentage=100
    scene.render.film_transparent=True;scene.view_settings.view_transform='Standard';scene.view_settings.look='None'
    scene.world.color=(.15,.15,.15)
    camera_data=bpy.data.cameras.new('MeasuredSpriteCamera');camera=bpy.data.objects.new('MeasuredSpriteCamera',camera_data);bpy.context.collection.objects.link(camera);scene.camera=camera
    camera_data.type='ORTHO';camera_data.ortho_scale=max(config['width'],config['height']);camera_data.clip_end=2000
    # Blender ortho scale spans the wider image dimension.
    diffuse=image_node(nodes,folder/'diffuse.png');em=nodes.new('ShaderNodeEmission');links.new(diffuse.outputs['Color'],em.inputs['Color']);links.new(em.outputs[0],nodes.get('Material Output').inputs['Surface'])
    render_dir=folder/'renders';render_dir.mkdir(exist_ok=True)
    for d in range(17):
        _,_,toward=basis(d,config['elevation']);camera.location=toward*600;camera.rotation_euler=(-toward).to_track_quat('-Z','Y').to_euler()
        scene.render.filepath=str(render_dir/f'{d:02}.png');bpy.ops.render.render(write_still=True)
    links.new(shader.outputs[0],nodes.get('Material Output').inputs['Surface'])
    bpy.ops.wm.save_as_mainfile(filepath=str(folder/'candidate.blend'))


if __name__=='__main__':
    for arg in sys.argv[sys.argv.index('--')+1:]:run(pathlib.Path(arg))
