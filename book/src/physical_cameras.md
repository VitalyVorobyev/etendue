# Physical cameras and the thick lens

A calibrated camera is a pinhole: focal lengths in pixels, a principal point, distortion. That
is what a calibration recovers, but it is not what you buy or mount. You buy a sensor and a
lens, set the aperture and the focus, and bolt the body to a bracket. A **physical camera** is
etendue's model of that hardware. It derives the pinhole from the parts, so you can ask design
questions before anything exists:

- How does the focal length in pixels change when I refocus from 2 m to 0.5 m?
- Where is the centre of projection, relative to the camera body, that my hand-eye
  calibration will report?
- How much blur does a point 50 mm behind the focus get at f/4?

## The parts

A scene camera becomes physical when it has an `optics` block:

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

| Field | Symbol | Meaning |
|---|---|---|
| `pixel_pitch_m` | $p$ | the sensor's pixel pitch |
| `focal_length_m` | $f$ | the lens's effective focal length |
| `f_number` | $N$ | the f-number; the entrance pupil's diameter is $f/N$ |
| `principal_gap_m` | $g$ | how far the front principal point H lies in front of the rear one H′ (signed, positive toward the object; 0 for a thin lens) |
| `entrance_pupil_m` | $e$ | where the entrance pupil E lies relative to H (signed, positive toward the object; 0 puts it on H) |
| `focus_distance_m` | $D$ | the distance from the image plane to the plane in focus, as the lens's focus scale reads it |
| `tilt_rad` | $\tau_x, \tau_y$ | the sensor's tilt about the image-plane datum, in calibration-rs's convention (see [Camera models](models/camera_models.md)) |

Lens data sheets rarely give $g$ and $e$. Leave them at 0 and the lens is a thin lens with its
pupil on it; that is what the library's lenses do. The model still breathes with focus (below).
The two values move the centre of projection and the pupils, and change the focal length in
pixels by a few per cent; the worked example below shows how much.

## The pose is the body

A physical camera is placed by its **mechanical frame** M:

- **origin:** the image-plane datum S, where the lens axis meets the image plane (the ⦵ mark on
  a camera body);
- **axes:** calibration-rs's: +Z along the lens axis toward the object, +X along the rows, +Y
  down.

Refocusing, stopping down or changing the lens never moves M. Everything else sits on M's z axis:

<svg viewBox="0 0 640 170" role="img" aria-label="Cardinal points of a physical camera on its axis" style="max-width:640px;width:100%;color:currentColor">
  <g stroke="currentColor" fill="none" stroke-width="1.5">
    <line x1="20" y1="90" x2="625" y2="90" stroke-dasharray="4 3"/>
    <line x1="40" y1="45" x2="40" y2="135" stroke-width="4"/>
    <line x1="200" y1="55" x2="200" y2="125"/>
    <line x1="250" y1="55" x2="250" y2="125"/>
    <line x1="290" y1="65" x2="290" y2="115" stroke-dasharray="3 2"/>
    <line x1="600" y1="40" x2="600" y2="140"/>
    <path d="M40 150 L600 150 M40 145 L40 155 M600 145 L600 155"/>
    <path d="M40 25 L200 25 M40 20 L40 30 M200 20 L200 30"/>
    <path d="M250 25 L600 25 M250 20 L250 30 M600 20 L600 30"/>
  </g>
  <g fill="currentColor" font-family="sans-serif" font-size="13" text-anchor="middle">
    <text x="40" y="168">S (sensor)</text>
    <text x="200" y="140">H′</text>
    <text x="250" y="140">H</text>
    <text x="290" y="130">E</text>
    <text x="600" y="168">plane in focus</text>
    <text x="320" y="146">D (focus distance)</text>
    <text x="120" y="20">v₀</text>
    <text x="425" y="20">d</text>
    <text x="225" y="50">g</text>
    <text x="270" y="60">e</text>
  </g>
</svg>

| Point | Distance from the image plane S |
|---|---|
| H′, the rear principal point | $v_0$ |
| H, the front principal point | $v_0 + g$ |
| E, the entrance pupil | $v_0 + g + e$ |
| E′, the exit pupil | $v_0 - \dfrac{f\,e}{e - f}$ |
| F′ and F, the focal points | $v_0 - f$ and $v_0 + g + f$ |

## Focusing

The lens focuses as a unit: the whole lens moves out to focus closer, and its focal length does
not change. The image distance $v_0$ (H′ to the sensor) and the object distance $d$ (H to the
plane in focus) are conjugate, and together with the gap they make up the focus distance:

$$
\frac{1}{d} + \frac{1}{v_0} = \frac{1}{f}, \qquad d + g + v_0 = D .
$$

With $T = D - g$:

$$
v_0 = \frac{T - \sqrt{T^2 - 4fT}}{2}, \qquad d = T - v_0, \qquad m_0 = \frac{v_0}{d}.
$$

A real solution needs $T \ge 4f$: a lens cannot focus closer than four focal lengths (plus the
gap) from its image plane, and etendue refuses a focus distance below that. $m_0$ is the lateral
magnification at the focus setting.

## The equivalent pinhole

Every chief ray passes through the centre of the entrance pupil, so **the pinhole's centre of
projection is E**, not a principal point. On the image side the chief rays leave the exit pupil
E′, their slopes divided by the pupil magnification $m_p = f / (f - e)$. For an untilted sensor
the result is a pinhole with

