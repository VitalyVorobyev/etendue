"""Generates tests/data/blender_probe.exr (Blender 5.1.1, Cycles, Metal):

    Blender -b --factory-startup --python-exit-code 1 --python blender_probe.py -- <out_dir>

A 64 × 48 render from a camera at the origin looking down −Z (+Y up) of a red
emissive 1 m square centred at (−0.6, 0.45, −2): left of and above the optical
axis, so it lands in the image's top-left quadrant.
"""
import bpy, json, sys, mathutils
out = sys.argv[sys.argv.index("--") + 1]
bpy.ops.wm.read_factory_settings(use_empty=True)
sc = bpy.context.scene
sc.render.engine = "CYCLES"
prefs = bpy.context.preferences.addons["cycles"].preferences
prefs.compute_device_type = "METAL"
prefs.get_devices()
for d in prefs.devices: d.use = True
sc.cycles.device = "GPU"
sc.cycles.samples = 4
sc.render.resolution_x, sc.render.resolution_y, sc.render.resolution_percentage = 64, 48, 100
# a red emissive square in the top-left quadrant of the view (camera at origin looking -Z, +Y up)
bpy.ops.mesh.primitive_plane_add(size=1, location=(-0.6, 0.45, -2))
mat = bpy.data.materials.new("red"); mat.use_nodes = True
nt = mat.node_tree; nt.nodes.clear()
em = nt.nodes.new("ShaderNodeEmission"); em.inputs[0].default_value = (1, 0, 0, 1)
o = nt.nodes.new("ShaderNodeOutputMaterial"); nt.links.new(em.outputs[0], o.inputs[0])
bpy.context.object.data.materials.append(mat)
cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam")); sc.collection.objects.link(cam); sc.camera = cam
sc.view_settings.view_transform = "Standard"
sc.render.image_settings.media_type = "MULTI_LAYER_IMAGE"; sc.render.image_settings.file_format = "OPEN_EXR_MULTILAYER"
sc.render.image_settings.color_depth = "32"
sc.view_layers[0].use_pass_z = True
sc.view_layers[0].use_pass_object_index = True
sc.render.filepath = out + "/probe.exr"
bpy.ops.render.render(write_still=True)
