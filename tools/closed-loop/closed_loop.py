"""P5-1 / P5-3 closed loop: calibrate from etendue's analytic ground truth, compare with the truth.

    cargo run -p etendue-cli -- gt <scene.json> <scenario.json> -o <dir>
    uv run --locked --project tools/closed-loop tools/closed-loop/closed_loop.py <dir> [--camera ID]

Reads `<dir>/dataset.json`, `robot_poses.json` and `gt.json` (etendue-synth `dataset::emit`).
The observations are the visible ground-truth corners: target points in the target frame,
their analytic pixels. They go through calibration-rs's own solver (the `vision-calibration`
wheel), so this validates etendue's frames, robot poses and ground truth end to end, with no
detector in the loop. Gate G5.1 (docs/pivot/PLAN.md P5-1) on noise-free input.

Robot poses are read from `robot_poses.json` through the dataset's own column map and pose
convention, as calibration-rs's dataset runner would read them. `--camera` calibrates a single
camera of a rig dataset (single-camera hand-eye); by default a rig dataset is calibrated as a
rig.

A dataset with a laser (topology `rig_handeye_laserline`) adds each view's visible laser-stripe
pixels and runs calibration-rs's joint rig hand-eye laserline problem. Gate G5.3 (P5-3) compares
the estimated laser plane with the truth in each camera's frame; the G5.1 limits apply to the
rest. Scheimpflug cameras are seeded as calibration-rs ADR 0022 prescribes ([`nominal_seed`]).
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

# Gate G5.3 on noise-free input.
LASER_GATE = {
    "plane_normal_deg": 0.01,
    "plane_distance_mm": 0.05,
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


def observation(gt: dict[str, Any], view: dict[str, Any]) -> vc.Observation | None:
    points = gt["target"]["points"]
    p3, p2 = [], []
    for p in view["points"]:
        if p.get("occluded") is None:
            x, y = points[p["point"]]["position_m"]
            p3.append((x, y, 0.0))
            p2.append(tuple(p["pixel"]))
    return vc.Observation(points_3d=p3, points_2d=p2) if len(p3) >= 4 else None


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


def plane_rows(estimated: vc.LaserlinePlane, truth: dict[str, Any], name: str) -> list[tuple[str, float, float | None]]:
    """Normal angle and offset difference of two planes n·p + d = 0 in one frame (sign-free)."""
    n_e, d_e = np.asarray(estimated.normal_xyz, dtype=float), float(estimated.distance)
    n_t, d_t = np.asarray(truth["normal"], dtype=float), float(truth["distance"])
    n_e, n_t = n_e / np.linalg.norm(n_e), n_t / np.linalg.norm(n_t)
    if float(n_e @ n_t) < 0:
        n_e, d_e = -n_e, -d_e
    angle = math.degrees(math.atan2(float(np.linalg.norm(np.cross(n_e, n_t))), float(n_e @ n_t)))
    return [
        (f"{name} laser normal deg", angle, LASER_GATE["plane_normal_deg"]),
        (f"{name} laser distance mm", abs(d_e - d_t) * 1e3, LASER_GATE["plane_distance_mm"]),
    ]


def report(title: str, rows: list[tuple[str, float, float | None]], gate: str = "G5.1") -> bool:
    print(title)
    ok = True
    for label, value, limit in rows:
        verdict = "" if limit is None else ("PASS" if value <= limit else "FAIL")
        ok &= limit is None or value <= limit
        limit_text = "" if limit is None else f"≤ {limit:g}"
        print(f"  {label:<28} {value:12.3e}  {limit_text:<10} {verdict}")
    print(f"  {gate}: {'PASS' if ok else 'FAIL'}")
    return ok


def laser_pixels(view: dict[str, Any]) -> list[tuple[float, float]] | None:
    """The view's visible laser-stripe pixels (the scene's single laser), or None if too few."""
    [stripe] = view["laser_points"]
    pixels = [tuple(p["pixel"]) for p in stripe["points"] if p.get("occluded") is None]
    return pixels if len(pixels) >= 2 else None


def scheimpflug(cameras: list[dict[str, Any]]) -> bool:
    return any(c["params"]["sensor"]["type"] == "scheimpflug" for c in cameras)


def tilt_rows(estimated: vc.ScheimpflugSensor, truth: dict[str, Any], name: str) -> list[tuple[str, float, float | None]]:
    sensor = truth["params"]["sensor"]
    true_x, true_y = (sensor["tilt_x"], sensor["tilt_y"]) if sensor["type"] == "scheimpflug" else (0.0, 0.0)
    return [
        (f"{name} tilt_x deg", abs(math.degrees(estimated.tilt_x - true_x)), None),
        (f"{name} tilt_y deg", abs(math.degrees(estimated.tilt_y - true_y)), None),
    ]


def nominal_seed(cameras: list[dict[str, Any]]) -> dict[str, Any]:
    """Seeds for a Scheimpflug rig, as a user would enter them from datasheets.

    calibration-rs's supported Scheimpflug route is seeded (its ADR 0022; unseeded init is
    experimental). The seed is the focal length rounded to 100 px, the principal point at the
    image centre, and the mount tilt about x rounded to a whole degree, with no tilt about y.
    """
    intrinsics, sensors = [], []
    for c in cameras:
        k = c["params"]["intrinsics"]
        w, h = c["resolution"]
        f = round((k["fx"] + k["fy"]) / 2, -2)
        intrinsics.append({"fx": f, "fy": f, "cx": w / 2, "cy": h / 2, "skew": 0.0})
        sensor = c["params"]["sensor"]
        tilt = sensor["tilt_x"] if sensor["type"] == "scheimpflug" else 0.0
        sensors.append({"tilt_x": math.radians(round(math.degrees(tilt))), "tilt_y": 0.0})
    return {"per_cam_intrinsics": intrinsics, "per_cam_distortion": None, "per_cam_sensors": sensors}


class SeededLaserlineConfig(vc.RigHandeyeLaserlineCalibrationConfig):
    """The joint config with `handeye.manual_init` set.

    The wheel does not expose `manual_init` yet (calibration-rs 0.8.3: "still Rust-only"), but the
    Rust config it is deserialised into accepts it, so it is added to the payload.
    """

    seed: dict[str, Any] | None = None

    def to_payload(self) -> dict[str, Any]:
        payload = super().to_payload()
        if self.seed is not None:
            payload["handeye"]["manual_init"] = self.seed
        return payload


def laser_loop(
    gt: dict[str, Any],
    poses: list[Iso],
    gripper_se3_rig: Iso,
    base_se3_target: Iso,
    cam_se3_rig: list[Iso],
) -> int:
    """P5-3: the joint rig hand-eye laserline problem, gated on G5.3 (and G5.1's limits)."""
    cameras = gt["cameras"]
    [laser] = gt["lasers"]
    views = []
    for capture, base_se3_gripper in zip(gt["captures"], poses):
        obs = [observation(gt, v) for v in capture["views"]]
        if any(o is not None for o in obs):
            views.append(
                vc.RigHandeyeLaserlineView(
                    cameras=obs,
                    laser_pixels=[laser_pixels(v) for v in capture["views"]],
                    base_se3_gripper=base_se3_gripper.to_pose(),
                )
            )
    config = SeededLaserlineConfig()
    config.handeye.handeye_init.handeye_mode = "EyeInHand"
    if scheimpflug(cameras):
        config.seed = nominal_seed(cameras)
        config.handeye.sensor = vc.ScheimpflugSensorMode(refine_scheimpflug_in_rig_ba=True)
        config.handeye.rig.refine_intrinsics_in_rig_ba = True
    result = vc.run_rig_handeye_laserline(vc.RigHandeyeLaserlineDataset(num_cameras=len(cameras), views=views), config)
    rig_se3_ref = cam_se3_rig[0].inv()
    rot_err, trans_err = difference(Iso.pose(result.gripper_se3_rig), gripper_se3_rig * rig_se3_ref)
    tr_err, tt_err = difference(Iso.pose(result.base_se3_target), base_se3_target)
    rows = [("mean reprojection px", result.mean_reproj_error, GATE["mean_reproj_px"])]
    for i, camera in enumerate(cameras):
        rows += intrinsics_rows(result.cameras[i], camera, camera["id"])
        if scheimpflug(cameras):
            rows += tilt_rows(result.sensors[i], camera, camera["id"])
        if i > 0:
            r, t = difference(Iso.pose(result.cam_se3_rig[i]), cam_se3_rig[i] * rig_se3_ref)
            rows += [(f"{camera['id']} cam_se3_rig deg", r, None), (f"{camera['id']} cam_se3_rig mm", t, None)]
        plane = next(p for p in laser["camera_planes"] if p["frame"] == camera["id"])
        rows += plane_rows(result.laser_planes_cam[i], plane, camera["id"])
    rows += [
        ("hand-eye rotation deg", rot_err, GATE["handeye_rot_deg"]),
        ("hand-eye translation mm", trans_err, GATE["handeye_trans_mm"]),
        ("base_se3_target rot deg", tr_err, None),
        ("base_se3_target trans mm", tt_err, None),
    ]
    n_laser = sum(len(px) for v in views for px in v.laser_pixels if px is not None)
    title = f"rig hand-eye laserline ({len(cameras)} cameras, {len(views)} views, {n_laser} laser pixels)"
    return 0 if report(title, rows, gate="G5.3") else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--camera", help="calibrate this camera alone (single-camera hand-eye)")
    args = parser.parse_args()
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
    n_points = sum(1 for c in gt["captures"] for v in c["views"] for p in v["points"] if p.get("occluded") is None)
    print(f"{directory}: {len(gt['captures'])} captures, cameras {ids}, {n_points} visible points")

    if gt.get("lasers"):
        if args.camera is not None:
            sys.exit("--camera does not apply to a laser dataset (rig hand-eye laserline)")
        return laser_loop(gt, poses, gripper_se3_rig, base_se3_target, cam_se3_rig)

    if args.camera is not None or len(cameras) == 1:
        name = args.camera or ids[0]
        i = ids.index(name)
        views = []
        for capture, base_se3_gripper in zip(gt["captures"], poses):
            obs = observation(gt, capture["views"][i])
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
    for capture, base_se3_gripper in zip(gt["captures"], poses):
        obs = [observation(gt, v) for v in capture["views"]]
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