$$
f_x = f_y = \frac{m_0\,(d - e)}{p} .
$$

For a thin lens with its pupil on it ($g = e = 0$) this is simply $v_0 / p$: the image distance,
not the focal length, divided by the pitch. That is **focus breathing**: as the focus comes
closer, $v_0$ grows and with it the focal length in pixels.

**Worked example.** A 25 mm lens at f/4 on a 5.86 µm sensor, thin, focused at $D = 0.5$ m:

| | Focus at 0.5 m | Focus at 2 m | At infinity |
|---|---|---|---|
| $v_0$ (H′ to sensor) | 26.393 mm | 25.321 mm | 25.000 mm |
| $d$ (H to the plane in focus) | 473.607 mm | 1974.679 mm | ∞ |
| magnification $m_0$ | 0.0557 | 0.0128 | 0 |
| $f_x$ | 4503.96 px | 4320.92 px | 4266.21 px ($f/p$) |

Refocusing from infinity to 0.5 m adds 5.6 % to $f_x$. A calibration done at one focus is wrong
by that much at another; a camera model that takes $f_x = f/p$ is wrong by it at any finite
focus. The entrance pupil (the centre of projection) is 26.39 mm in front of the image plane at
0.5 m, and 25.32 mm at 2 m.

The same lens with a 5 mm principal-plane gap and its entrance pupil 10 mm behind H ($g = 5$ mm,
$e = -10$ mm), still focused at 0.5 m, has $v_0 = 26.409$ mm, $d = 468.591$ mm and
$f_x = 4602.82$ px. Its points sit at H′ 26.41, H 31.41, E 21.41 and E′ 19.27 mm from the image
plane, and its pupil magnification is $m_p = 0.714$.

### A tilted sensor

A tilted sensor makes the map from the projection frame to pixels a homography, and etendue
decomposes it exactly into calibration-rs's tilted-sensor model, $K\,H(\tau')$:

- with $m_p = 1$ (the pupil on H) the model tilt $\tau'$ equals the physical tilt, and $K$ is
  the plain pinhole;
- for a tilt about one axis, $\tan\tau' = \tan\tau / m_p$, and that axis's focal length scales by
  $\cos\tau' / \cos\tau$. With $e = -10$ mm and a 3° tilt about x, the example above gets
  $f_x = 4599.06$ px and $f_y = 4593.02$ px;
- for a tilt about both axes, calibration-rs's upper-triangular $K$ cannot hold the result
  directly. etendue rolls the projection frame by a small angle about the axis, which leaves a
  small skew.

## Derived and as built

- **Derived from the optics:** `fx`, `fy`, `skew` and the sensor's model tilt. They are still
  stored in `params`, because every tool that reads a scene, and calibration-rs, reads them
  there. `etendue validate` refuses a scene whose stored values differ from the derived ones by
  more than one part in $10^9$, so the two cannot drift apart. The studio derives them for you.
- **As built:** `cx`, `cy` and the distortion. The paraxial optics do not determine them, and the
  optics do not touch them.

Other figures follow from the same parts, and the Optics workspace shows them:

- the **working f-number** $N_w = N\,(1 + m_0 / m_p)$, the cone that reaches the sensor (4.22
  for the example at 0.5 m), which sets diffraction (see [Defocus and the blur
  budget](models/blur.md));
- the **exit pupil diameter** $m_p f / N$;
- the **object pixel**, the size on the object of one pixel at the image centre: 105 µm at
  0.5 m for the example.

Try it: change the focal length, f-number, focus distance, gap and pupil position. Watch $f_x$
grow as the focus comes closer (breathing), and the cardinal points move together while the
image plane stays put.

<div class="etendue-demo" data-demo="optics"></div>

## What the tools see

Everything that projects (ground truth, calibration datasets, render jobs, the studio's camera
views) works with the camera at its entrance pupil: etendue resolves each physical camera into
its equivalent pinhole at E, and re-expresses whatever is attached to the camera so that it stays
where the scene put it. A calibration recovers that projection frame, not the body. The ground
truth therefore records, for each physical camera, where its projection frame sits in its body,
`mech_se3_projection` in `gt.json`. For the example at 0.5 m that is a translation of 26.39 mm
along +Z. A hand-eye result can then be compared with a mount drawing in either frame.

## Assumptions and limits

- **Paraxial and aberration-free.** No distortion is derived (it is as built), no field
  curvature, no spherical or chromatic aberration, no vignetting.
- **Unit focusing only.** The whole lens moves to focus. Internal-focus lenses, whose focal
  length changes with focus, are not modelled.
- **Pupils are points on the axis**, with the aperture a circle of diameter $f/N$ at the entrance
  pupil.
- **Accuracy.** An independent paraxial ray trace (Gaussian conjugates and line–plane
  intersections only) checks the model over five lenses (thin, thick, crossed principal planes,
  the pupil in front of and behind H) and five sensor tilts, at 200 points each: the projection
  agrees to $8.3 \times 10^{-13}$ px, the blur without tilt agrees exactly, and with a tilted
  sensor the plane of best focus is exact while the blur's size off it is first order (within
  $0.54\tan\theta$ of the traced value, $\theta$ the sensor's tilt; about 2 % at 0.08 rad).

API: `etendue-core` `optics::camera::PhysicalCamera`; `@etendue/wasm` `deriveOptics`.
