# Physical cameras

A scene camera is a calibrated pinhole: focal lengths, principal point, distortion and an
optional Scheimpflug tilt (calibration-rs's camera model). Give it an `optics` block and it
becomes a **physical camera**: a body with a sensor and a lens, focused at a distance. Its focal
lengths, skew and model tilt then come from the optics.

```json
{
  "id": "cam",
  "parent": "rig",
  "parent_se3_self": { "rotation": [0, 0, 0, 1], "translation": [0, 0, 0] },
  "resolution": [1936, 1216],
  "params": { "…": "the equivalent pinhole the optics give" },
  "optics": {
    "sensor": { "name": "Sony IMX174", "pixel_pitch_m": 5.86e-6 },
    "lens": { "name": "Kowa LM25HC", "focal_length_m": 0.025, "f_number": 4.0,
              "principal_gap_m": 0.0, "entrance_pupil_m": 0.0 },
    "focus_distance_m": 0.5,
    "tilt_rad": [0.0, 0.0]
  }
}
```

## The pose is the body

A physical camera is placed by its **mechanical frame**:
- **origin:** where the lens axis meets the image plane (the ⦵ mark on a camera body);
- **axes:** calibration-rs's axes, +Z toward the object, +X along the rows, +Y down.

Refocusing, stopping down or changing the lens never moves it.

The lens's points lie on that frame's Z axis. `v0` is the image distance at the focus setting.

| Point | Distance from the image plane |
|---|---|
| H′, the rear principal point | `v0` |
| H, the front principal point | `v0 + g` |
| E, the entrance pupil | `v0 + g + e` |
| E′, the exit pupil | `v0 − f·e / (e − f)` |

- **`focus_distance_m`** is measured from the image plane, as a lens's focus scale is.
- **The lens focuses as a unit:** it moves out to focus closer.
- **Focus breathing:** `v0`, and with it the focal length in pixels, grows as the focus comes
  closer. A 25 mm lens focused at 0.5 m has `fx` 5.6 % above its infinity value.

## It projects from the entrance pupil

Every chief ray passes through the entrance pupil, so that is the pinhole's centre of
projection. The equivalent pinhole about it has

```
fx = fy = m0 · (d − e) / pitch,     m0 = v0 / d
```

with `d` the object distance from H. A tilted sensor maps exactly to calibration-rs's
tilted-sensor model. With an entrance pupil away from H and a tilt about both axes, this takes a
small roll of the projection frame and a small skew.

## What stays, what is derived

- **Derived from the optics:** fx, fy, skew and the sensor model's tilt. They stay in `params`,
  because every tool that reads a scene, and calibration-rs, reads them there. `etendue
  validate` refuses a scene whose stored values differ from the derived ones by more than
  1e-9.
- **As built:** cx, cy and the distortion are properties of the built camera. The optics do
  not touch them.

The studio's kernel gives the derived values (`deriveOptics`).

## What the tools see

Everything that projects works with the camera at its entrance pupil:
- ground truth and calibration datasets;
- render jobs;
- the studio's camera views.

Things attached to the camera stay where the scene puts them. A calibration recovers the
projection frame, so the ground truth records where each physical camera's projection frame sits
in its body (`mech_se3_projection` in `gt.json`). A result can then be compared in either frame.

## Accuracy

The model is paraxial and aberration-free. An independent paraxial ray trace checks it:
- projection agrees to 1e-12 px;
- blur without tilt agrees exactly;
- with a tilted sensor, the plane of best focus is exact, and the blur's magnitude off that plane
  is first order: within `0.6·tan θ` of the traced ellipse, with θ the sensor's tilt.
