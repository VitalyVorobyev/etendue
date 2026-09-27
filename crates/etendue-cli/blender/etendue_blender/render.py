"""etendue's Blender backend (ADR 0005): a thin renderer.

    $ETENDUE_BLENDER -b --factory-startup --python-exit-code 1 \\
        --python render.py -- job.json out_dir

Reads a render job written by ``etendue render`` (etendue-synth ``job``), builds
the scene once, and for every shot poses every object and renders each
requested camera: the **canonical pinhole** of ADR 0004 (square pixels,
centred principal point, no distortion), with Cycles, as multilayer linear EXR
(Combined, Depth, IndexOB). Remapping onto the calibrated camera, exposure,
noise and quantisation all happen in Rust.

Only ``bpy`` and the standard library; no camera-model math. Frame conventions
live in ``convert.py``.
"""

import json
import math
import os
import sys

import bpy
from mathutils import Matrix

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import convert  # noqa: E402

SENSOR_WIDTH_MM = 36.0
JOB_VERSION = 1


def mat(m):
    return Matrix(m)


def material(name, rgb, roughness=0.5):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    bsdf = m.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = roughness
    return m


def setup_render(job, out_dir):
    sc = bpy.context.scene
    r = job["render"]
    sc.render.engine = "CYCLES"
    device = r.get("device", "gpu")
    if device == "gpu":
        prefs = bpy.context.preferences.addons["cycles"].preferences
        for kind in ("METAL", "OPTIX", "CUDA", "HIP", "ONEAPI"):
            try:
                prefs.compute_device_type = kind
            except TypeError:
                continue
            prefs.get_devices()
            gpus = [d for d in prefs.devices if d.type != "CPU"]
            if gpus:
                for d in prefs.devices:
                    d.use = d.type != "CPU"
                sc.cycles.device = "GPU"
                break
        else:
            sc.cycles.device = "CPU"
    else:
        sc.cycles.device = "CPU"
    sc.cycles.samples = int(r["samples"])
    sc.cycles.seed = int(r["seed"])
    sc.cycles.use_denoising = False
    sc.cycles.use_animated_seed = False
    sc.render.film_transparent = False
    sc.render.resolution_percentage = 100
    sc.render.pixel_aspect_x = sc.render.pixel_aspect_y = 1.0
    # Linear radiance out: no view transform, no look, no exposure (ADR 0005).
    sc.view_settings.view_transform = "Standard"
    sc.view_settings.look = "None"
    sc.view_settings.exposure = 0.0
    sc.view_settings.gamma = 1.0
    s = sc.render.image_settings
    s.media_type = "MULTI_LAYER_IMAGE"
    s.file_format = "OPEN_EXR_MULTILAYER"
    s.color_depth = "32"
    s.exr_codec = "ZIP"
    layer = sc.view_layers[0]
    layer.use_pass_z = True
    layer.use_pass_object_index = True
    # World: the job's ambient radiance (uniform white environment), 0 for none.
    world = bpy.data.worlds.new("etendue")
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (1.0, 1.0, 1.0, 1.0)
    bg.inputs["Strength"].default_value = float(r.get("ambient", 0.0))
    sc.world = world
    return sc


def import_mesh(entry, robot_material):
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=entry["path"])
    new = [o for o in bpy.data.objects if o not in before]
    undo = mat(convert.GLTF_IMPORT_UNDO)
    root = bpy.data.objects.new(entry["id"], None)
    bpy.context.scene.collection.objects.link(root)
    for o in new:
        if o.type == "MESH":
            # Bake the importer's object transform, then undo its Y-up rotation.
            o.data.transform(undo @ o.matrix_world)
            o.matrix_world = Matrix.Identity(4)
            o.data.materials.clear()
            o.data.materials.append(robot_material)
            o.pass_index = int(entry.get("pass_index", 0))
            o.parent = root
        else:
            bpy.data.objects.remove(o, do_unlink=True)
    return root


