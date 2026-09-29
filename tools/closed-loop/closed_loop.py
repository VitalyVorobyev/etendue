"""P5-1 closed loop: calibrate from etendue's analytic ground truth and compare with the truth.

    cargo run -p etendue-cli -- gt <scene.json> <scenario.json> -o <dir>
    uv run --locked --project tools/closed-loop tools/closed-loop/closed_loop.py <dir> [--camera ID]
    uv run … closed_loop.py <dir> --features <dir>/features.json   # image level (P5-1b)

Reads `<dir>/dataset.json`, `robot_poses.json` and `gt.json` (etendue-synth `dataset::emit`).
The observations are the visible ground-truth corners: target points in the target frame,
their analytic pixels. They go through calibration-rs's own solver (the `vision-calibration`
wheel), so this validates etendue's frames, robot poses and ground truth end to end, with no
detector in the loop. Gate G5.1 (docs/pivot/PLAN.md P5-1) on noise-free input.

Robot poses are read from `robot_poses.json` through the dataset's own column map and pose
convention, as calibration-rs's dataset runner would read them. `--camera` calibrates a single
camera of a rig dataset (single-camera hand-eye); by default a rig dataset is calibrated as a
rig.

`--features` takes the observations from `etendue detect` instead (render → detect → calibrate,
P5-1b): the detected pixel of every board point the detector labelled, in views it kept. The
truth it is compared with is still `gt.json`, and so are the limits, though G5.1 itself is
defined on noise-free input. `--noise-px σ` instead adds white Gaussian noise (σ per axis, seeded)
to the analytic pixels: what a detector with no bias and that RMS would give on these views.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path
from typing import Any

import numpy as np
import vision_calibration as vc

# Gate G5.1 on noise-free input.
GATE = {
    "mean_reproj_px": 0.05,
    "focal_rel": 1e-4,
    "principal_px": 0.05,
    "handeye_rot_deg": 0.01,
    "handeye_trans_mm": 0.05,
}


# ── SE(3) as (R, t), quaternions [x, y, z, w] ──────────────────────────────────


def rot(q: list[float]) -> np.ndarray:
    x, y, z, w = (float(v) for v in q)
    n = math.sqrt(x * x + y * y + z * z + w * w)
    x, y, z, w = x / n, y / n, z / n, w / n
    return np.array(
        [
            [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
            [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
            [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
        ]
    )


def quat(r: np.ndarray) -> list[float]:
    """Rotation matrix to [x, y, z, w] (Shepperd)."""
    t = np.trace(r)
    if t > 0:
        s = math.sqrt(t + 1.0) * 2
        w, x, y, z = 0.25 * s, (r[2, 1] - r[1, 2]) / s, (r[0, 2] - r[2, 0]) / s, (r[1, 0] - r[0, 1]) / s
    elif r[0, 0] > r[1, 1] and r[0, 0] > r[2, 2]:
        s = math.sqrt(1.0 + r[0, 0] - r[1, 1] - r[2, 2]) * 2
        w, x, y, z = (r[2, 1] - r[1, 2]) / s, 0.25 * s, (r[0, 1] + r[1, 0]) / s, (r[0, 2] + r[2, 0]) / s
    elif r[1, 1] > r[2, 2]:
        s = math.sqrt(1.0 + r[1, 1] - r[0, 0] - r[2, 2]) * 2
        w, x, y, z = (r[0, 2] - r[2, 0]) / s, (r[0, 1] + r[1, 0]) / s, 0.25 * s, (r[1, 2] + r[2, 1]) / s
    else:
        s = math.sqrt(1.0 + r[2, 2] - r[0, 0] - r[1, 1]) * 2
        w, x, y, z = (r[1, 0] - r[0, 1]) / s, (r[0, 2] + r[2, 0]) / s, (r[1, 2] + r[2, 1]) / s, 0.25 * s
    return [x, y, z, w]


class Iso:
    def __init__(self, r: np.ndarray, t: np.ndarray) -> None:
        self.r, self.t = r, np.asarray(t, dtype=float)

    @classmethod
    def wire(cls, w: dict[str, Any]) -> "Iso":
        return cls(rot(w["rotation"]), np.array(w["translation"], dtype=float))

    @classmethod
    def pose(cls, p: vc.Pose) -> "Iso":
        return cls(rot(list(p.rotation_xyzw)), np.array(p.translation_xyz, dtype=float))

    def __mul__(self, o: "Iso") -> "Iso":
        return Iso(self.r @ o.r, self.r @ o.t + self.t)

    def inv(self) -> "Iso":
        return Iso(self.r.T, -self.r.T @ self.t)

    def to_pose(self) -> vc.Pose:
        return vc.Pose(rotation_xyzw=tuple(quat(self.r)), translation_xyz=tuple(self.t))


def difference(estimate: Iso, truth: Iso) -> tuple[float, float]:
    """Rotation angle (deg) and translation distance (mm) between two poses."""
    d = truth.inv() * estimate
    # atan2 keeps precision near zero, where acos of the trace does not.
    r = d.r
    sin = 0.5 * float(np.linalg.norm([r[2, 1] - r[1, 2], r[0, 2] - r[2, 0], r[1, 0] - r[0, 1]]))
    angle = math.degrees(math.atan2(sin, (np.trace(r) - 1) / 2))
    return angle, float(np.linalg.norm(estimate.t - truth.t)) * 1e3


# ── Inputs ────────────────────────────────────────────────────────────────────


def robot_poses(directory: Path, dataset: dict[str, Any]) -> list[Iso]:
    """base_se3_gripper per capture, read through the dataset's column map and convention."""
    source = dataset["robot_poses"]
    convention = dataset["pose_convention"]
    if source["format"] != "json" or convention != {
        "transform": "t_base_tcp",
        "rotation_format": "quat_xyzw",
        "translation_units": "m",
    }:
        sys.exit(f"unsupported robot pose source/convention: {source} {convention}")
    columns = source["columns"]
    rows = json.loads((directory / source["path"]).read_text())
    return [
        Iso(
            rot([row[c] for c in columns["rotation"]]),
            np.array([row[columns["tx"]], row[columns["ty"]], row[columns["tz"]]], dtype=float),
        )
        for row in rows
    ]


