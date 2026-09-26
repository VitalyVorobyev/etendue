# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = [
#     "numpy==2.5.3",
#     "pin==4.1.0",
# ]
# [tool.uv]
# exclude-newer = "2026-09-26T00:00:00Z"
# ///
"""Generate the Pinocchio forward-kinematics fixtures for gate G1.1.

For every ``assets/robots/<id>/robot.json`` this loads ``robot.urdf`` with
``pinocchio.buildModelFromUrdf`` (kinematic model only, no meshes) and writes
``tools/fixtures/fk/<id>.json``:

* ``samples``: 10 000 joint vectors drawn uniformly within ``[lower, upper]``
  of robot.json (``numpy.random.default_rng(seed)``), in robot.json joint
  order, each with ``base_se3_tcp``;
* ``all_links_samples``: the first 100 of those, with ``base_se3_<link>`` for
  every link of the URDF.

Poses are ``base_se3_x = (oMf[base_link])^-1 * oMf[x]`` in the nalgebra
``Isometry3`` wire form ``{rotation: [qx, qy, qz, qw], translation: [...]}``.
Floats are written with Python ``repr`` (exact round trip).

Before writing, the script cross-checks Pinocchio against an independent
numpy URDF chain FK (driven by robot.json joint order, so it also validates
the q mapping) and ``oMi[parentJoint] * placement`` against ``oMf``.

Usage (from the repository root)::

    uv run tools/fixtures/fk_fixture.py            # all robots
    uv run tools/fixtures/fk_fixture.py --robot ur5e
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

import numpy as np
import pinocchio as pin

REPO_ROOT = Path(__file__).resolve().parents[2]
ROBOTS_DIR = REPO_ROOT / "assets" / "robots"
OUT_DIR = Path(__file__).resolve().parent / "fk"
SEED = 20260926
N_SAMPLES = 10_000
N_ALL_LINKS = 100
N_CROSS_CHECK = 1_000
CROSS_CHECK_TOL = 1e-12


# --------------------------------------------------------------------------- #
# Pinocchio model and q mapping
# --------------------------------------------------------------------------- #


def body_frame_id(model: pin.Model, link: str) -> int:
    if not model.existFrame(link, pin.FrameType.BODY):
        raise SystemExit(f"link {link!r} has no BODY frame in the Pinocchio model")
    return model.getFrameId(link, pin.FrameType.BODY)


def q_mapper(model: pin.Model, joint_names: list[str]):
    """Return f(q_robot_json) -> Pinocchio configuration vector."""
    slots = []
    for name in joint_names:
        if not model.existJointName(name):
            raise SystemExit(f"joint {name!r} not in the Pinocchio model")
        joint = model.joints[model.getJointId(name)]
        if joint.nq not in (1, 2) or joint.nv != 1:
            raise SystemExit(f"joint {name!r}: unsupported Pinocchio joint {joint.shortname()}")
        slots.append((joint.idx_q, joint.nq))
    movable = model.njoints - 1  # joint 0 is the universe
    if movable != len(joint_names):
        raise SystemExit(f"model has {movable} movable joints, robot.json lists {len(joint_names)}")
    neutral = pin.neutral(model)

    def to_pin(q: np.ndarray) -> np.ndarray:
        out = neutral.copy()
        for value, (idx, nq) in zip(q, slots):
            if nq == 1:
                out[idx] = value
            else:  # continuous (unbounded) joint: q is (cos, sin)
                out[idx], out[idx + 1] = math.cos(value), math.sin(value)
        return out

    return to_pin


def se3_json(pose: pin.SE3) -> dict:
    quat = pin.Quaternion(pose.rotation).coeffs()  # [x, y, z, w]
    return {"rotation": [float(v) for v in quat], "translation": [float(v) for v in pose.translation]}


# --------------------------------------------------------------------------- #
# Independent numpy FK (URDF chain from robot.json joint order)
# --------------------------------------------------------------------------- #


def _rpy(r: float, p: float, y: float) -> np.ndarray:
    cr, sr, cp, sp, cy, sy = math.cos(r), math.sin(r), math.cos(p), math.sin(p), math.cos(y), math.sin(y)
    return np.array(
        [
            [cy * cp, cy * sp * sr - sy * cr, cy * sp * cr + sy * sr],
            [sy * cp, sy * sp * sr + cy * cr, sy * sp * cr - cy * sr],
            [-sp, cp * sr, cp * cr],
        ]
    )


def _axis_angle(axis: np.ndarray, angle: float) -> np.ndarray:
    k = axis / np.linalg.norm(axis)
    kx = np.array([[0, -k[2], k[1]], [k[2], 0, -k[0]], [-k[1], k[0], 0]])
    return np.eye(3) + math.sin(angle) * kx + (1 - math.cos(angle)) * (kx @ kx)


def numpy_chain_fk(urdf_path: Path, base: str, tip: str, joint_names: list[str]):
    """Independent base_T_tip = (root_T_base)^-1 * root_T_tip from the raw URDF.

    ``base`` may be a sibling branch of the arm (REP-199 ``base``); the root ->
    base path must be fixed-only and the movable joints on root -> tip must be
    exactly robot.json's joints, in order.
    """
    root = ET.parse(urdf_path).getroot()
    by_child = {j.find("child").get("link"): j for j in root.findall("joint")}

    def steps_to(link: str):
        chain = []
        while link in by_child:
            chain.append(by_child[link])
            link = by_child[link].find("parent").get("link")
        steps = []
        for j in reversed(chain):
            origin = j.find("origin")
            xyz = [float(v) for v in (origin.get("xyz", "0 0 0") if origin is not None else "0 0 0").split()]
            rpy = [float(v) for v in (origin.get("rpy", "0 0 0") if origin is not None else "0 0 0").split()]
            axis_el = j.find("axis")
            axis = np.array([float(v) for v in (axis_el.get("xyz") if axis_el is not None else "1 0 0").split()])
            steps.append((j.get("name"), j.get("type"), np.array(xyz), _rpy(*rpy), axis))
        return steps

    base_steps, tip_steps = steps_to(base), steps_to(tip)
    if any(kind != "fixed" for _, kind, *_ in base_steps):
        raise SystemExit(f"root -> {base} path has non-fixed joints")
    movable = [name for name, kind, *_ in tip_steps if kind != "fixed"]
    if movable != joint_names:
        raise SystemExit(f"numpy chain joints {movable} != robot.json joints {joint_names}")
    index = {name: i for i, name in enumerate(joint_names)}

    def root_pose(steps, q: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
        rot, pos = np.eye(3), np.zeros(3)
        for name, kind, xyz, r_origin, axis in steps:
            pos, rot = pos + rot @ xyz, rot @ r_origin
            if kind in ("revolute", "continuous"):
                rot = rot @ _axis_angle(axis, q[index[name]])
            elif kind == "prismatic":
                pos = pos + rot @ (axis / np.linalg.norm(axis) * q[index[name]])
            elif kind != "fixed":
                raise SystemExit(f"unsupported joint type {kind}")
        return rot, pos

    def fk(q: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
        r_base, p_base = root_pose(base_steps, q)
        r_tip, p_tip = root_pose(tip_steps, q)
        return r_base.T @ r_tip, r_base.T @ (p_tip - p_base)

    return fk


# --------------------------------------------------------------------------- #
# Fixture
# --------------------------------------------------------------------------- #


def build_fixture(robot_dir: Path) -> Path:
    manifest = json.loads((robot_dir / "robot.json").read_text())
    rid = manifest["id"]
    urdf_path = robot_dir / manifest["urdf"]
    urdf_bytes = urdf_path.read_bytes()
    base, tcp = manifest["base_link"], manifest["tcp_link"]
    joint_names = [j["name"] for j in manifest["joints"]]
    lower = np.array([j["lower"] for j in manifest["joints"]], dtype=np.float64)
    upper = np.array([j["upper"] for j in manifest["joints"]], dtype=np.float64)

    model = pin.buildModelFromUrdf(str(urdf_path))
    data = model.createData()
    to_pin = q_mapper(model, joint_names)
    links = [link.get("name") for link in ET.fromstring(urdf_bytes).findall("link")]
    link_frames = {link: body_frame_id(model, link) for link in links}
    base_id, tcp_id = link_frames[base], link_frames[tcp]

    rng = np.random.default_rng(SEED)
    qs = rng.uniform(lower, upper, size=(N_SAMPLES, len(joint_names)))

    fk_np = numpy_chain_fk(urdf_path, base, tcp, joint_names)
    worst_chain = worst_frames = 0.0
    samples, all_links = [], []
    for k, q in enumerate(qs):
        pin.framesForwardKinematics(model, data, to_pin(q))
        base_inv = data.oMf[base_id].inverse()
        tcp_pose = base_inv * data.oMf[tcp_id]
        samples.append({"q": q.tolist(), "base_se3_tcp": se3_json(tcp_pose)})

        if k < N_CROSS_CHECK:
            rot, pos = fk_np(q)
            worst_chain = max(
                worst_chain,
                float(np.abs(rot - tcp_pose.rotation).max()),
                float(np.abs(pos - tcp_pose.translation).max()),
            )
            for fid in link_frames.values():
                frame = model.frames[fid]
                manual = data.oMi[frame.parentJoint] * frame.placement
                worst_frames = max(worst_frames, float(np.abs(manual.homogeneous - data.oMf[fid].homogeneous).max()))

        if k < N_ALL_LINKS:
            all_links.append(
                {
                    "q": q.tolist(),
                    "base_se3_link": {link: se3_json(base_inv * data.oMf[fid]) for link, fid in link_frames.items()},
                }
            )

    print(
        f"[{rid}] cross-check on {N_CROSS_CHECK} samples: numpy chain vs pinocchio {worst_chain:.2e}, "
        f"oMi*placement vs oMf {worst_frames:.2e} (tol {CROSS_CHECK_TOL:.0e})"
    )
    if worst_chain > CROSS_CHECK_TOL or worst_frames > CROSS_CHECK_TOL:
        raise SystemExit(f"[{rid}] cross-check failed")

    fixture = {
        "robot": rid,
        "urdf_sha256": hashlib.sha256(urdf_bytes).hexdigest(),
        "generator": {"library": "pinocchio", "version": pin.__version__, "numpy": np.__version__},
        "seed": SEED,
        "base_link": base,
        "tcp_link": tcp,
        "joint_names": joint_names,
        "samples": samples,
        "all_links_samples": all_links,
    }
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    out = OUT_DIR / f"{rid}.json"
    out.write_text(json.dumps(fixture, separators=(",", ":")) + "\n")
    print(f"[{rid}] wrote {out.relative_to(REPO_ROOT)} ({out.stat().st_size / 1e6:.2f} MB, {len(links)} links)")
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate Pinocchio FK fixtures (gate G1.1).")
    parser.add_argument("--robot", action="append", help="only this robot id (repeatable)")
    opts = parser.parse_args()
    robot_dirs = sorted(p.parent for p in ROBOTS_DIR.glob("*/robot.json"))
    if opts.robot:
        robot_dirs = [d for d in robot_dirs if d.name in opts.robot]
        missing = set(opts.robot) - {d.name for d in robot_dirs}
        if missing:
            raise SystemExit(f"no assets/robots/<id>/robot.json for {sorted(missing)}")
    if not robot_dirs:
        raise SystemExit("no robots found; run tools/robot-assets/build.py first")
    for robot_dir in robot_dirs:
        build_fixture(robot_dir)
    return 0


if __name__ == "__main__":
    sys.exit(main())
