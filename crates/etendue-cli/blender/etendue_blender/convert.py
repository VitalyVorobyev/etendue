"""Frame conventions between etendue and Blender: the one place they live (PLAN §3).

Pure Python (no ``bpy``), so the conversions are unit-tested outside Blender
(``python3 -m unittest discover -s blender/tests``).

- etendue world: right-handed, +Z up, metres; poses arrive as row-major 4×4
  ``world_se3_frame`` matrices. Blender's world is the same.
- Camera frames are OpenCV (+Z forward, +Y down). A Blender camera looks down
  its −Z with +Y up: the two differ by Rx(π) — the same relation as three.js.
- Lights: etendue directional lights emit along local +Z; Blender spot and
  area lights emit along local −Z: Rx(π) again.
- Robot link meshes are GLBs in raw link-frame coordinates (Z up, no glTF
  Y-up conversion, tools/robot-assets). Blender's glTF importer still applies
  its Y-up → Z-up rotation, Rx(+π/2), baked into the vertices;
  ``GLTF_IMPORT_UNDO`` = Rx(−π/2) restores the raw coordinates.
"""

import math

Matrix4 = list  # 4 rows of 4 floats, row-major


def identity():
    return [[1.0 if i == j else 0.0 for j in range(4)] for i in range(4)]


def matmul(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(4)) for j in range(4)] for i in range(4)]


def rot_x(angle):
    c, s = math.cos(angle), math.sin(angle)
    return [[1.0, 0.0, 0.0, 0.0], [0.0, c, -s, 0.0], [0.0, s, c, 0.0], [0.0, 0.0, 0.0, 1.0]]


def from_row_major(flat):
    """A 4×4 matrix from 16 row-major numbers."""
    if len(flat) != 16:
        raise ValueError(f"a 4x4 matrix needs 16 numbers, got {len(flat)}")
    return [list(map(float, flat[4 * i : 4 * i + 4])) for i in range(4)]


# cv_se3_blender: rotates Blender camera / light axes (−Z forward, +Y up) onto
# CV axes (+Z forward, +Y down). Its own inverse.
CV_TO_BLENDER = rot_x(math.pi)

# Applied to imported GLB mesh data to undo the importer's Y-up conversion.
GLTF_IMPORT_UNDO = rot_x(-math.pi / 2)


def camera_matrix(world_se3_cv):
    """``matrix_world`` of a Blender camera that sees what a CV camera at ``world_se3_cv`` sees."""
    return matmul(world_se3_cv, CV_TO_BLENDER)


def emitter_matrix(world_se3_light):
    """``matrix_world`` of a Blender spot / area light emitting along etendue's local +Z."""
    return matmul(world_se3_light, CV_TO_BLENDER)


def lens_mm(focal_px, width_px, sensor_width_mm):
    """Blender focal length for a pinhole of ``focal_px`` pixels over ``width_px`` pixels
    (``sensor_fit = HORIZONTAL``, square pixels)."""
    return focal_px * sensor_width_mm / width_px