def correspondences(view: dict[str, Any]) -> list[tuple[int, list[float]]]:
    """(point index, pixel) pairs of a view: the visible points of a `gt.json` view, or every
    point of a `features.json` view the detector kept."""
    if "status" in view:
        return [(p["point"], p["pixel"]) for p in view["points"]] if view["status"] == "ok" else []
    return [(p["point"], p["pixel"]) for p in view["points"] if p.get("occluded") is None]


def observation(
    gt: dict[str, Any], view: dict[str, Any], noise: np.random.Generator | None = None, sigma: float = 0.0
) -> vc.Observation | None:
    points = gt["target"]["points"]
    p3, p2 = [], []
    for point, pixel in correspondences(view):
        x, y = points[point]["position_m"]
        p3.append((x, y, 0.0))
        if noise is not None:
            pixel = [pixel[0] + noise.normal(0.0, sigma), pixel[1] + noise.normal(0.0, sigma)]
        p2.append(tuple(pixel))
    return vc.Observation(points_3d=p3, points_2d=p2) if len(p3) >= 4 else None


def observed_captures(gt: dict[str, Any], features: dict[str, Any] | None) -> list[dict[str, Any]]:
    """The captures whose views give the observations: `gt.json`'s, or `features.json`'s
    (checked to be the same captures and cameras, in the same order)."""
    if features is None:
        return gt["captures"]
    if features.get("version") != 1:
        sys.exit(f"unsupported features.json version {features.get('version')}")
    captures = features["captures"]
    same = len(captures) == len(gt["captures"]) and all(
        f["id"] == g["id"] and [v["camera"] for v in f["views"]] == [v["camera"] for v in g["views"]]
        for f, g in zip(captures, gt["captures"])
    )
    if not same:
        sys.exit("features.json does not match gt.json's captures and cameras")
    return captures


# ── Comparison ────────────────────────────────────────────────────────────────


def intrinsics_rows(estimated: vc.PinholeBrownConradyCamera, truth: dict[str, Any], name: str) -> list[tuple[str, float, float | None]]:
    k = truth["params"]["intrinsics"]
    e = estimated.intrinsics
    d_true = truth["params"]["distortion"]
    d = estimated.distortion
    rows: list[tuple[str, float, float | None]] = [
        (f"{name} fx rel", abs(e.fx - k["fx"]) / k["fx"], GATE["focal_rel"]),
        (f"{name} fy rel", abs(e.fy - k["fy"]) / k["fy"], GATE["focal_rel"]),
        (f"{name} cx px", abs(e.cx - k["cx"]), GATE["principal_px"]),
        (f"{name} cy px", abs(e.cy - k["cy"]), GATE["principal_px"]),
    ]
    for c in ("k1", "k2", "k3", "p1", "p2"):
        rows.append((f"{name} {c} abs", abs(getattr(d, c) - d_true[c]), None))
    return rows