def board(entry, light_mat, dark_mat):
    """A width × height board centred on its frame's origin in z = 0, facing +Z; columns
    along X, the dark square at −X/−Y (the layout etendue's viewers draw)."""
    import bmesh

    w, h = entry["width"], entry["height"]
    checker = entry.get("checker")
    cols, rows = (checker["cols"], checker["rows"]) if checker else (1, 1)
    bm = bmesh.new()
    for r in range(rows):
        for c in range(cols):
            x0, y0 = -w / 2 + c * w / cols, -h / 2 + r * h / rows
            x1, y1 = x0 + w / cols, y0 + h / rows
            verts = [bm.verts.new(p) for p in ((x0, y0, 0), (x1, y0, 0), (x1, y1, 0), (x0, y1, 0))]
            face = bm.faces.new(verts)
            face.material_index = 1 if checker and (r + c) % 2 == 0 else 0
    mesh = bpy.data.meshes.new(entry["id"])
    bm.to_mesh(mesh)
    bm.free()
    mesh.materials.append(light_mat)
    mesh.materials.append(dark_mat)
    obj = bpy.data.objects.new(entry["id"], mesh)
    obj.pass_index = int(entry.get("pass_index", 0))
    bpy.context.scene.collection.objects.link(obj)
    return obj


def light(entry):
    shape = entry["shape"]
    kind = {"point": "POINT", "spot": "SPOT", "area": "AREA"}[shape["type"]]
    data = bpy.data.lights.new(entry["id"], kind)
    data.energy = float(entry["power_w"])
    data.color = tuple(entry["color"])
    if kind == "POINT":
        data.shadow_soft_size = float(shape.get("radius_m", 0.0))
    elif kind == "SPOT":
        data.spot_size = float(shape["cone_angle"])
        data.spot_blend = float(shape.get("blend", 0.0))
        data.shadow_soft_size = 0.0
    else:
        data.shape = "RECTANGLE"
        data.size, data.size_y = map(float, shape["size_m"])
    obj = bpy.data.objects.new(entry["id"], data)
    bpy.context.scene.collection.objects.link(obj)
    return obj, kind != "POINT"


def camera(entry, clip):
    data = bpy.data.cameras.new(entry["id"])
    data.type = "PERSP"
    data.sensor_fit = "HORIZONTAL"
    data.sensor_width = SENSOR_WIDTH_MM
    data.lens = convert.lens_mm(entry["focal_px"], entry["width"], SENSOR_WIDTH_MM)
    data.shift_x = data.shift_y = 0.0
    data.clip_start, data.clip_end = clip
    data.dof.use_dof = False
    obj = bpy.data.objects.new(entry["id"], data)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def main():
    argv = sys.argv[sys.argv.index("--") + 1 :]
    job_path, out_dir = argv[0], argv[1]
    with open(job_path) as f:
        job = json.load(f)
    if job.get("version") != JOB_VERSION:
        raise SystemExit(f"unsupported job version {job.get('version')} (this script reads {JOB_VERSION})")

    bpy.ops.wm.read_factory_settings(use_empty=True)
    sc = setup_render(job, out_dir)
    robot_mat = material("robot", (0.35, 0.36, 0.38), 0.45)
    light_mat = material("board_light", (0.85, 0.85, 0.85), 0.6)
    dark_mat = material("board_dark", (0.03, 0.03, 0.03), 0.6)

    posed = []  # (object, frame, needs_emitter_flip)
    for m in job.get("meshes", []):
        posed.append((import_mesh(m, robot_mat), m["frame"], False))
    for b in job.get("boards", []):
        posed.append((board(b, light_mat, dark_mat), b["frame"], False))
    for l in job.get("lights", []):
        obj, flip = light(l)
        posed.append((obj, l["frame"], flip))
    clip = tuple(job["render"].get("clip", (0.01, 50.0)))
    cams = {c["id"]: (camera(c, clip), c) for c in job["cameras"]}

    for shot in job["shots"]:
        poses = {name: convert.from_row_major(m) for name, m in shot["poses"].items()}
        for obj, frame, flip in posed:
            pose = convert.emitter_matrix(poses[frame]) if flip else poses[frame]
            obj.matrix_world = mat(pose)
        for out in shot["outputs"]:
            obj, spec = cams[out["camera"]]
            obj.matrix_world = mat(convert.camera_matrix(poses[spec["frame"]]))
            sc.camera = obj
            sc.render.resolution_x, sc.render.resolution_y = spec["width"], spec["height"]
            sc.render.filepath = os.path.join(out_dir, out["path"])
            bpy.ops.render.render(write_still=True)
            print(f"etendue: rendered {out['path']}", flush=True)


if __name__ == "__main__":
    main()
