"""Unit tests of the closed-loop inputs (no solver run).

    uv run --locked --project tools/closed-loop python -m unittest discover -s tools/closed-loop
"""

from __future__ import annotations

import unittest

from closed_loop import correspondences, observed_captures

GT = {
    "target": {"points": [{"position_m": [0.0, 0.0]}, {"position_m": [0.02, 0.0]}]},
    "captures": [
        {
            "id": "cap_000",
            "views": [
                {
                    "camera": "cam",
                    "points": [
                        {"point": 0, "pixel": [10.0, 20.0]},
                        {"point": 1, "pixel": [30.0, 20.0], "occluded": "outside_image"},
                    ],
                }
            ],
        }
    ],
}


def features(status: str, camera: str = "cam") -> dict:
    view = {"camera": camera, "status": status, "points": [{"point": 1, "pixel": [30.1, 19.9]}]}
    return {"version": 1, "captures": [{"id": "cap_000", "views": [view]}]}


class Inputs(unittest.TestCase):
    def test_gt_views_give_their_visible_points(self) -> None:
        self.assertEqual(correspondences(GT["captures"][0]["views"][0]), [(0, [10.0, 20.0])])

    def test_feature_views_give_what_the_detector_kept(self) -> None:
        captures = observed_captures(GT, features("ok"))
        self.assertEqual(correspondences(captures[0]["views"][0]), [(1, [30.1, 19.9])])
        dropped = observed_captures(GT, features("partial"))
        self.assertEqual(correspondences(dropped[0]["views"][0]), [])

    def test_features_must_match_the_ground_truth(self) -> None:
        with self.assertRaises(SystemExit):
            observed_captures(GT, features("ok", camera="other"))


if __name__ == "__main__":
    unittest.main()


class Laser(unittest.TestCase):
    def test_plane_rows_are_sign_and_scale_free(self) -> None:
        from types import SimpleNamespace

        from closed_loop import plane_rows

        truth = {"normal": [0.0, 0.0, 1.0], "distance": -0.5}
        # The same plane with a flipped, doubled normal.
        estimated = SimpleNamespace(normal_xyz=[0.0, 0.0, -2.0], distance=1.0)
        (_, angle, _), (_, distance_mm, _) = plane_rows(estimated, truth, "cam")
        self.assertAlmostEqual(angle, 0.0, places=12)
        self.assertAlmostEqual(distance_mm, 0.0, places=9)
        # 1 mm and a small tilt away.
        tilted = SimpleNamespace(normal_xyz=[0.0, 1e-4, 1.0], distance=-0.501)
        (_, angle, _), (_, distance_mm, _) = plane_rows(tilted, truth, "cam")
        self.assertGreater(angle, 0.0)
        self.assertAlmostEqual(distance_mm, 1.0, delta=1e-3)

    def test_laser_pixels_keep_visible_points_only(self) -> None:
        from closed_loop import laser_pixels

        stripe = {"points": [{"pixel": [1.0, 2.0]}, {"pixel": [3.0, 4.0]}, {"pixel": [5.0, 6.0], "occluded": "x"}]}
        self.assertEqual(laser_pixels({"laser_points": [stripe]}), [(1.0, 2.0), (3.0, 4.0)])
        stripe["points"] = stripe["points"][:1]
        self.assertIsNone(laser_pixels({"laser_points": [stripe]}))

    def test_nominal_seed_rounds_like_a_datasheet(self) -> None:
        import math

        from closed_loop import nominal_seed

        camera = {
            "resolution": [1280, 1024],
            "params": {
                "intrinsics": {"fx": 2318.8, "fy": 2318.2},
                "sensor": {"type": "scheimpflug", "tilt_x": math.radians(3.4), "tilt_y": math.radians(0.7)},
            },
        }
        seed = nominal_seed([camera])
        self.assertEqual(seed["per_cam_intrinsics"][0], {"fx": 2300.0, "fy": 2300.0, "cx": 640.0, "cy": 512.0, "skew": 0.0})
        self.assertEqual(seed["per_cam_sensors"][0], {"tilt_x": math.radians(3), "tilt_y": 0.0})