def report(title: str, rows: list[tuple[str, float, float | None]]) -> bool:
    print(title)
    ok = True
    for label, value, gate in rows:
        verdict = "" if gate is None else ("PASS" if value <= gate else "FAIL")
        ok &= gate is None or value <= gate
        gate_text = "" if gate is None else f"≤ {gate:g}"
        print(f"  {label:<28} {value:12.3e}  {gate_text:<10} {verdict}")
    print(f"  G5.1: {'PASS' if ok else 'FAIL'}")
    return ok


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--camera", help="calibrate this camera alone (single-camera hand-eye)")
    parser.add_argument("--features", type=Path, help="observations from `etendue detect` (features.json)")
    parser.add_argument("--noise-px", type=float, default=0.0, help="white noise σ per axis on analytic pixels")
    parser.add_argument("--seed", type=int, default=0, help="seed of --noise-px")
    args = parser.parse_args()
    if args.features and args.noise_px:
        sys.exit("--noise-px applies to the analytic observations, not to --features")
    noise = np.random.default_rng(args.seed) if args.noise_px else None
    directory: Path = args.directory
    dataset = json.loads((directory / "dataset.json").read_text())
    gt = json.loads((directory / "gt.json").read_text())
    handeye = gt["handeye"]
    if handeye is None or handeye["type"] != "eye_in_hand":
        sys.exit("only eye-in-hand datasets are supported so far")
    poses = robot_poses(directory, dataset)
    if len(poses) != len(gt["captures"]):
        sys.exit(f"{len(poses)} robot poses for {len(gt['captures'])} captures")
    cameras = gt["cameras"]
    ids = [c["id"] for c in cameras]
    gripper_se3_rig = Iso.wire(handeye["gripper_se3_rig"])
    base_se3_target = Iso.wire(handeye["base_se3_target"])
    # A camera without a rig is its own rig frame.
    cam_se3_rig = [Iso.wire(c["cam_se3_rig"]) if c.get("cam_se3_rig") else Iso(np.eye(3), np.zeros(3)) for c in cameras]
    features = json.loads(args.features.read_text()) if args.features else None
    captures = observed_captures(gt, features)
    n_points = sum(len(correspondences(v)) for c in captures for v in c["views"])
    source = (
        f"detected points ({args.features})"
        if features
        else f"visible points (analytic{f', noise σ = {args.noise_px} px' if args.noise_px else ''})"
    )
    print(f"{directory}: {len(gt['captures'])} captures, cameras {ids}, {n_points} {source}")

    if args.camera is not None or len(cameras) == 1:
        name = args.camera or ids[0]
        i = ids.index(name)
        views = []
        for capture, base_se3_gripper in zip(captures, poses):
            obs = observation(gt, capture["views"][i], noise, args.noise_px)
            if obs is not None:
                views.append(vc.SingleCamHandeyeView(observation=obs, base_se3_gripper=base_se3_gripper.to_pose()))
        config = vc.SingleCamHandeyeCalibrationConfig()
        config.handeye_init.handeye_mode = "EyeInHand"
        result = vc.run_single_cam_handeye(vc.SingleCamHandeyeDataset(views=views), config)
        truth_gripper_se3_camera = gripper_se3_rig * cam_se3_rig[i].inv()
        rot_err, trans_err = difference(Iso.pose(result.gripper_se3_camera), truth_gripper_se3_camera)
        tr_err, tt_err = difference(Iso.pose(result.base_se3_target), base_se3_target)
        rows = [("mean reprojection px", result.mean_reproj_error, GATE["mean_reproj_px"])]
        rows += intrinsics_rows(result.camera, cameras[i], name)
        rows += [
            ("hand-eye rotation deg", rot_err, GATE["handeye_rot_deg"]),
            ("hand-eye translation mm", trans_err, GATE["handeye_trans_mm"]),
            ("base_se3_target rot deg", tr_err, None),
            ("base_se3_target trans mm", tt_err, None),
        ]
        return 0 if report(f"single-camera hand-eye ({name}, {len(views)} views)", rows) else 1

    # Rig: calibration-rs's rig frame is the reference camera's (index 0).
    views = []
    for capture, base_se3_gripper in zip(captures, poses):
        obs = [observation(gt, v, noise, args.noise_px) for v in capture["views"]]
        if any(o is not None for o in obs):
            views.append(vc.RigHandeyeView(cameras=obs, base_se3_gripper=base_se3_gripper.to_pose()))
    config = vc.RigHandeyeCalibrationConfig()
    config.handeye_init.handeye_mode = "EyeInHand"
    result = vc.run_rig_handeye(vc.RigHandeyeDataset(num_cameras=len(cameras), views=views), config)
    rig_se3_ref = cam_se3_rig[0].inv()
    rot_err, trans_err = difference(Iso.pose(result.gripper_se3_rig), gripper_se3_rig * rig_se3_ref)
    tr_err, tt_err = difference(Iso.pose(result.base_se3_target), base_se3_target)
    rows = [("mean reprojection px", result.mean_reproj_error, GATE["mean_reproj_px"])]
    for i, camera in enumerate(cameras):
        rows += intrinsics_rows(result.cameras[i], camera, camera["id"])
        if i > 0:
            r, t = difference(Iso.pose(result.cam_se3_rig[i]), cam_se3_rig[i] * rig_se3_ref)
            rows += [(f"{camera['id']} cam_se3_rig deg", r, None), (f"{camera['id']} cam_se3_rig mm", t, None)]
    rows += [
        ("hand-eye rotation deg", rot_err, GATE["handeye_rot_deg"]),
        ("hand-eye translation mm", trans_err, GATE["handeye_trans_mm"]),
        ("base_se3_target rot deg", tr_err, None),
        ("base_se3_target trans mm", tt_err, None),
    ]
    return 0 if report(f"rig hand-eye ({len(cameras)} cameras, {len(views)} views)", rows) else 1


if __name__ == "__main__":
    sys.exit(main())
