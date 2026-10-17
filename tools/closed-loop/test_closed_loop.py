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
