"""Write the P5-1 closed-loop scenario: `examples/closed_loop_ur5e/scenario.json`.

    uv run --locked --project tools/closed-loop tools/closed-loop/make_scenario.py

Twenty viewpoints of the board: the rig looks at the board centre from a cone of directions
(tilt 8–25° from vertical, spread in azimuth), at 0.45–0.55 m, with the rig rolled about
its optical axis by up to ±25°. Deterministic (no RNG). The tool pose of each viewpoint is
`base_se3_rig · (tool_se3_rig)⁻¹`, with the mount read from the scene.
"""

from __future__ import annotations

import json
import math
from pathlib import Path

import numpy as np

from closed_loop import Iso, quat

ROOT = Path(__file__).resolve().parents[2]
EXAMPLE = ROOT / "examples" / "closed_loop_ur5e"
N = 20


def look_at(eye: np.ndarray, target: np.ndarray, roll: float) -> Iso:
    """A CV-convention frame at `eye` whose +Z points at `target`, rolled about +Z."""
    z = target - eye
    z /= np.linalg.norm(z)
    # +Y of the image roughly along world −X before roll (image "down" away from the robot).
    down = np.array([-1.0, 0.0, 0.0])
    x = np.cross(down, z)
    x /= np.linalg.norm(x)
    y = np.cross(z, x)
    r = np.column_stack([x, y, z])
    c, s = math.cos(roll), math.sin(roll)
    return Iso(r @ np.array([[c, -s, 0], [s, c, 0], [0, 0, 1]]), eye)


def main() -> None:
    scene = json.loads((EXAMPLE / "scene.json").read_text())
    rig = next(r for r in scene["rigs"] if r["id"] == "rig")
    tool_se3_rig = Iso.wire(rig["parent_se3_self"])
    board = next(t for t in scene["targets"] if t["id"] == "board")
    centre = np.array(board["parent_se3_self"]["translation"], dtype=float)

    steps = []
    for k in range(N):
        # Golden-angle azimuths, tilts cycling through 8°–25°, distances 0.45–0.55 m.
        azimuth = k * math.pi * (3 - math.sqrt(5))
        tilt = math.radians(8 + 17 * ((k * 7) % N) / (N - 1))
        distance = 0.45 + 0.10 * ((k * 3) % N) / (N - 1)
        roll = math.radians(25 * math.sin(1.7 * k))
        direction = np.array(
            [math.sin(tilt) * math.cos(azimuth), math.sin(tilt) * math.sin(azimuth), math.cos(tilt)]
        )
        base_se3_rig = look_at(centre + distance * direction, centre, roll)
        base_se3_tool = base_se3_rig * tool_se3_rig.inv()
        steps.append(
            {
                "type": "ptp_pose",
                "robot": "ur5e",
                "base_se3_tool": {
                    "rotation": quat(base_se3_tool.r),
                    "translation": base_se3_tool.t.tolist(),
                },
                "speed_scale": 0.5,
            }
        )
        steps.append({"type": "capture"})

    scenario = {
        "version": 1,
        "dt": 0.01,
        "description": "P5-1 closed loop: 20 viewpoints of the board at 0.45–0.55 m "
        "(tools/closed-loop/make_scenario.py).",
        "steps": steps,
    }
    (EXAMPLE / "scenario.json").write_text(json.dumps(scenario, indent=2) + "\n")
    print(f"wrote {N} viewpoints to {EXAMPLE / 'scenario.json'}")


if __name__ == "__main__":
    main()
