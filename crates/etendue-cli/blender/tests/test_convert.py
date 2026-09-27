import math
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from etendue_blender import convert as c  # noqa: E402


def apply(m, p):
    return [sum(m[i][k] * (p + [1.0])[k] for k in range(4)) for i in range(3)]


def direction(m, v):
    return [sum(m[i][k] * v[k] for k in range(3)) for i in range(3)]


def close(a, b, tol=1e-12):
    return all(abs(x - y) <= tol for x, y in zip(a, b))


class ConvertTest(unittest.TestCase):
    def test_cv_to_blender_is_an_involution(self):
        m = c.matmul(c.CV_TO_BLENDER, c.CV_TO_BLENDER)
        for i in range(4):
            self.assertTrue(close(m[i], c.identity()[i]))

    def test_camera_looks_where_the_cv_camera_looks(self):
        # A CV camera 1 m up, pitched 30° about world X.
        pose = c.matmul([[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 1.0], [0, 0, 0, 1]], c.rot_x(math.radians(30)))
        cam = c.camera_matrix(pose)
        # Blender camera forward is local −Z, up is local +Y; CV forward +Z, down +Y.
        self.assertTrue(close(direction(cam, [0, 0, -1]), direction(pose, [0, 0, 1])))
        self.assertTrue(close(direction(cam, [0, 1, 0]), direction(pose, [0, -1, 0])))
        self.assertTrue(close(apply(cam, [0, 0, 0]), [0, 0, 1.0]))
        # The round trip gives the CV pose back.
        back = c.matmul(cam, c.CV_TO_BLENDER)
        for i in range(4):
            self.assertTrue(close(back[i], pose[i]))

    def test_emitters_face_local_plus_z(self):
        light = c.emitter_matrix(c.identity())
        self.assertTrue(close(direction(light, [0, 0, -1]), [0, 0, 1]))

    def test_gltf_undo_inverts_the_importers_y_up_rotation(self):
        # Measured with Blender 5.1.1: an imported vertex (x, y, z) arrives as (x, −z, y).
        raw = [0.1, 0.2, 0.3]
        imported = apply(c.rot_x(math.pi / 2), raw)
        self.assertTrue(close(imported, [0.1, -0.3, 0.2]))
        self.assertTrue(close(apply(c.GLTF_IMPORT_UNDO, imported), raw))

    def test_lens_and_row_major(self):
        self.assertAlmostEqual(c.lens_mm(1800.0, 2048, 36.0), 1800.0 * 36.0 / 2048)
        m = c.from_row_major(list(range(16)))
        self.assertEqual(m[1], [4.0, 5.0, 6.0, 7.0])
        with self.assertRaises(ValueError):
            c.from_row_major([1, 2])


if __name__ == "__main__":
    unittest.main()
