# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
# Run through the private Blender executable; assets_review supplies the job file.
"""Inspect and render a delivery without executing its Blender source file."""
import json
import math
import pathlib
import sys

import bpy
from mathutils import Vector


def render(job_path: pathlib.Path) -> None:
    job = json.loads(job_path.read_text())
    out = job_path.parent
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=job['model'])
    scene = bpy.context.scene
    for rig in [o for o in scene.objects if o.type == 'ARMATURE']:
        animation = rig.animation_data
        if animation:
            animation.action = None
            for track in animation.nla_tracks:
                track.mute = True
            action = bpy.data.actions.get(job['idle'])
            if action:
                animation.action = action
                if action.slots:
                    animation.action_slot = action.slots[0]
    scene.frame_set(0)
    bpy.context.view_layer.update()
    meshes = [o for o in scene.objects if o.type == 'MESH']
    vertices = []
    triangles = 0
    for obj in meshes:
        evaluated = obj.evaluated_get(bpy.context.evaluated_depsgraph_get())
        mesh = evaluated.to_mesh()
        mesh.calc_loop_triangles()
        triangles += len(mesh.loop_triangles)
        vertices.extend(evaluated.matrix_world @ v.co for v in mesh.vertices)
        evaluated.to_mesh_clear()
    low = [min(v[i] for v in vertices) for i in range(3)]
    high = [max(v[i] for v in vertices) for i in range(3)]
    bones = [b.name for o in scene.objects if o.type == 'ARMATURE' for b in o.data.bones]
    report = {'triangles': triangles, 'blenderBounds': [low, high], 'bones': bones,
              'clips': [a.name for a in bpy.data.actions],
              'textures': [{'name': i.name, 'size': list(i.size)} for i in bpy.data.images if i.source != 'GENERATED']}
    (out/'geometry.json').write_text(json.dumps(report, indent=2)+'\n')
    camera = job['camera']
    scene.render.engine = 'CYCLES'
    scene.cycles.samples = 8
    scene.render.film_transparent = True
    scene.view_settings.view_transform = 'Standard'
    scene.view_settings.look = 'None'
    scene.world = bpy.data.worlds.new('Review world')
    scene.world.color = (.12, .12, .12)
    cam_data = bpy.data.cameras.new('Review camera')
    cam = bpy.data.objects.new('Review camera', cam_data)
    scene.collection.objects.link(cam)
    scene.camera = cam
    # Close-ups retain authored geometry and materials, before calibration placement.
    light_data = bpy.data.lights.new('Review key', 'AREA')
    light_data.energy = 200
    light_data.size = 4
    light = bpy.data.objects.new('Review key', light_data)
    scene.collection.objects.link(light)
    center = Vector([(a+b)/2 for a,b in zip(low, high)])
    size = max(b-a for a,b in zip(low, high))
    light.location = center + Vector((size*2, -size*2, size*3))
    light.rotation_euler = (center-light.location).to_track_quat('-Z','Y').to_euler()
    cam_data.type = 'PERSP'
    cam_data.lens = 50
    scene.render.resolution_x = scene.render.resolution_y = 640
    scene.render.resolution_percentage = 100
    turntable = out/'turntable'
    turntable.mkdir()
    for index in range(36):
        angle = index*math.tau/36
        cam.location = center + Vector((math.sin(angle)*size*2, math.cos(angle)*size*2, size*.35))
        cam.rotation_euler = (center-cam.location).to_track_quat('-Z','Y').to_euler()
        scene.render.filepath = str(turntable/f'{index:02}.png')
        bpy.ops.render.render(write_still=True)
    light.hide_render = True
    # One placement for all directions; never register individual images.
    factor = job['referenceHeight']/(high[2]-low[2])
    roots = [o for o in scene.objects if o.parent is None and o not in (cam, light)]
    for obj in roots:
        obj.location *= factor
        obj.location.z += job['referenceFoot']-low[2]*factor
        obj.scale *= factor
    for material in bpy.data.materials:
        if not material.use_nodes:
            continue
        nodes = material.node_tree.nodes
        links = material.node_tree.links
        shader = next((n for n in nodes if n.type == 'BSDF_PRINCIPLED'), None)
        output = next((n for n in nodes if n.type == 'OUTPUT_MATERIAL'), None)
        if shader and output:
            emission = nodes.new('ShaderNodeEmission')
            base = shader.inputs['Base Color']
            if base.is_linked:
                links.new(base.links[0].from_socket, emission.inputs['Color'])
            else:
                emission.inputs['Color'].default_value = base.default_value
            links.new(emission.outputs[0], output.inputs['Surface'])
    scene.render.resolution_x = camera['width']
    scene.render.resolution_y = camera['height']
    cam_data.type = 'ORTHO'
    cam_data.ortho_scale = max(camera['width'], camera['height'])/camera['pixelsPerUnit']
    target = Vector(camera['target'])
    renders = out/'renders'
    renders.mkdir()
    for index, yaw in enumerate(camera['yaws']):
        a, e = math.radians(yaw), math.radians(camera['elevation'])
        toward = Vector((math.sin(a)*math.cos(e),math.cos(a)*math.cos(e),math.sin(e)))
        cam.location = target+toward*10
        cam.rotation_euler = (-toward).to_track_quat('-Z','Y').to_euler()
        scene.render.filepath = str(renders/f'{index:02}.png')
        bpy.ops.render.render(write_still=True)


if __name__ == '__main__':
    render(pathlib.Path(sys.argv[sys.argv.index('--')+1]))
